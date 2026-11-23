# Engineering notes

Decisions, trade-offs and things that were not straightforward, newest
first. Add an entry whenever something needed extra understanding.

Template:

```
## YYYY-MM-DD: Short title

**Context:** what we were building.
**What wasn't obvious:** the trap, the question, the surprise.
**What we did:** the decision and how it works.
**Consequences:** what this means later, and what to watch for.
```

---

## 2026-11-23: Cookie sessions and cross-site requests

**Context:** the dashboard will call the API from a browser.

**What wasn't obvious:** cookies are sent automatically, so another
website could try to make a signed-in owner's browser send a request
(CSRF).

**What we did:**
- The session cookie is `HttpOnly` (JavaScript can't read it), `Secure`
  (HTTPS only), `SameSite=Lax` (not sent on cross-site POSTs) and
  `Path=/`.
- Every request body must be JSON. A plain HTML form on another site
  can't send `application/json` without a CORS preflight, which we don't
  allow.

**Consequences:** the dashboard must be served from the same site as the
API, for example `app.<domain>` and `api.<domain>`, or the dashboard
proxying `/api`. If we ever serve it from a different site we need CORS
with credentials and a CSRF token. A future mobile app would use an
`Authorization` header instead of the cookie; the extractor is the only
place that would change.

## 2026-11-23: Sign-in codes that can't be guessed or abused

**Context:** staff sign in with a 6-digit code sent to their phone.

**What wasn't obvious:**
1. A 6-digit code has only a million values. A plain SHA-256 of it in the
   database could be reversed instantly if the database leaked.
2. The endpoint could reveal which phone numbers are staff, by answering
   differently or more slowly for them (sending a WhatsApp takes time).
3. Two requests at the same instant could both pass a "max 5 per hour"
   check.
4. Two guesses at the same instant could both read "4 attempts used" and
   both get a try.

**What we did:**
1. Store `HMAC-SHA256(AUTH_SECRET, phone + ":" + code)`. Without the
   secret, a leaked hash is useless; binding the phone means a hash can't
   be replayed for another number. Comparison is constant-time
   (`verify_slice`).
2. `POST /auth/code` always returns the same 202 and writes a row even
   for unknown numbers (so rate limits behave identically). Delivery runs
   in a background task, so response time doesn't depend on it.
3. `pg_advisory_xact_lock(hashtextextended(phone, 0))` serializes code
   requests per phone inside the transaction, so the count is reliable.
4. The newest unused code is read `FOR UPDATE`, so concurrent guesses
   queue up.
5. Only the newest code counts; asking again replaces the old one. A
   correct code consumes all outstanding codes for that phone.

**Consequences:** an attacker gets at most 5 codes × 5 guesses = 25
guesses per hour per phone, about 1 in 40,000. Limits are per phone, not
per IP; an attacker could still fill the table with codes for many random
numbers. Add per-IP rate limiting at the reverse proxy (nginx
`limit_req`) before launch, and a job that deletes codes older than a day.

## 2026-11-23: Sign-in tables live in a separate `platform` schema

**Context:** codes and sessions exist before we know the studio.

**What wasn't obvious:** every table in `public` must have an `org_id`
and a row-level security policy; the guard test enforces it. Sign-in
tables have no studio, so they can't follow that rule, and we didn't want
to weaken the guard.

**What we did:** a separate `platform` schema. `ncrm_app` has no `USAGE`
on it, and the tables have row-level security turned on with no policies
as a second lock. Only the owner role reaches them. A test checks that a
studio-scoped transaction gets "permission denied".

**Consequences:** anything else that is not studio data (subscriptions,
the message wallet's top-ups at platform level, Meta webhook dedupe
records) goes in `platform` too.

## 2026-11-23: Async in traits, and Axum 0.8 extractors

**Context:** the code sender needs several implementations (log, test
capture, WhatsApp), and the API needs custom extractors.

**What wasn't obvious:** Rust now supports `async fn` in traits, but such
traits can't be used as trait objects (`Arc<dyn Sender>`) without boxing
every future. Axum 0.8 also changed extractors: no more
`#[async_trait]`, and `Option<T>` extractors need a separate
`OptionalFromRequestParts` trait.

**What we did:** `CodeSender` is an enum with one variant per
implementation, which keeps async simple. Extractors use plain
`async fn from_request_parts`. Sign-out reads the cookie jar directly
instead of using an optional extractor.

**Consequences:** adding WhatsApp delivery means adding an enum variant
with its client inside.

## 2026-11-23: Foreign keys can leak across studios unless they include org_id

**Context:** staff point at a branch; later, attendance points at a
session, a session at a batch, and so on.

**What wasn't obvious:** Postgres checks foreign keys *without*
row-level security. With a plain `branch_id REFERENCES branches(id)`, a
studio could insert a staff row pointing at another studio's branch, as
long as it knew the id. RLS on the staff table doesn't catch it, because
the staff row itself has the right `org_id`.

**What we did:** every table has `UNIQUE (org_id, id)` and child rows
reference `(org_id, parent_id)`. The foreign key now requires the parent
to be in the same studio. A test tries exactly this attack.

**Consequences:** every future table follows the pattern; it's in the
checklist in `docs/TENANCY.md`.

## 2026-11-23: Row-level security, roles and pooled connections

**Context:** many studios share one database; a bug must not leak data.

**What wasn't obvious:**
1. The connection pool reuses connections. If a request set "studio = A"
   on a connection and the next request (for studio B) got the same
   connection, B could see A's data.
2. The table owner bypasses row-level security. That's handy for sign-in
   queries, but means studio queries must not run as the owner.
3. Managed Postgres doesn't give you a superuser. A normal owner can't
   `SET ROLE ncrm_app` unless it's a member of that role.
4. Roles are shared by the whole Postgres server, not per database.
   Tests create many databases at once, and each runs the migration that
   creates `ncrm_app`, so two can race.

**What we did:**
1. Both the role switch and the studio id are set with transaction-local
   commands (`SET LOCAL ROLE`, `set_config(..., true)`), which Postgres
   undoes at commit or rollback. A test forces one connection and checks
   nothing survives, including after a dropped (rolled-back) transaction.
2. Studio work always goes through `TenantTx`, which switches to the
   restricted `ncrm_app` role. When no studio is set,
   `app_current_org()` is NULL and matches no rows: the default is "see
   nothing".
3. The migration grants `ncrm_app` to the current user when it isn't a
   superuser. The whole suite is also run as a non-superuser owner.
4. The migration's `CREATE ROLE` ignores both `duplicate_object` and
   `unique_violation`, the two errors the race can produce.

**Consequences:** never set `app.org_id` or the role any other way. If the
app ever connects as a non-owner role, the cross-studio queries listed in
`docs/ARCHITECTURE.md` must move into `SECURITY DEFINER` functions.

## 2026-11-23: Phone numbers are identities

**Context:** WhatsApp identifies people by number, and staff sign in
with it.

**What wasn't obvious:** the same number gets typed many ways
(`98765 43210`, `+91-98765-43210`, `09876543210`, `0091…`). Stored as
typed, one person becomes two, and sign-in breaks.

**What we did:** `phone::normalize` turns every form into E.164
(`+919876543210`) before anything is stored or compared. Bare 10-digit
numbers are treated as Indian mobiles (they must start with 6–9);
numbers with `+` or `00` keep their own country code. A `+` is only
accepted before the first digit. The database also checks the E.164 shape.

**Consequences:** every place that accepts a phone must call `normalize`.
Tests compare normalized numbers too (one test helper got this wrong at
first).

## 2026-11-23: Dynamic SQL in SQLx 0.9

**Context:** shared column lists in queries.

**What wasn't obvious:** SQLx 0.9's `query()` only accepts string
literals by default, to stop SQL injection through `format!`. Building a
query string needs an explicit `sqlx::AssertSqlSafe(...)`.

**What we did:** use `AssertSqlSafe(format!(...))` only to splice in
constant column lists defined in the same file. User input always goes
through `.bind()`.

**Consequences:** reviewers should treat any `AssertSqlSafe` whose
string includes something other than a constant as a bug.
