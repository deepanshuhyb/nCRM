# ncrm architecture

How the system is put together, how its parts connect, and how a request
travels through it. Things marked *(planned)* are designed but not built.

## The big picture

```
                 ┌──────────────────────────┐
  Owner / staff  │ Dashboard (Next.js)      │ (planned)
  browser ──────▶│ same site as the API     │
                 └────────────┬─────────────┘
                              │ HTTPS, JSON, session cookie
                              ▼
                 ┌──────────────────────────┐        ┌───────────────────┐
                 │ ncrm-api (Rust, Axum)    │◀──────▶│ PostgreSQL 16     │
  Meta WhatsApp ▶│ webhooks (planned)       │  SQL   │  public: studio   │
  Razorpay ─────▶│ webhooks (planned)       │        │    data, RLS      │
                 └──────────────────────────┘        │  platform: sign-in│
                                                     │    data           │
                 ┌──────────────────────────┐        │                   │
                 │ ncrm-worker (Rust)       │◀──────▶│  jobs + outbox    │
                 │ reminders, sends,        │  SQL   │  (planned)        │
                 │ summaries (planned)      │        └───────────────────┘
                 └────────────┬─────────────┘
                              │ HTTPS (planned)
                              ▼
                     WhatsApp Cloud API, SMS fallback
```

Three processes share one codebase and one database:

| Process | What it does | Talks to |
|---|---|---|
| `ncrm-api` | Every HTTP request: dashboard calls, sign-in, and later inbound webhooks from WhatsApp and Razorpay. Answers fast; slow work goes to the queue. | Postgres |
| `ncrm-worker` | Background work: sending WhatsApp messages from the outbox, scheduled reminders, daily summaries. A stub today. | Postgres, WhatsApp *(planned)* |
| Dashboard | The owner and staff interface. *(planned)* | `ncrm-api` only |

The API and the worker never call each other. They coordinate through
Postgres: the API writes rows (a message to send, a job to run) and the
worker picks them up. That keeps both simple and means a crash in one
never loses work for the other.

## Code layout

```
crates/
  ncrm-core/    domain logic and ALL database access; no HTTP
    src/db.rs        connection pool, migrations, TenantTx
    src/auth.rs      sign-in: codes, sessions, studio switching
    src/access.rs    what each role may do
    src/orgs.rs, branches.rs, staff.rs, audit.rs   studio data
    src/phone.rs     phone number normalization
  ncrm-api/     HTTP layer (Axum): routes, extractors, JSON errors
    src/extract.rs   cookie → session → studio context
    src/error.rs     the one JSON error format
    src/sign_in.rs   /auth/*
    src/branches.rs  /branches, the pattern for studio-scoped routes
  ncrm-worker/  background jobs (stub)
migrations/     SQL, applied in order, embedded into the binaries
```

Dependencies point one way: `ncrm-api` and `ncrm-worker` depend on
`ncrm-core`; `ncrm-core` knows nothing about HTTP. Business rules and SQL
live in core so both processes (and tests) use the same code.

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

Both settings end with the transaction (commit, rollback, or the
`TenantTx` being dropped), so the next request that borrows the same
pooled connection starts clean. A test proves this.

Full rules: `docs/TENANCY.md`.

## A request, step by step

Example: the owner's dashboard calls `POST /branches`.

```
Browser                 ncrm-api                                Postgres
   │ POST /branches        │                                        │
   │ Cookie: ncrm_session  │                                        │
   │──────────────────────▶│                                        │
   │                       │ 1. Staff extractor:                    │
   │                       │    hash cookie token, look up session ─▶ platform.sessions
   │                       │    check staff still active          ─▶ staff (as owner)
   │                       │ 2. Body extractor: parse JSON          │
   │                       │ 3. staff.require(ManageBranches)       │
   │                       │ 4. TenantTx::begin(org_id)           ─▶ SET LOCAL ROLE ncrm_app
   │                       │                                        │ set app.org_id
   │                       │ 5. INSERT branch                     ─▶ RLS checks org_id
   │                       │ 6. INSERT audit_log entry            ─▶ same transaction
   │                       │ 7. COMMIT                            ─▶
   │◀──────────────────────│ 201 + branch JSON                      │
```

If any step fails, the handler returns an `ApiError`, which always becomes
`{"error": {"code": "...", "message": "..."}}` with the right status. The
`code` is stable for the dashboard to switch on.

| Code | Status | Meaning |
|---|---|---|
| `not_signed_in` | 401 | No valid session, or no longer staff there |
| `choose_studio` | 409 | Phone works at several studios; none picked |
| `not_allowed` | 403 | The role can't do this |
| `not_found` | 404 | Doesn't exist, or belongs to another studio (indistinguishable on purpose) |
| `conflict` | 409 | Duplicate (for example a branch name) |
| `invalid_input` | 400 | Bad or missing fields, malformed JSON |
| `invalid_code` | 401 | Sign-in code wrong, expired or used |
| `code_locked` | 429 | Too many wrong guesses; ask for a new code |
| `too_many_requests` | 429 | Rate limit |
| `internal` | 500 | Our fault; detail goes to the log only |

## Signing in

Staff sign in with their phone number and a 6-digit code. There are no
passwords.

```
Browser                    ncrm-api                         Postgres            WhatsApp
  │ POST /auth/code {phone}  │                                 │                  │
  │─────────────────────────▶│ normalize phone                 │                  │
  │                          │ lock on phone, count last hour ─▶ login_codes       │
  │                          │ store HMAC(phone:code)         ─▶ login_codes       │
  │                          │ is this phone active staff?    ─▶ staff             │
  │◀─────────────────────────│ 202, same answer either way     │                  │
  │                          │ (background) deliver code ──────────────────────────▶ (planned;
  │                          │                                 │                  │  logged in dev)
  │ POST /auth/verify        │                                 │                  │
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

Endpoints: `POST /auth/code`, `POST /auth/verify`, `GET /auth/me`,
`POST /auth/studio`, `POST /auth/sign-out`.

Why each piece is built the way it is (HMAC, rate limits, identical
answers, cookie flags) is in `docs/NOTES.md`.

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
hide what the person can't use. The server checks again on every call.

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
| `BIND_ADDR` | api | Default `0.0.0.0:8080`. |
| `RUN_MIGRATIONS` | api | `1` to apply migrations at startup. |
| `COOKIE_SECURE` | api | Default on. Set `0` only for local http. |
| `CODE_SENDER` | api | `log` (default, development). WhatsApp comes with that milestone. |

## Testing

- `#[sqlx::test]` gives every test its own fresh database with all
  migrations applied, so tests are independent and run in parallel.
- `ncrm-core/tests/` tests rules directly: isolation between studios,
  sign-in abuse cases.
- `ncrm-api/tests/` drives the real router in memory (no network) with a
  `Capture` code sender, so a test can read the code it was "sent".
- The suite also passes as a non-superuser database owner, which is how a
  managed Postgres behaves.
- CI (GitHub Actions) runs format, lint and the full suite against
  Postgres 16 on every push.
