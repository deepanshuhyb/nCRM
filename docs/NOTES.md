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

## 2026-11-23: A CHECK that comes out NULL counts as passed

**Context:** the `plans` table, where a pack needs a class count and a
validity, and a monthly fee needs neither.

**What wasn't obvious:** SQL has three truth values. A CHECK constraint
refuses a row only when its expression is FALSE; NULL counts as passed.
`CHECK (kind <> 'pack' OR classes BETWEEN 2 AND 500)` lets a pack with
`classes = NULL` through, because `NULL BETWEEN 2 AND 500` is NULL, and
`FALSE OR NULL` is NULL. The same trap catches `x > 0`, `a = b` and every
comparison on a nullable column.

**What we did:** `plans_shape` starts every branch with an explicit
`IS NOT NULL` (`classes IS NOT NULL AND classes BETWEEN 2 AND 500`).
`FALSE AND anything` is FALSE, so each branch is a definite yes or no. A
test inserts a pack with no class count straight into the table, past the
application, and checks the database refuses it.

**Consequences:** any CHECK on a nullable column needs the same care. Ask
"what does this give when the column is NULL?"

## 2026-11-23: Class credits are a ledger the database guards

**Context:** packs and drop-ins give a student class credits, which
attendance will spend.

**What wasn't obvious:** a `credits_left` number on the student is easy to
corrupt. Two front-desk screens marking the same student at once each read
"3", each write "2", and one class is never charged. A bug that adds
instead of subtracts leaves no trace of what happened.

**What we did:** two append-only tables. A *lot* is credits given at once
(a pack, a drop-in, free classes), with the price copied in and a last
usable day. A *move* is every later change (used, returned, taken back).
What's left is `lot.classes + sum(moves.change)`. The app role can only
`SELECT` and `INSERT` them, never update or delete.

The guarantee lives in the database: the trigger
`credit_moves_keep_balance` refuses any move that would take a lot below
zero or above its size. Two moves on the same lot at once would each see
the other missing, so the trigger first takes an advisory lock named after
the lot. The second waits until the first commits. Under Postgres's
default isolation (READ COMMITTED), each statement inside the function
then takes a fresh snapshot, so its sum includes the first move. A row
lock (`SELECT … FOR UPDATE` on the lot) would have been the usual tool,
but Postgres requires UPDATE rights for row locks, and the app role
deliberately has none on the ledger.

Credits are spent from the lot that expires soonest, so nobody loses
credits to expiry while holding ones that would have lasted.

**Consequences:** attendance (next milestone) spends credits by inserting
`used` moves; undoing an attendance mark inserts a `returned` move.
Nothing ever rewrites history, so "where did her 8 classes go?" always
has an answer.

## 2026-11-23: No double-booked rooms or teachers, even with two people saving

**Context:** creating and editing classes.

**What wasn't obvious:** "is Studio A free on Monday at 6?" is a read,
then the save is a write. Two managers adding classes for the same slot at
the same moment both read "free" and both save. A unique index can't
express "time ranges overlap on the same weekday", and an exclusion
constraint can't see across the `batches` and `batch_slots` tables (the
room is on one, the time on the other).

**What we did:** every schedule change first takes a per-studio advisory
lock (`TenantTx::lock_studio("schedule")`, released when the transaction
ends), then checks clashes, then writes. Schedule edits are rare, so
making them queue costs nothing anyone would notice. The clash query
compares all the new slots at once by passing them as arrays and
`unnest`ing them; two times overlap when each starts before the other
ends, so back-to-back classes are fine. Date ranges are compared with
Postgres's `daterange … && …`, where a missing last day means "no end".

One trap in that query: excluding the class being edited with
`b.id <> $1` would exclude *every* row when creating a new class, because
`$1` is NULL and `anything <> NULL` is NULL (not true). It uses
`b.id IS DISTINCT FROM $1`, which treats NULL as a plain value.

**Consequences:** the same lock-then-check pattern fits any future rule
that a constraint can't express. A test runs two creations at once and
checks the second is refused.

## 2026-11-23: The last place in a class goes to one student

**Context:** enrolling into a class with a capacity.

