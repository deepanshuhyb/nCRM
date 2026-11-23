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

You need Rust (stable) and Docker.

```sh
docker compose up -d          # start Postgres
cp .env.example .env          # then put a real AUTH_SECRET in .env
export $(grep -v '^#' .env | xargs)
cargo run -p ncrm-api         # http://localhost:8080/healthz
```

Then the dashboard, in another terminal:

```sh
cd web && cp .env.example .env.local && pnpm install && pnpm dev
# open http://localhost:3000
```

Try signing in (add a studio and a staff member with your number first;
there is no signup screen yet):

```sh
psql "$DATABASE_URL" -c "INSERT INTO organizations (name, slug) VALUES ('My Studio', 'my-studio')"
psql "$DATABASE_URL" -c "INSERT INTO staff (org_id, phone, name, role)
  SELECT id, '+91XXXXXXXXXX', 'Me', 'owner' FROM organizations WHERE slug = 'my-studio'"

curl -X POST localhost:8080/auth/code -H 'content-type: application/json' \
  -d '{"phone":"XXXXXXXXXX"}'
# the code is printed in the API's console (CODE_SENDER=log)
curl -c jar -X POST localhost:8080/auth/verify -H 'content-type: application/json' \
  -d '{"phone":"XXXXXXXXXX","code":"123456"}'
curl -b jar localhost:8080/branches
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
