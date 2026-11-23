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
- [x] Excel and CSV import with a preview, duplicate detection by name and phone, and safe re-import
- [x] `students.joined_on` set from the studio's date; join dates and class starts can be backdated, marked with a pill
- [ ] Import: a Class column that also puts each student in that class

## 2. Attendance and bookings

- [x] Sessions from class schedules, created on demand for today and the past; cancel with a reason and bring back
- [x] Members are expected automatically; drop-ins are added per class and use a credit
- [x] Student attendance (present, late, absent, excused) and teacher check-in with substitutes
- [x] Credits spent when someone comes, returned when the mark changes, owed when there are none
- [x] Alerts: class over and not marked, three absences in a row (on the Today page and home)
- [ ] Reschedule one meeting to another time or room; a one-off extra class
- [ ] Booking a place in a future class (needs sessions ahead of time), capacity for drop-ins, waitlists
- [ ] Students on a break: skip them in the roster between two dates

## 3. WhatsApp

- [ ] Webhook receiver with duplicate-event protection
- [ ] Outbox table, sending worker, delivery status
- [ ] Identify sender by number; "which child?" for shared numbers
- [ ] Bot: schedule, dues, classes left, book a class, report absence
- [ ] Teachers: mark attendance, report leave
- [ ] Shared inbox with staff takeover
- [ ] AI provider switch, including a studio's own key

## 4. Fees and reminders

- [x] Charges and payments in paise; payments settle the oldest dues first, extra is kept as advance
- [x] Monthly fees created on demand once the studio switches them on; packs become charges when sold
- [x] Receipts with gapless numbers per studio, printable; dues list; the day's payments by method
- [x] Cancelling a charge or payment: owners and managers only, with a reason, nothing deleted
- [ ] Discounts and part waivers on a charge (today: cancel it and add a new one)
- [ ] Earlier months for a studio that joins mid-year (today: added by hand as "old balance")
- [ ] Razorpay payment links and automatic matching (needs the studio's keys)
- [ ] Send the receipt on WhatsApp (after phase 3)
- [ ] Reminder schedules with escalation and fallback
- [ ] Message wallet metering per studio

## 5. Analytics

- [x] Report endpoints: attendance by day, joined and left by month, dues by age, collection by day, a class's last meetings
- [x] Owner home dashboard with graphs; graphs on Today, Fees, class and student pages
- [x] Dashboard overhaul (docs/UI_PLAN.md): solid surfaces, type scale, sidebar, loading skeletons, page transitions
- [ ] Daily rollup tables (not needed until a studio is large; reports read live tables)
- [ ] A Reports page with date ranges and export
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
