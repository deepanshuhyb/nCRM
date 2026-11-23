# ncrm architecture

How the system is put together, how its parts connect, and how a request
travels through it. Things marked *(planned)* are designed but not built.

## The big picture

```
                         ┌───────────────────────────────────────────┐
  Owner / staff          │ nginx (TLS, X-Real-IP)          (planned) │
  browser ── HTTPS ─────▶│ the only thing exposed to the internet    │
                         └──────────────────┬────────────────────────┘
                                            │
                         ┌──────────────────▼────────────────────────┐
                         │ Dashboard: Next.js 16 (web/)              │
                         │  pages: server-rendered per request, CSP  │
                         │  /api/*: same-origin gateway to the API   │
                         └──────────────────┬────────────────────────┘
                                            │ HTTP on the private network
                                            │ (cookie, Origin, X-Real-IP)
                         ┌──────────────────▼────────────────────────┐        ┌───────────────────┐
  Meta WhatsApp ────────▶│ ncrm-api (Rust, Axum)                     │◀──────▶│ PostgreSQL 16     │
  Razorpay ─────────────▶│ webhooks (planned)                        │  SQL   │  public: studio   │
                         └───────────────────────────────────────────┘        │    data, RLS      │
                                                                              │  platform:        │
                         ┌───────────────────────────────────────────┐        │    sign-in data   │
                         │ ncrm-worker (Rust)                        │◀──────▶│  jobs + outbox    │
                         │ reminders, sends, summaries (planned)     │  SQL   │  (planned)        │
                         └─────────────────────┬─────────────────────┘        └───────────────────┘
                                               │ HTTPS (planned)
                                               ▼
                                    WhatsApp Cloud API, SMS fallback
```

| Process | What it does | Talks to |
|---|---|---|
| Dashboard (`web/`) | The owner and staff interface. Renders pages on the server and forwards `/api/*` calls to the API. Holds no data and no secrets of its own. | `ncrm-api` only |
| `ncrm-api` | Every API request: sign-in, studio data, and later inbound webhooks. Answers fast; slow work goes to the queue. | Postgres |
| `ncrm-worker` | Background work: WhatsApp sends from the outbox, reminders, summaries. A stub today. | Postgres, WhatsApp *(planned)* |

The API and the worker never call each other. They coordinate through
Postgres: the API writes rows (a message to send, a job to run) and the
worker picks them up. A crash in one never loses work for the other.

## Code layout

```
crates/
  ncrm-core/    domain logic and ALL database access; no HTTP
    src/db.rs        connection pool, migrations, TenantTx
    src/auth.rs      sign-in: codes, sessions, studio switching
    src/access.rs    what each role may do
    src/orgs.rs, branches.rs, staff.rs, audit.rs   studio data
    src/phone.rs     phone number normalization
  ncrm-api/     HTTP layer (Axum)
    src/lib.rs       routes and protective layers (headers, limits, timeout)
    src/security.rs  origin check, per-IP rate limits, client address
    src/extract.rs   cookie → session → studio context
    src/error.rs     the one JSON error format
    src/sign_in.rs   /auth/*
    src/branches.rs  /branches, the pattern for studio-scoped routes
  ncrm-worker/  background jobs (stub)
migrations/     SQL, applied in order, embedded into the binaries
web/            the dashboard (Next.js 16, TypeScript, Tailwind 4)
  src/proxy.ts               CSP nonce + security headers, sign-in redirect
  src/app/api/[...path]/     same-origin gateway to the Rust API
  src/app/(auth)/            sign-in, choose a studio
  src/app/(app)/             the signed-in app: shell, home, branches
  src/lib/api-server.ts      server-side API calls (server-only)
  src/lib/api-client.ts      browser-side API calls through /api
  src/lib/bff.ts, csp.ts, safe-redirect.ts   security helpers (unit-tested)
```

Rust dependencies point one way: `ncrm-api` and `ncrm-worker` depend on
`ncrm-core`, which knows nothing about HTTP. The dashboard depends on the
API's JSON contract only (`web/src/lib/types.ts`).

## The database

One PostgreSQL database, two schemas, two roles.

| | `public` schema | `platform` schema |
|---|---|---|
| Holds | Studio data: organizations, branches, staff, audit log, and later students, classes, attendance, fees | Data that exists before anyone is inside a studio: sign-in codes, sessions |
| Every row has | `org_id` | no studio |
| Protected by | Row-level security, policy per table | No access for `ncrm_app` at all |