**What wasn't obvious:** counting "how many are in" and inserting a new
enrollment is again read-then-write; two front-desk screens can both see
"19 of 20".

**What we did:** enrolling locks the class's row
(`SELECT … FROM batches … FOR UPDATE`) before counting. The second
enrollment waits, then counts 20 and is refused with "Bollywood Kids is
full (20 of 20)". Lowering a class's capacity takes the same row lock, and
can't go below the number already in.

**Consequences:** a waitlist (planned) will hang off this same check.

## 2026-11-23: Today is the studio's today

**Context:** start dates, credit expiry, leaving a class "from today".

**What wasn't obvious:** the server runs on UTC, so between midnight and
5:30 am in India the server's `CURRENT_DATE` is still yesterday. A pack
sold at 1 am would expire a day early.

**What we did:** `TenantTx::studio_today()` asks Postgres for
`now() AT TIME ZONE <studio timezone>`. Everything date-like in the
classes code uses it, and `/summary` returns it so the dashboard marks
the right day as today without guessing.

**Consequences:** `students.joined_on` still defaults to `CURRENT_DATE`
(written before this existed); it can be off by one for sign-ups between
midnight and 5:30 am. Worth fixing with the next migration that touches
students.

## 2026-11-23: Times of day travel as "18:00"

**Context:** class slots in JSON.

**What wasn't obvious:** the `time` crate's readable format for a time of
day is `18:00:00.0`, which is noisy and not what `<input type="time">`
gives (`"18:00"`).

**What we did:** a serde format made with
`time::serde::format_description!(hhmm, Time, "[hour]:[minute]")`, used
on the slot fields. Anything else (`"7pm"`, `"25:00"`) is refused while
reading the request, and the person sees which field was wrong. Slot times
are wall-clock times without a time zone: a 6 pm class is at 6 pm at its
branch, whatever the date. Classes can't cross midnight.

**Consequences:** attendance will turn slots into dated sessions using the
branch's (or studio's) time zone.

## 2026-11-23: Two date conventions, on purpose

**Context:** when a student left a class, and until when credits last.

**What wasn't obvious:** "ends on" can mean the last day included or the
first day excluded, and mixing them up loses or adds a day.

**What we did:** each field says which it is, in the migration and the
code. `enrollments.ended_on` is the day they left: from that day on
they're not in the class (so leaving "today" means today's class doesn't
expect them; leaving before the start date makes `ended_on = starts_on`,
zero days). `credit_lots.expires_on` is the last day the credits can be
used, because that's what "use by 6 Jan" means to a person. A 30-day pack
sold today expires on today + 29.

**Consequences:** read the column comment before comparing these dates.

## 2026-11-23: Error messages are sentences, never internals

**Context:** the API's 400 and 409 answers.

**What wasn't obvious:** database errors carried constraint names
(`rooms_org_id_branch_id_name_key`) and JSON errors carried parser text
("the 'hour' component could not be parsed at line 1 column 110").
Neither helps a person, and both describe our internals to anyone
probing.

