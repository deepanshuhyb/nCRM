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
    src/orgs.rs, branches.rs, audit.rs   studio data
    src/staff.rs     the team: hire, change role/phone/branch, deactivate
    src/students.rs  students, their contacts, search
    src/rooms.rs     rooms inside a branch
    src/batches.rs   classes and their weekly slots; clash checks
    src/plans.rs     prices: monthly fees, packs, drop-ins
    src/enrollments.rs  putting students in classes and taking them out
    src/credits.rs   class credits from packs and drop-ins
    src/validate.rs  shared input checks with plain messages
    src/phone.rs     phone number normalization
  ncrm-api/     HTTP layer (Axum)
    src/lib.rs       routes and protective layers (headers, limits, timeout)
    src/security.rs  origin check, per-IP rate limits, client address
    src/extract.rs   cookie → session → studio context
    src/error.rs     the one JSON error format
    src/sign_in.rs   /auth/*
    src/branches.rs  /branches, the pattern for studio-scoped routes
    src/team.rs      /staff
    src/students.rs  /students and their contacts
    src/summary.rs   /summary, the counts on the home page
    src/classes.rs   /rooms, /batches
    src/plans.rs     /plans
    src/enrollments.rs  /students/{id}/enrollments, /enrollments, credits
  ncrm-worker/  background jobs (stub)
migrations/     SQL, applied in order, embedded into the binaries
web/            the dashboard (Next.js 16, TypeScript, Tailwind 4)
  src/proxy.ts               CSP nonce + security headers, sign-in redirect
  src/app/api/[...path]/     same-origin gateway to the Rust API
  src/app/(auth)/            sign-in, choose a studio
  src/app/(app)/             the signed-in app: shell, home, branches,
                             classes (timetable, prices), students, team
  src/components/timetable.tsx  the week grid (server-rendered, no JS)
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
| Holds | Studio data: organizations, branches, staff, students, contacts, rooms, classes, plans, enrollments, class credits, audit log, and later attendance and fees | Data that exists before anyone is inside a studio: sign-in codes, sessions |
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

### Studio tables

| Table | One row is | Notes |
|---|---|---|
| `organizations` | a studio (the tenant) | Its `id` is the `org_id` everywhere else |
| `branches` | a place the studio teaches | |
| `staff` | a person on the team, with a role | Phone unique per studio; that phone is how they sign in |
| `students` | someone who takes classes | Status `active`, `paused` or `left`; never deleted by the app (see below) |
| `contacts` | a WhatsApp number we may message | Phone unique per studio, so siblings share one parent |
| `student_contacts` | "this contact is the mother/father/self… of this student" | At most one `is_primary` per student; up to 4 per student |
| `rooms` | a space inside a branch | Optional capacity; name unique per branch |
| `batches` | a class that meets every week ("Hip-hop Beginners") | Branch, optional room and usual teacher, capacity, first and last day; archived, never deleted |
| `batch_slots` | one weekly meeting of a class ("Monday 18:00–19:00") | Wall-clock time of the branch; no class crosses midnight |
| `plans` | a price | `monthly`, `pack` (N classes within D days) or `drop_in`; money in whole paise |
| `enrollments` | "this student is a regular in this class" | `ended_on` is the day they left; one open enrollment per student and class |
| `credit_lots` | classes given at once (a pack sold, a drop-in, free classes) | Price copied at sale; `expires_on` is the last usable day. Append-only |
| `credit_moves` | a later change to a lot (used, returned, taken back) | Append-only; a trigger keeps each lot between 0 and its size |
| `audit_log` | something that changed, who did it | Append-only: the app role can insert and read, not change or delete |

```
students ──< student_contacts >── contacts
   │
   ├──< enrollments >── batches ──< batch_slots
   │         │            │  └──▶ rooms ──▶ branches
   │         └──▶ plans   └──▶ staff (teacher)
   │
   └──< credit_lots ──< credit_moves
             └──▶ plans
```

A class's room must be in the class's branch. The database enforces that
too: `batches` references `rooms (org_id, branch_id, id)`, so a room from
another branch (or another studio) is refused. With `room_id` NULL, the
default `MATCH SIMPLE` skips the check.

Every arrow is a composite foreign key that includes `org_id`. Foreign
key checks ignore row-level security, so a plain `branch_id` key would let
a student point at another studio's branch; with `(org_id, branch_id)` the
database itself refuses it.

The app role has no `DELETE` on `students`, `contacts`, `staff`,
`batches`, `plans` or `enrollments`. People leave by status (`left`,
deactivated); classes are archived; plans stop being offered; leaving a
class sets `ended_on`. Erasing a person on request is a separate, audited
job, not a button (planned; see `docs/PLAN.md`).

Class credits are a ledger the app role can only add to: `SELECT, INSERT`
on `credit_lots` and `credit_moves`, nothing else. A credit is never
edited; a correction is a new move with a reason. The trigger
`credit_moves_keep_balance` refuses any move that would take a lot below
zero or above its original size, so even a bug or two screens saving at
once can't spend a credit twice.

### Concurrency: who wins when two people save at once

| Rule | How it holds |
|---|---|
| A room or teacher isn't double-booked | Every schedule change takes a per-studio lock (`TenantTx::lock_studio("schedule")`, a transaction-scoped advisory lock), then checks for clashes, then writes |
| A class doesn't go over capacity | Enrolling locks the class row (`SELECT … FOR UPDATE`) before counting |
| A studio keeps an owner | The other owners' rows are locked before the check |
| A credit isn't spent twice | The ledger trigger takes a per-lot advisory lock before summing |

Each has a test that runs two transactions at once and checks the second
is refused.

### Dates and times

The API server runs on UTC. "Today" for a studio comes from
`TenantTx::studio_today()`, which uses the studio's `timezone` (India by
default). Class times are wall-clock `time` values, sent as `"18:00"`.
Dates are `"2026-11-23"`.

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
2. **Interactive parts** (sign-in form, adding a branch, a student or a
   team member, editing them, switching studio, signing out) call
   `/api/...` on the dashboard's own origin with `src/lib/api-client.ts`.
   The route handler forwards to the API. After a change, the component
   calls `router.refresh()`, which re-renders the server page with fresh
   data; no data is cached in the browser.

The gateway only forwards paths whose first segment is on the allowlist in
`src/lib/bff.ts`: `auth`, `batches`, `branches`, `enrollments`, `plans`,
`rooms`, `staff`, `students`, `summary`. A new
top-level API path is unreachable from the browser until it is added there.

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
| `/` | signed in with a studio | Setup checklist driven by `/summary`, counts for students, team and branches |
| `/branches` | signed in with a studio | Branch list; add form for owners and managers |
| `/students` | manage students | Search by name or 4+ phone digits, filter by status, 50 per page |
| `/students/new` | manage students | Student details and up to 4 contacts |
| `/students/[id]` | manage students | Profile, contacts (add, remove, choose the main one), edit details and status |
| `/team` | manage staff | Team list with per-person edit; add someone. Only owners see the "Owner" role |
| `/classes` | everyone | The week as a timetable per branch; each class fills up as students join and turns to the attention colour when full. A list by day on phones |
| `/classes/new` | manage classes | Name, branch, room, teacher, capacity, then days and times |
| `/classes/[id]` | everyone | Schedule, how full, roster (teacher of the class and front desk), add or take out students, edit and archive |
| `/classes/plans` | manage students | Prices by kind; owners and managers add and edit |
| `/branches` | (now also) | Rooms per branch, added and edited in place |
| `/students/[id]` | (now also) | Their classes, put in a class, class credits, sell a pack, give or take back credits |

Navigation shows only the pages the role can open (built from the
capabilities in `/auth/me`). That is a convenience: a page opened directly
by someone without the capability shows "You can't open …", and the API
refuses the data anyway. A student or member id that isn't a UUID, doesn't
exist, or belongs to another studio gives the same 404 page.

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

### Endpoints

| Method and path | Needs | Does |
|---|---|---|
| `GET /healthz` | nothing | Database reachable? |
| `POST /auth/code`, `/auth/verify` | nothing (rate limited) | Sign-in, see below |
| `GET /auth/me` | session | Who am I, which studio, which capabilities |
| `POST /auth/studio`, `/auth/sign-out` | session | Switch studio, sign out |
| `GET /summary` | view studio | Counts for the home page (branches, students, team, classes, plans) and the studio's date |
| `GET, POST /branches` | view / manage branches | List, add |
| `GET, POST /staff` | manage staff | List the team, add someone |
| `PUT /staff/{id}` | manage staff | Replace name, phone, role, branch, active |
| `GET /students?q=&status=&branch_id=&limit=&offset=` | manage students | Search and page; returns `{items, total, limit, offset}` |
| `POST /students` | manage students | Create with up to 4 contacts |
| `GET, PUT /students/{id}` | manage students | Profile with contacts; replace name, date of birth, branch, status, notes |
| `POST /students/{id}/contacts` | manage students | Link a contact (reusing one with the same number) |
| `DELETE /students/{id}/contacts/{contact_id}` | manage students | Unlink; the oldest remaining contact becomes the main one |
| `GET /rooms` | view studio | Every room |
| `POST /branches/{id}/rooms`, `PUT /rooms/{id}` | manage branches | Add; rename, resize or close (not while a class uses it) |
| `GET /batches?branch_id=&archived=` | view studio | The timetable: classes with slots, names and how many are in each |
| `POST /batches`, `PUT /batches/{id}` | manage classes | Create; replace everything including slots. `active: false` archives and takes everyone out |
| `GET /batches/{id}` | view studio | One class; `roster` is null unless you manage students or teach it |
| `GET /plans` | manage students | Prices (front desk needs them to sell) |
| `POST /plans`, `PUT /plans/{id}` | manage classes | Add; change name, price, terms, offered. The kind never changes |
| `GET, POST /students/{id}/enrollments` | manage students | Their classes; put them in one |
| `POST /enrollments/{id}/end` | manage students | Take them out from today |
| `GET /students/{id}/credits` | manage students | Usable classes, next expiry, every lot |
| `POST /students/{id}/credits/sell` | manage fees | Sell a pack or drop-in |
| `POST /students/{id}/credits/give`, `/take-back` | manage classes | Free classes or take some back, with a reason |

Updates are `PUT` with the whole object, not `PATCH`. With `PATCH`, a
missing `branch_id` and `"branch_id": null` ("clear it") are easy to mix
up in both Rust and TypeScript; sending everything every time removes the
question. Ids in paths that aren't UUIDs get `not_found`, the same as ids
from another studio.

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
| Manage students (and put them in classes) | ✓ | ✓ | ✓ | |
| Manage classes: timetable, prices, free credits | ✓ | ✓ | | |
| Manage fees (sell packs, payments) | ✓ | ✓ | ✓ | |
| Mark any class's attendance | ✓ | ✓ | ✓ | |
| Mark own classes' attendance | ✓ | ✓ | ✓ | ✓ |
| View financial reports | ✓ | ✓ | | |
| Billing and message wallet | ✓ | | | |

`GET /auth/me` returns the list for the current role so the dashboard can
hide what the person can't use. The API checks again on every call, and
the core functions check a third time: every function in `staff.rs` and
`students.rs` that changes data takes an `Actor` (who is acting, with what
role) and calls `actor.require(capability)` itself. A future caller that
isn't an HTTP handler (a worker job, an import, the WhatsApp bot) can't
forget the check.

Team rules on top of the matrix, enforced in `staff.rs` with row locks:

- Only an owner can add an owner or make someone an owner. Managers can't
  edit owners at all.
- Nobody can change their own role or deactivate themselves, so a studio
  can't lock itself out by accident.
- A studio always keeps at least one active owner, even if two owners
  demote each other at the same moment (the other owners' rows are locked
  while the check runs).
- Changing someone's phone ends their sessions: a session only works while
  the staff row still has the phone that signed in.

Instructors can open the home page, so `/summary` needs only "view the
studio". It returns counts, never names or numbers.

Instructors see the whole timetable (so they know where to be), and the
students of their own classes only: `GET /batches/{id}` includes the
roster for staff who manage students and for that class's teacher, and
`null` for everyone else. The roster has names and plans, never phone
numbers.

Front desk sells packs (money comes in) but can't give classes away or
take credits back, because that's revenue the studio doesn't collect.
Those need "manage classes", a reason, and leave an audit entry.

## Defences at a glance

| Threat | Defence | Where |
|---|---|---|
| One studio reading another's data | Row-level security, composite foreign keys | Postgres, `TenantTx` |
| Doing more than the role allows | Capability check in every handler, again in core functions | `access.rs`, `Staff::require`, `Actor::require` |
| A studio losing its last owner | Row-locked "another active owner exists" check | `staff::update` |
| The audit log becoming a contact list | Audit entries record field names and roles, never phone numbers | `staff.rs`, `students.rs` |
| Search input breaking queries | `%` and `_` escaped, length capped, parameters bound | `students::list` |
| Double-booked rooms or teachers | Clash check under a per-studio lock | `batches.rs` |
| Overfilled classes | Capacity counted under a row lock | `enrollments::enroll` |
| Credits spent twice or invented | Append-only ledger, balance trigger with a per-lot lock | migration 0004, `credits.rs` |
| Free classes handed out quietly | Owners and managers only, reason required, audited | `credits::give` |
| Error text showing internals | Constraint names and parser messages turned into sentences | `ncrm_core::error`, `extract.rs` |
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
  abuse cases, team and student rules in `people.rs`, classes, plans,
  enrollments and credits in `classes.rs`, including races between two
  transactions).
- `ncrm-api/tests/`: the real router in memory, with a `Capture` code
  sender; `security_api.rs` attacks each protection; `people_api.rs` runs
  students and team through each role, bad input and another studio;
  `classes_api.rs` does the same for classes, prices and credits.
- `web/src/lib/security.test.ts` (Vitest): redirects, gateway paths,
  header filtering, CSP, and the phone/age/date formatting helpers.
- The Rust suite also passes as a non-superuser database owner, which is
  how a managed Postgres behaves.
- Before a milestone ships, the full stack (Postgres, API, production
  dashboard build) is run together and exercised through the dashboard's
  `/api` gateway, including the attacks above.
- CI runs format, lint, tests, builds and dependency audits for both the
  Rust workspace and the dashboard.