| Role | Can log in | Used for |
|---|---|---|
| Owner (the `DATABASE_URL` user) | yes | Running migrations; sign-in queries; worker jobs that sweep all studios. Owns the tables, so row-level security does not restrict it. |
| `ncrm_app` | no | Every studio-scoped query. Row-level security limits it to one studio. |

### How a connection becomes "inside one studio"

`TenantTx::begin(pool, org_id)` in `ncrm-core/src/db.rs`:

1. Borrows a connection from the pool and starts a transaction.
2. `SET LOCAL ROLE ncrm_app`: for this transaction only, act as the
   restricted role.
3. `set_config('app.org_id', <uuid>, true)`: for this transaction only,
   remember which studio we are in.
4. Every table's policy compares `org_id` to `app_current_org()`, which
   reads that setting. Rows of other studios are invisible and cannot be
   written.

Both settings end with the transaction, so the next request that borrows
the same pooled connection starts clean. A test proves this. Full rules:
`docs/TENANCY.md`.

## The dashboard

### Two ways the dashboard reaches the API

```
Browser ──GET /branches──▶ Next.js page (server)
                             │ proxy.ts: nonce + CSP, cookie present?
                             │ (app)/layout.tsx: getMe()  ─┐
                             │ page.tsx: listBranches()   ─┤ api-server.ts
                             │                             ▼
                             │            GET {API_ORIGIN}/auth/me, /branches
                             │            with only the session cookie
                             ◀── HTML, every script carrying the nonce

Browser ──POST /api/branches──▶ Next.js route handler app/api/[...path]
  (fetch from a client           │ path must be on the allowlist
   component, JSON body,         │ only allowlisted headers pass
   cookie sent automatically)    │ body ≤ 64 KB, no redirects followed
                                 ▼
                          POST {API_ORIGIN}/branches  (Origin, cookie, X-Real-IP)
                                 │ API: origin check, session, role, RLS
                                 ◀── JSON + Set-Cookie passed back
```

1. **Server-rendered pages** call the API from the Next.js server with
   `src/lib/api-server.ts`. It forwards only the session cookie, never
   caches, and turns "not signed in" into a redirect to `/sign-in` and
   "no studio picked" into a redirect to `/choose-studio`.
2. **Interactive parts** (sign-in form, adding a branch, switching studio,
   signing out) call `/api/...` on the dashboard's own origin with
   `src/lib/api-client.ts`. The route handler forwards to the API.

So the browser only ever talks to one origin: the session cookie is
first-party, no CORS is needed, and the API never has to be reachable from
the internet.

### What runs before every page (`src/proxy.ts`)

1. Generates a random 128-bit nonce and sets a strict Content Security
   Policy: scripts only run if they carry the nonce; fonts, styles, images
   and fetches only from our own origin; no framing.
2. Sends visitors without a session cookie to `/sign-in?next=…`. This is a
   convenience only; the API decides whether a session is valid.

Static security headers for every response (nosniff, frame denial,
referrer policy, permissions policy, HSTS in production) are set in
`next.config.ts`.

### Pages

| Route | Who | Shows |
|---|---|---|
| `/sign-in` | anyone | Phone, then 6-digit code |
| `/choose-studio` | signed in, several studios | Studio picker |
| `/` | signed in with a studio | Setup checklist, branch count, role; empty states for what isn't built yet |
| `/branches` | signed in with a studio | Branch list; add form for owners and managers |

Theme (Daylight or Emerald) is a plain, non-sensitive `ncrm_theme` cookie
read on the server, so the page renders in the right theme without a
flash.

## A request through the API, step by step

Example: the dashboard calls `POST /branches`.

```
Dashboard gateway           ncrm-api                                Postgres
   │ POST /branches            │                                        │
   │ Cookie, Origin,           │                                        │
   │ X-Real-IP                 │                                        │
   │──────────────────────────▶│ 0. layers: body ≤ 64 KB, 30 s timeout, │
   │                           │    Origin must be on ALLOWED_ORIGINS   │
   │                           │ 1. Staff extractor:                    │
   │                           │    hash cookie token, look up session ─▶ platform.sessions
   │                           │    check staff still active          ─▶ staff (as owner)
   │                           │ 2. Body extractor: parse JSON          │
   │                           │ 3. staff.require(ManageBranches)       │
   │                           │ 4. TenantTx::begin(org_id)           ─▶ SET LOCAL ROLE ncrm_app
   │                           │                                        │ set app.org_id
   │                           │ 5. INSERT branch                     ─▶ RLS checks org_id
   │                           │ 6. INSERT audit_log entry            ─▶ same transaction
   │                           │ 7. COMMIT                            ─▶
   │◀──────────────────────────│ 201 + branch JSON                      │
   │                           │ + no-store, nosniff, no-referrer, CSP  │
```

