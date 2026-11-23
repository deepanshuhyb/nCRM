# ncrm

Studio management with WhatsApp built in: CRM, attendance, reminders and
integrations for class-based businesses.

- Plan and progress: [docs/PLAN.md](docs/PLAN.md)
- How studios are kept apart: [docs/TENANCY.md](docs/TENANCY.md)
- Design language: [docs/DESIGN.md](docs/DESIGN.md)

## Layout

| Path | What it is |
|---|---|
| `crates/ncrm-core` | Domain logic and all database access |
| `crates/ncrm-api` | HTTP API (Axum) |
| `crates/ncrm-worker` | Background jobs |
| `migrations` | SQL migrations, applied in order |

## Run it locally

You need Rust (stable) and Docker.

```sh
docker compose up -d          # start Postgres
cp .env.example .env
export $(grep -v '^#' .env | xargs)
cargo run -p ncrm-api         # http://localhost:8080/healthz
```

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