**What we did:** `ncrm_core::error` maps each known constraint to a
sentence ("That branch already has a room with that name.") and unknown
ones to a generic one. The JSON body reader in `extract.rs` reports the
field instead ("slots[0].starts_at isn't in the right format.",
"Something's missing: name."). Conflicts found by the application itself
already were sentences ("Studio A is booked for Hip-hop Beginners on
Wednesday 18:00–19:00.").

**Consequences:** a new unique constraint should get a line in
`unique_message`, or people see "That already exists."

## 2026-11-23: Front desk sells, managers give

**Context:** who may change class credits.

**What wasn't obvious:** "manage fees" (front desk has it) sounds like it
should cover all credit changes. But selling a pack brings money in,
while giving free classes or taking credits back changes what the studio
is owed. The second is exactly where a studio quietly loses money.

**What we did:** a new capability, "manage classes", for owners and
managers: the timetable, prices, and free or taken-back credits. Both
need a reason, which is stored on the lot or move and the change is
audited. Selling a pack stays with "manage fees".

**Consequences:** reports can later show "classes given free this month,
by whom" straight from the ledger.

## 2026-11-23: Utility classes that did nothing (CSS cascade layers)

**Context:** the student profile and the theme button.

**What wasn't obvious:** `className="btn px-0"` left the button's padding
at 18px, and the theme button's moon icon was squeezed to a 2px sliver.
Tailwind 4 puts its utilities in a CSS *cascade layer*
(`@layer utilities`). Our own `.btn`, `.field`, `.glass` rules were written
outside any layer, and unlayered CSS beats layered CSS whatever the
specificity. So every utility that touched the same property as one of
our classes (`px-0`, `py-3`, `m-0`, `min-h-[44px]`) was silently ignored.
Nothing errors; it just looks slightly wrong.

**What we did:** wrapped the surfaces, controls and list styles in
`@layer components { … }` in `globals.css`. Tailwind declares the order
`theme, base, components, utilities`, so a utility on the same element now
wins, which is what anyone reading the markup expects. The rules inside
the layer keep their order relative to each other, so `.segmented` still
overrides `.glass` where both are used. Rules that must beat everything
(the bottom nav, reduced motion) stay unlayered on purpose.

Also: after editing `globals.css`, `pnpm build` once reused a stale
stylesheet from Turbopack's cache (`.next/cache`) and the change didn't
show. `rm -rf .next` and building again fixed it. If a CSS change doesn't
appear in a production build, check the built file in
`.next/static/chunks/*.css` before debugging the CSS.

**Consequences:** new component classes go inside the `components` layer.
Don't fight a utility with `!important`; if a utility seems ignored, check
whether the competing rule is unlayered.

## 2026-11-23: People are shared, not copied: contacts

**Context:** students and the numbers we message about them.

**What wasn't obvious:** two siblings have the same mother. If each
student stored "parent phone" as a column, the mother is two records; a
fee reminder for both kids is two separate messages, and fixing her number
means finding every copy. And on WhatsApp, the identity is the phone
number, so a number must map to exactly one person per studio.

**What we did:** a `contacts` table, unique on `(org_id, phone)`, and a
link table `student_contacts` with the relation (mother, father, self…)
and an `is_primary` flag. Adding a contact whose number already exists
reuses that row (`INSERT … ON CONFLICT (org_id, phone) DO UPDATE SET phone
= EXCLUDED.phone RETURNING id`; the no-op update is there because
`DO NOTHING` returns no row). The existing name is kept: if the second
sibling's form says "N. Sharma", the mother stays "Neha Sharma" rather
than whoever typed last overwriting her. The profile shows "also for 1
other student" so staff can see the sharing.

A partial unique index allows one primary contact per student. When the
primary is removed, the oldest remaining contact is promoted, so reminders
never silently stop. Up to 4 contacts per student.

**Consequences:** the WhatsApp bot will look up `contacts` by phone and
can answer for all of that person's children at once. Renaming a contact
is a separate action (not built yet), not a side effect of adding a
student.

## 2026-11-23: The app can't delete people

**Context:** what "remove a student" should mean.

**What wasn't obvious:** deleting a student row would take their
attendance and fee history with it, and an accidental tap would be
unrecoverable. But India's DPDP Act also gives people the right to have
their data erased, so "never delete" isn't an answer either.

**What we did:** the `ncrm_app` role has no `DELETE` on `students`,
`contacts` or `staff`. Leaving is a status (`left`, or a deactivated team
member), which keeps history and can be undone. Removing a contact from a
student only deletes the link row. Erasure on request will be a separate,
audited job that runs as the owner role, not a button in the dashboard.

**Consequences:** listed in `docs/PLAN.md`. Most students are minors. The
DPDP Rules 2025 (notified 13 Nov 2025) require verifiable parental consent
before processing a child's data, and that obligation takes effect on
13 May 2027 unless brought forward. Consent capture is planned before
then; whether any exemption applies to a dance studio is a question for a
lawyer, not for us.

## 2026-11-23: A studio must always keep an owner

**Context:** editing team members.

**What wasn't obvious:** "refuse to demote the last owner" sounds like
one `SELECT count(*)`. But two owners can each demote the other at the
same moment: each transaction counts two owners, both pass, both commit,
and the studio has none. Under Postgres's default isolation level, the
two transactions don't see each other's uncommitted changes.

**What we did:** in `staff::update`, the target row is read
`FOR UPDATE`, and before demoting or deactivating an owner we run
`SELECT id FROM staff WHERE role = 'owner' AND active AND id <> $1
FOR UPDATE`. That locks the *other* owners' rows. The second transaction
waits on the lock until the first commits, then re-reads and sees there's
no other owner left, and refuses. Other rules: only owners can create or
grant owner, managers can't edit owners at all, and nobody can change
their own role or deactivate themselves. The test also covers a stale
actor: an owner deactivated in the meantime who tries to demote the last
owner.

**Consequences:** any future rule of the form "there must always be at
least one X" needs the same lock-the-others pattern, not a count.

## 2026-11-23: A test that hung forever: a shadowed transaction

**Context:** the people tests in `ncrm-core/tests/people.rs`.

**What wasn't obvious:** a test opened `let mut tx = TenantTx::begin(…)`,
updated a staff row, then later wrote `let mut tx = TenantTx::begin(…)`
again to check something else. In Rust, the second `let` *shadows* the
first name but doesn't end the first value's life: the first transaction
stayed open until the end of the function, still holding its
`FOR UPDATE` row lock. The second transaction waited for that lock, which
would only be released when the test finished: a deadlock with no error.

**What we did:** `drop(tx)` (rolls back) or `tx.commit()` before starting
the next transaction, with a comment at that spot.

**Consequences:** in tests and in code, finish a transaction explicitly
before opening another on the same rows. If a test hangs instead of
failing, look for two transactions alive at once.

## 2026-11-23: Updates are PUT with the whole object

**Context:** editing students and team members.

**What wasn't obvious:** with PATCH, `{}` means "change nothing" and
`{"branch_id": null}` means "clear the branch". Serde in Rust and JSON in
TypeScript both make it easy to turn one into the other by accident (an
`Option` that's `None` either way, `undefined` dropped by
`JSON.stringify`). A bug there silently erases data.

**What we did:** `PUT /students/{id}` and `PUT /staff/{id}` take every
editable field, every time. The dashboard's edit forms hold the whole
record and send it back. The server compares old and new to work out what
changed (for the audit log and for rules like "a phone change signs them
out").

**Consequences:** two people editing the same student at once: the last
save wins. Fine for a studio's front desk; if it ever isn't, add a
version column and refuse stale writes.

## 2026-11-23: What the audit log records

**Context:** recording team and student changes.

**What wasn't obvious:** an audit log is kept for years and read by
whoever runs support. If entries contain phone numbers, the audit log
becomes a second copy of every contact list, and erasing a person would
mean rewriting history.

**What we did:** entries record who acted, what kind of thing changed and
which fields (`"changed": ["phone"]`), and for team edits the before and
after of role and active when those changed. Never phone numbers, names
of contacts, or notes. Tests check that no phone number appears.

**Consequences:** "what was her old number?" can't be answered from the
log. That's deliberate.

## 2026-11-23: Checks in the core, not only in handlers

**Context:** who may change students and the team.

**What wasn't obvious:** every handler already calls
`staff.require(Capability::…)`. But the WhatsApp bot, an Excel import and
worker jobs will call the same core functions without going through a
handler, and a missing check there would be invisible.

**What we did:** core functions that change people take an `Actor`
(staff id and role) and call `actor.require(…)` themselves, returning
`Error::Forbidden`. The handler check stays too: it refuses before any
database work. Instructors can open the home page, so `/summary` needs
only "view the studio" and returns counts only, never names or numbers.

**Consequences:** instructors will need to see *their own* batches'
students for attendance. That's a narrower rule (planned with classes),
not "manage students".

## 2026-11-23: Search that can't be turned against us

**Context:** the student search box.

**What wasn't obvious:** `ILIKE '%' || $1 || '%'` is safe from SQL
injection because `$1` is a bound parameter, but `%` and `_` inside the
search text are still wildcards: searching for `%` matches everyone, and a
long run of `_%_%_%` can make Postgres work hard.

**What we did:** `%`, `_` and `\` are escaped before the pattern is built
(`ESCAPE '\'`), searches longer than 80 characters are refused (the
dashboard trims to 80 before sending), and pages are at most 100 rows.
If the text has 4 or more digits, we also match contact phone numbers by
digits, so typing the last digits of a parent's number finds the child.
Fewer than 4 digits would match almost everyone.

**Consequences:** fine up to tens of thousands of students per studio.
Beyond that, a trigram index (`pg_trgm`) on names is the next step.

## 2026-11-23: Dates go over JSON as "2026-11-03"

**Context:** date of birth and joined date.

**What wasn't obvious:** the `time` crate's default serde format for a
`Date` is a compact tuple like `[2026, 307]` (year and day of the year),
which no browser understands.

**What we did:** enabled the crate's `serde-human-readable` feature, so
`Date` is written and read as `"2026-11-03"` (ISO 8601). The dashboard
formats it as "3 Nov 2026" in `lib/format.ts` without creating a
JavaScript `Date` from the string, because `new Date("2026-11-03")` is
midnight UTC, which can show as the previous day in some time zones.

**Consequences:** every date field in the API is an ISO date string; every
timestamp is an RFC 3339 string with a time zone.

## 2026-11-23: Next.js 16 is not the Next.js most examples show

**Context:** scaffolding the dashboard with `create-next-app@16.4.6`.

**What wasn't obvious:** the generated `AGENTS.md` warns that APIs and
conventions changed, and points to guides bundled in
`node_modules/next/dist/docs/`. The ones that mattered:
1. `middleware.ts` is deprecated and renamed `proxy.ts` (exported
   function `proxy`). It runs on Node.js by default.
2. New apps turn on `cacheComponents`, which prerenders a static shell for
   every page. The bundled CSP guide says nonce-based CSP needs every page
   rendered per request and is incompatible with prerendered shells,
   because a shell built ahead of time can't carry a per-request nonce.
3. With Cache Components, reading `cookies()` outside `<Suspense>` is a
   build error.

**What we did:** `cacheComponents: false`. Every dashboard page is
per-person data anyway, so prerendering would buy nothing. The root layout
reads `headers()`, which makes every page dynamic, and Next.js then adds
the nonce from the request's CSP header to its own scripts. A production
build lists every route as dynamic (ƒ), and a live check found all 12
scripts on the sign-in page carrying that request's nonce.

**Consequences:** read the bundled guide before touching framework-level
code (`web/README.md` says so too). If a public, cacheable page is ever
needed (a marketing site), it belongs in a separate app.

## 2026-11-23: The dashboard talks to the API through its own `/api` gateway

**Context:** the browser needs to call the API for sign-in and changes.

**What wasn't obvious:** there are three common ways, and two of them
weaken something. Calling the API's domain directly from the browser
needs CORS with credentials and a cross-site cookie. A Next.js `rewrites`
rule forwards everything (all headers, any path), which is hard to reason
about.

**What we did:** a route handler at `web/src/app/api/[...path]/route.ts`
forwards explicitly:
- only top-level paths on an allowlist (`auth`, `branches`), each segment
  matching `[A-Za-z0-9_-]+`, so `..`, encoded slashes and `@host` tricks
  can't escape (unit-tested);
- only allowlisted request headers (cookie, content type, Origin, Fetch
  Metadata, user agent, `x-real-ip`) and response headers (content type,
  cache control, `Set-Cookie`);
- bodies capped at 64 KB, redirects never followed, nothing cached.

Server Components call the API directly (server to server) and forward
only the session cookie. `api-server.ts` and `env.ts` import
`server-only`, so the build fails if the API address ever ends up in a
browser bundle.

**Consequences:** every new top-level API path must be added to
`ALLOWED_PREFIXES` in `web/src/lib/bff.ts`, or the dashboard gets a 404.
Next.js itself normalizes `/api/branches/..` with a 308 to `/api`, which
then 404s; that's harmless.

## 2026-11-23: Cross-site request protection beyond SameSite

**Context:** making the API safe to use from a browser.

**What wasn't obvious:** `SameSite=Lax` plus JSON-only bodies already
block classic CSRF, but both rely on browser behaviour we don't control
(older browsers, subdomain takeovers count as "same site").

**What we did:** a second, independent check on every POST/PUT/PATCH/
DELETE (`ncrm-api/src/security.rs`):
- if the request has an `Origin` header, it must exactly match one of
  `ALLOWED_ORIGINS` (so `null`, look-alike domains and subdomains fail);
- if it has no `Origin` but `Sec-Fetch-Site: cross-site`, it's refused;
- requests with neither (curl, server-to-server) pass, because they can't
  carry a victim's browser cookies.

In production the cookie is `__Host-ncrm_session`. Browsers only accept a
`__Host-` cookie when it's `Secure`, has `Path=/` and no `Domain`, so no
subdomain can plant or overwrite it. Over local http the prefix is
impossible, so dev uses `ncrm_session`. The dashboard reads either name.

**Consequences:** `ALLOWED_ORIGINS` must be set in every environment or the
browser can't sign in (the API logs a warning at startup if it's empty).

