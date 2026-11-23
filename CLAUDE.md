# Working on ncrm

ncrm is a studio management product with WhatsApp built in: CRM,
attendance for students and teachers, reminders and integrations, for
class-based businesses. The first customer is a dance studio with three
branches. Backend in Rust (`crates/`), database PostgreSQL, dashboard in
Next.js 16 (`web/`, read `web/AGENTS.md` before touching framework code).

Read these before changing anything:

- `docs/ARCHITECTURE.md`: how the pieces fit and connect.
- `docs/TENANCY.md`: how studios are kept apart. Non-negotiable rules.
- `docs/NOTES.md`: decisions, trade-offs and gotchas so far.
- `docs/DESIGN.md`: the visual language for the dashboard.
- `docs/PLAN.md`: the milestone checklist.

## Document as you go (the owner asked for this explicitly)

Every change that touches architecture is documented in the same commit:

- **New component, connection, data flow or external service** → update
  `docs/ARCHITECTURE.md` (diagram, request flow, configuration table).
- **Anything that was not straightforward** (a trade-off, a security
  decision, a library quirk, a bug whose cause was surprising, something
  that needed extra understanding) → add a dated entry to `docs/NOTES.md`
  using the template at the top of that file.
- **Finished a plan item** → tick it in `docs/PLAN.md`.

Write for someone who knows TypeScript and web development but is new to
Rust and Postgres internals: explain the why, not only the what.

## Rules that keep data safe

- Studio data is only read or written through `TenantTx`. Functions that
  touch studio data take `&mut TenantTx`.
- New tables follow the checklist in `docs/TENANCY.md`; the guard test
  `every_table_is_protected_by_row_level_security` must keep passing.
- Cross-studio queries live only in `ncrm_core::auth` (sign-in) and,
  later, clearly named worker jobs. Each one is documented in
  `docs/ARCHITECTURE.md`.
- Handlers check the role with `staff.require(Capability::…)` before
  doing anything.
- Never log or store sign-in codes or session tokens in the clear. The
  `Log` code sender is for local development only.
- Never commit `.env` or any secret.
- The dashboard never calls the API's address from the browser: browser
  code uses `/api/...` (the gateway), server code uses `lib/api-server.ts`.
  New top-level API paths go on the gateway allowlist in `web/src/lib/bff.ts`.
- No third-party scripts, fonts or origins in the dashboard without a CSP
  change and a `docs/NOTES.md` entry.

## Before every commit

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
DATABASE_URL=postgres://… cargo test --workspace
cd web && pnpm lint && pnpm typecheck && pnpm test && pnpm build
```

Every feature ships with tests against a real Postgres (`#[sqlx::test]`),
including the ways it should refuse: wrong studio, wrong role, bad input.
