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
- [ ] Staff sign-in with a one-time code
- [ ] Role checks (owner, manager, front desk, instructor)
- [ ] Next.js dashboard shell and generated API client
- [ ] Meta business verification started (owner task)

## 1. Core records

- [ ] Students and guardians, one family profile, shared phone numbers
- [ ] Teachers
- [ ] Rooms, batches, schedules
- [ ] Plans: monthly batch fee, class pack, drop-in
- [ ] Enrollments and the credit ledger for packs
- [ ] Excel import with duplicate detection by phone number
- [ ] Dashboard screens for all of the above

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
- [ ] Events, make-up credits, progress tracking