## 2026-11-23: Per-IP rate limits and which IP to trust

**Context:** last milestone's limits were per phone number, so one machine
could spread guesses across many numbers.

**What wasn't obvious:** behind a proxy, every request appears to come from
the proxy's address. The real client IP arrives in a header, but anyone can
send that header, so trusting it blindly lets an attacker pick a fresh
"IP" for every request.

**What we did:** in-memory fixed-window counters per (IP, endpoint):
10 code requests and 30 verify attempts per IP per 10 minutes. The IP is
the TCP peer by default. Only when `CLIENT_IP_HEADER` is set is a header
used, and that's only safe because nginx overwrites `X-Real-IP` and is the
only public entrance. A test proves a spoofed header is ignored when not
configured. Expired counters are swept once more than 50,000 are tracked.

**Consequences:** in-memory counters are per process. With more than one
API process, move them to Postgres or Redis. The API and dashboard must
listen on `127.0.0.1` only, or the header can be spoofed by going around
nginx.

## 2026-11-23: Content Security Policy trade-offs

**Context:** strict CSP for the dashboard.

**What wasn't obvious:**
1. Nonces cover `<script>` and `<style>` elements but not `style="…"`
   attributes, which React and Next.js both use (chart bar widths, the
   route announcer). Under `style-src 'nonce-…'` alone they'd be blocked.