Every failure becomes `{"error": {"code": "...", "message": "..."}}`:

| Code | Status | Meaning |
|---|---|---|
| `not_signed_in` | 401 | No valid session, or no longer staff there |
| `choose_studio` | 409 | Phone works at several studios; none picked |
| `not_allowed` | 403 | The role can't do this |
| `bad_origin` | 403 | A state-changing request from a page that isn't the dashboard |
| `not_found` | 404 | Doesn't exist, or belongs to another studio (indistinguishable on purpose) |
| `conflict` | 409 | Duplicate (for example a branch name) |
| `invalid_input` | 400 | Bad or missing fields, malformed JSON |
| `too_large` | 413 | Body over 64 KB |
| `invalid_code` | 401 | Sign-in code wrong, expired or used |
| `code_locked` | 429 | Too many wrong guesses; ask for a new code |
| `too_many_requests` | 429 | Per-phone or per-IP rate limit |
| `internal` | 500 | Our fault; detail goes to the log only |

The dashboard gateway adds two of its own: `api_unreachable` (502) and the
same `not_found`/`too_large` shapes for paths or bodies it refuses itself.

## Signing in

Staff sign in with their phone number and a 6-digit code. No passwords.

```
Browser                    ncrm-api                         Postgres            WhatsApp
  │ POST /auth/code {phone}  │ per-IP limit                    │                  │
  │─────────────────────────▶│ normalize phone                 │                  │
  │                          │ lock on phone, count last hour ─▶ login_codes       │
  │                          │ store HMAC(phone:code)         ─▶ login_codes       │
  │                          │ is this phone active staff?    ─▶ staff             │
  │◀─────────────────────────│ 202, same answer either way     │                  │
  │                          │ (background) deliver code ──────────────────────────▶ (planned;
  │                          │                                 │                  │  logged in dev)
  │ POST /auth/verify        │ per-IP limit                    │                  │
  │ {phone, code}            │ newest unused code, FOR UPDATE ─▶ login_codes       │
  │─────────────────────────▶│ compare HMAC in constant time   │                  │
  │                          │ wrong: attempts + 1             │                  │
  │                          │ right: consume, find studios   ─▶ staff             │
  │                          │ create session (token hash)    ─▶ sessions          │
  │◀─────────────────────────│ Set-Cookie + who you are        │                  │
```

- **One studio:** the session lands in it straight away.
- **Several studios:** the session has no studio until `POST /auth/studio`
  picks one. The same call switches studios later.
- **Every request** re-checks that the person is still active staff with
  the same phone, so deactivating someone takes effect immediately.
- **Signing out** revokes the session and clears the cookie.
- **The cookie** is `__Host-ncrm_session` in production (`HttpOnly`,
  `Secure`, `SameSite=Lax`, `Path=/`, no `Domain`), `ncrm_session` over
  local http.

## Who may do what

Row-level security decides which studio's data a request can reach. The
capability matrix in `ncrm-core/src/access.rs` decides what the person may
do inside it:

| Capability | Owner | Manager | Front desk | Instructor |
|---|---|---|---|---|
| View the studio | ✓ | ✓ | ✓ | ✓ |
| Manage branches | ✓ | ✓ | | |
| Manage staff | ✓ | ✓ | | |
| Manage students | ✓ | ✓ | ✓ | |
| Manage fees | ✓ | ✓ | ✓ | |
| Mark any class's attendance | ✓ | ✓ | ✓ | |
| Mark own classes' attendance | ✓ | ✓ | ✓ | ✓ |
| View financial reports | ✓ | ✓ | | |
| Billing and message wallet | ✓ | | | |

`GET /auth/me` returns the list for the current role so the dashboard can
hide what the person can't use. The API checks again on every call.

## Defences at a glance

