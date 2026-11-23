# ncrm

Studio management with WhatsApp built in: CRM, attendance, reminders and
integrations for class-based businesses.

- How it fits together: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Decisions and gotchas: [docs/NOTES.md](docs/NOTES.md)
- How studios are kept apart: [docs/TENANCY.md](docs/TENANCY.md)
- Design language: [docs/DESIGN.md](docs/DESIGN.md)
- Plan and progress: [docs/PLAN.md](docs/PLAN.md)

## Layout

| Path | What it is |
|---|---|
| `crates/ncrm-core` | Domain logic and all database access |
| `crates/ncrm-api` | HTTP API (Axum) |
| `crates/ncrm-worker` | Background jobs |
| `migrations` | SQL migrations, applied in order |
| `web` | The dashboard (Next.js), see `web/README.md` |

## Run it locally

One command starts Postgres, the API and the dashboard, and creates a demo
studio ("Rhythm House": three branches, classes, prices, students) with
you as its owner:

```sh
./scripts/dev.sh 9876543210 "Your Name"    # your own phone number
```

Open http://localhost:3000 and sign in with that number. No message is
sent in development: the 6-digit code is printed in the same terminal, on
a line like `[api] [ncrm dev] sign-in code for +91...: 123456`.

After the first time, `./scripts/dev.sh` alone is enough. Run it with
another number to add another owner. Ctrl+C stops everything.

You need:

- Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Node 22+ and pnpm: `brew install node && npm install -g pnpm`
- Postgres 16, either through Docker Desktop (the script starts it), or
  your own:

  ```sh
  brew install postgresql@16 && brew services start postgresql@16
  psql postgres -c "CREATE ROLE ncrm LOGIN SUPERUSER PASSWORD 'ncrm'"
  psql postgres -c "CREATE DATABASE ncrm OWNER ncrm"
  ```

The script writes `.env` (with a fresh `AUTH_SECRET`) and
`web/.env.local` from the examples the first time. To start again from an
empty database: `docker compose down -v` (or drop and recreate the `ncrm`
database), then run the script with your number.

Other roles to try: the seeder also creates a manager, a front desk and
three instructors with made-up numbers. To sign in as one, look up the
number (`psql "$DATABASE_URL" -c "SELECT name, role, phone FROM staff"`)
and use it; the code still appears in your terminal.

## Tests

Tests run against a real Postgres. Each test creates its own database, so
the user in `DATABASE_URL` must be allowed to create databases.

```sh
export DATABASE_URL=postgres://ncrm:ncrm@localhost:5432/ncrm
cargo test --workspace
```

Before pushing:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```
