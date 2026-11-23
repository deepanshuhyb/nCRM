# ncrm build plan

A studio management system with WhatsApp built in: CRM, attendance for
students and teachers, reminders, and integrations. First customer is a
dance studio with 3 branches; the product is built for any class-based
business.

Stack: Rust (Axum, SQLx, Tokio), PostgreSQL with row-level security, jobs in
Postgres, Next.js dashboard, WhatsApp Cloud API, Razorpay.

## 0. Groundwork

- [x] Cargo workspace: `ncrm-core`, `ncrm-api`, `ncrm-worker`
- [x] Organizations, branches, staff, audit log
- [x] Row-level security with a restricted app role
- [x] Isolation tests against real Postgres
- [x] Phone number normalization
- [x] API health endpoint, worker stub
- [x] CI: format, lint, test
- [x] Staff sign-in with a one-time code (code delivery logs to the console until WhatsApp is wired)
- [x] Role checks (owner, manager, front desk, instructor)
- [x] First studio-scoped endpoints: `/branches` list and create, with audit
- [x] Architecture docs and engineering notes (`docs/ARCHITECTURE.md`, `docs/NOTES.md`)
- [x] Browser-facing API protections: origin check, per-IP limits, `__Host-` cookie, no-store, body limit, timeout
- [x] Next.js dashboard: sign-in, studio picker, app shell, home, branches, Daylight/Emerald themes
- [x] Dashboard security: nonce CSP, security headers, same-origin `/api` gateway, safe redirects
- [x] CI: dashboard lint, typecheck, tests, build; `cargo audit` and `pnpm audit`
- [ ] Meta business verification started (owner task)

## 1. Core records

- [x] Students with contacts (parents, guardians or themselves); one contact per number, shared by siblings
- [x] Team management: hire, change role, phone or branch, deactivate; owner safeguards
- [x] Students and team screens: list, search, add, profile, edit; navigation by role; home counts
- [x] Rooms per branch; classes with weekly schedules; no double-booked rooms or teachers
- [x] Plans: monthly fee, class pack, drop-in
- [x] Enrollments with capacity, and the append-only credit ledger for packs
- [x] Instructors see their own classes' students, not the whole list
- [x] Dashboard: timetable, add and edit classes, prices, rosters, student classes and credits, rooms
- [ ] Rename a contact; merge two contacts that turn out to be one person
- [ ] Excel import with duplicate detection by phone number
- [ ] `students.joined_on` defaults to the studio's date, not the server's

## 2. Attendance and bookings

- [ ] Sessions generated from batch schedules; cancel and reschedule
- [ ] Bookings: automatic for batch members, single for drop-ins
- [ ] Capacity and waitlists
- [ ] Student attendance and teacher check-in
- [ ] Alerts: unmarked class, three absences in a row

## 3. WhatsApp

- [ ] Webhook receiver with duplicate-event protection
- [ ] Outbox table, sending worker, delivery status
- [ ] Identify sender by number; "which child?" for shared numbers
- [ ] Bot: schedule, dues, classes left, book a class, report absence
- [ ] Teachers: mark attendance, report leave
- [ ] Shared inbox with staff takeover
- [ ] AI provider switch, including a studio's own key

## 4. Fees and reminders

- [ ] Dues, receipts, Razorpay payment links
- [ ] Reminder schedules with escalation and fallback
- [ ] Message wallet metering per studio

## 5. Analytics

- [ ] Daily rollup tables
- [ ] Owner home dashboard
- [ ] Morning summary on WhatsApp

## 6. Pilot

- [ ] Run the 3-branch studio on it
- [ ] Measure message cost, AI cost and support time per branch

## 7. Product launch

- [ ] Self-serve signup and Meta embedded signup
- [ ] Subscription billing
- [ ] Lead capture: Instagram, Meta lead ads, forms
- [ ] Sheets sync, accounting export, public API and webhooks
- [ ] nginx config: TLS, `X-Real-IP`, only public listener; API and dashboard on 127.0.0.1
- [ ] Move rate-limit counters to Postgres if we run more than one API process
- [ ] Cleanup job: delete sign-in codes older than a day, expired sessions
- [ ] Verifiable parental consent for students under 18 (DPDP Rules 2025, Rule 10; applies from 13 May 2027). Ask a lawyer whether any exemption covers studios
- [ ] Erase a person on request: audited owner-role job, not a dashboard button
- [ ] Optimistic locking (version column) if two people editing one record becomes a real problem
- [ ] Events, make-up credits, progress tracking
