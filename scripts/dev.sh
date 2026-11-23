#!/usr/bin/env bash
# Run ncrm on your own computer: Postgres, the API and the dashboard, with
# a demo studio you can sign in to.
#
#   ./scripts/dev.sh 9876543210 "Your Name"     # first time: your phone number
#   ./scripts/dev.sh                             # after that
#
# Needs Rust, Node 22+ with pnpm, and either Docker (for Postgres) or a
# Postgres 16 already listening on localhost:5432. Stop with Ctrl+C.
set -euo pipefail
cd "$(dirname "$0")/.."

phone="${1:-}"
name="${2:-Studio Owner}"

missing=0
need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing $1. $2"
    missing=1
  fi
}
need cargo "Install Rust:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
need node "Install Node:  brew install node"
need pnpm "Install pnpm:  npm install -g pnpm"
need openssl "Install OpenSSL (it ships with macOS and most Linux systems)."
[ "$missing" = 0 ] || exit 1

# Settings: created once from the examples, with a fresh secret.
if [ ! -f .env ]; then
  secret="$(openssl rand -base64 48 | tr -d '\n')"
  sed -e "s|^AUTH_SECRET=.*|AUTH_SECRET=${secret}|" \
      -e "s|^BIND_ADDR=.*|BIND_ADDR=127.0.0.1:8080|" .env.example > .env
  echo "Wrote .env with a new AUTH_SECRET."
fi
[ -f web/.env.local ] || cp web/.env.example web/.env.local
set -a
# shellcheck disable=SC1091
. ./.env
set +a

port_open() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

# Postgres: use what's already on 5432, else start it with Docker.
if ! port_open 5432; then
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    echo "Starting Postgres with Docker..."
    docker compose up -d postgres
    for _ in $(seq 1 30); do port_open 5432 && break; sleep 1; done
    sleep 2
  else
    cat <<'HELP'
No database is running on localhost:5432. Either:

  1. Start Docker Desktop, then run this script again, or
  2. Install Postgres yourself:
       brew install postgresql@16 && brew services start postgresql@16
       psql postgres -c "CREATE ROLE ncrm LOGIN SUPERUSER PASSWORD 'ncrm'"
       psql postgres -c "CREATE DATABASE ncrm OWNER ncrm"
HELP
    exit 1
  fi
fi
port_open 5432 || { echo "Postgres didn't come up on localhost:5432."; exit 1; }

for port in 8080 3000; do
  if port_open "$port"; then
    echo "Something is already using port $port. Stop it (or an earlier run of this script) first."
    exit 1
  fi
done

echo "Building the API (the first build takes a few minutes)..."
cargo build -q -p ncrm-api

if [ -n "$phone" ]; then
  ./target/debug/ncrm-dev-seed "$phone" "$name"
fi

[ -d web/node_modules ] || (cd web && pnpm install)

# Run both, each line labelled, and stop both together. `kill 0` signals
# every process this script started (they share its process group).
cleanup() {
  trap - INT TERM EXIT
  echo
  echo "Stopping..."
  kill 0 2>/dev/null || true
}
trap cleanup INT TERM EXIT

RUN_MIGRATIONS=1 ./target/debug/ncrm-api 2>&1 | sed -u 's/^/[api] /' &
(cd web && PORT=3000 pnpm dev 2>&1 | sed -u 's/^/[web] /') &

for _ in $(seq 1 60); do port_open 3000 && port_open 8080 && break; sleep 1; done
cat <<BANNER

  ncrm is running.

    Dashboard:   http://localhost:3000   (use localhost, not 127.0.0.1)
    Sign in:     with the phone number you seeded${phone:+ ($phone)}
    The code:    appears below on a line like
                 [api] [ncrm dev] sign-in code for +91...: 123456

  No studio yet? Stop this (Ctrl+C) and run:  ./scripts/dev.sh <your phone number> "Your Name"
  Stop everything with Ctrl+C.

BANNER
wait