| Threat | Defence | Where |
|---|---|---|
| One studio reading another's data | Row-level security, composite foreign keys | Postgres, `TenantTx` |
| Doing more than the role allows | Capability check in every handler | `access.rs`, `Staff::require` |
| Guessing sign-in codes | HMAC storage, 5 attempts per code, 5 codes per phone per hour, per-IP limits | `auth.rs`, `security.rs` |
| Finding out who is staff | Identical answers and timing for every number | `auth.rs`, `sign_in.rs` |
| Stolen database leaking sessions | Only SHA-256 of tokens stored | `platform.sessions` |
| Cross-site request forgery | SameSite cookie, JSON-only bodies, Origin and Fetch Metadata check | cookie, `security.rs` |
| Script injection (XSS) | React escaping, nonce-based CSP, no inline scripts | `proxy.ts`, `csp.ts` |
| Clickjacking | `frame-ancestors 'none'`, `X-Frame-Options: DENY` | `csp.ts`, `next.config.ts` |
| Open redirects after sign-in | `safeNextPath` | `safe-redirect.ts` |
| Reaching unintended API paths | Gateway path allowlist, strict segments | `bff.ts` |
| Caching personal data | `no-store` on every API response and page | API layers, `proxy.ts` |
| Huge or slow requests | 64 KB body limit, 30 s timeout | API and gateway |
| Known-vulnerable dependencies | `cargo audit`, `pnpm audit` in CI | CI |

## Queries that look across studios

These run as the owner role, outside any studio, and are the only ones
allowed to. Each looks up only the caller's own phone number or session:

| Where | Why it can't be studio-scoped |
|---|---|
| `auth::request_code` | Checks whether a phone is staff anywhere, before any studio is known |
| `auth::verify_code` | Lists the studios a phone works at |
| `auth::memberships`, `select_studio` | Studio switcher |
| `auth::session_from_token`, `staff_context` | Turns a cookie into a studio context, on every request |
| `orgs::create` | A new studio doesn't exist yet |

## Configuration

| Variable | Used by | Meaning |
|---|---|---|
| `DATABASE_URL` | api, worker, tests | Postgres connection. The user must own the schema (and may create databases, for tests). |
| `AUTH_SECRET` | api | At least 32 bytes. Keys the HMAC of sign-in codes. Changing it invalidates codes in flight, not sessions. Generate with `openssl rand -base64 48`. |
| `ALLOWED_ORIGINS` | api | Exact dashboard origin(s), comma-separated, e.g. `https://app.example.com`. State-changing requests from other pages are refused. |
| `CLIENT_IP_HEADER` | api | Header carrying the real client IP, set by our own proxy (`x-real-ip`). Unset means use the TCP peer. Only set it when nothing can reach the API except through that proxy. |
| `BIND_ADDR` | api | Default `0.0.0.0:8080`. Bind to `127.0.0.1` in production. |
| `RUN_MIGRATIONS` | api | `1` to apply migrations at startup. |
| `COOKIE_SECURE` | api | Default on (`__Host-` cookie, HTTPS only). `0` only for local http. |
| `CODE_SENDER` | api | `log` (default, development). WhatsApp comes with that milestone. |
| `API_ORIGIN` | dashboard | Where the dashboard's server reaches the API, e.g. `http://127.0.0.1:8080`. Server-side only. |

## Deployment shape *(planned)*

One VPS to start:

- **nginx** is the only public listener: TLS, HTTP→HTTPS redirect, and
  `proxy_set_header X-Real-IP $remote_addr;` (which overwrites anything the
  client sent).
- **Dashboard** (`next start`) on `127.0.0.1:3000`, behind nginx.
- **API** on `127.0.0.1:8080`, reachable only from the dashboard (and
  later nginx for webhooks), with `CLIENT_IP_HEADER=x-real-ip` and
  `ALLOWED_ORIGINS=https://<dashboard domain>`.
- **Postgres**: managed, with automatic backups.

## Testing

- `#[sqlx::test]` gives every Rust test its own fresh database with all
  migrations applied, so tests are independent and run in parallel.
- `ncrm-core/tests/`: rules directly (isolation between studios, sign-in
  abuse cases).
- `ncrm-api/tests/`: the real router in memory, with a `Capture` code
  sender; `security_api.rs` attacks each protection.
- `web/src/lib/security.test.ts` (Vitest): redirects, gateway paths,
  header filtering, CSP.
- The Rust suite also passes as a non-superuser database owner, which is
  how a managed Postgres behaves.
- Before a milestone ships, the full stack (Postgres, API, production
  dashboard build) is run together and exercised through the dashboard's
  `/api` gateway, including the attacks above.
- CI runs format, lint, tests, builds and dependency audits for both the
  Rust workspace and the dashboard.