2. The CSP example in the docs includes `upgrade-insecure-requests`. On a
   local production run over `http://localhost`, browsers would try to
   load the site's own files over https and fail.
3. Google Fonts would need extra allowed origins and send visitors' IPs to
   Google.

**What we did:**
1. Added `style-src-attr 'unsafe-inline'`: style attributes are allowed,
   `<style>` elements still need the nonce. Attributes can't run script,
   and `default-src 'self'` stops them loading anything from elsewhere.
2. Dropped `upgrade-insecure-requests`: everything is same-origin, and HSTS
   forces HTTPS in production anyway.
3. Fonts are self-hosted with `next/font/local` (files and their SIL Open
   Font Licenses in `web/src/app/fonts/`), so `font-src 'self'` holds.

**Consequences:** any third-party script (analytics, chat widget) needs a
deliberate CSP change and a note here.

## 2026-11-23: An oversized body came back as 400, not 413

**Context:** testing the 64 KB body limit.

**What wasn't obvious:** Axum enforces the limit while the JSON extractor
reads the body, so the failure arrives as a JSON *rejection*. Our `Body`
extractor turned every rejection into `invalid_input` (400), hiding the
real reason.

**What we did:** `Body` checks the rejection's status and maps 413 to a
dedicated `too_large` error. Caught by the test
`oversized_bodies_are_refused`.

**Consequences:** when wrapping an extractor, keep its status codes rather
than collapsing them.

## 2026-11-23: Echoed query strings in the page are not open redirects

**Context:** the live check searched the sign-in page for an attacker's
domain passed as `?next=//evil.example` and found it once.

**What wasn't obvious:** whether that meant the redirect was live.

**What we did:** traced it: Next.js serializes the current URL into its
router state (encoded, as data). The value the sign-in form actually
redirects to is `"/"`, produced by `safeNextPath`. Nothing renders or
follows the raw parameter.

**Consequences:** when checking for open redirects, check the value the
code uses, not whether the string appears in the HTML.

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
