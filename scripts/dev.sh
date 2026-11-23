#!/usr/bin/env bash
# Run ncrm on your own computer: Postgres, the API and the dashboard, with
# a demo studio you can sign in to.
#
#   ./scripts/dev.sh --install 9876543210 "Your Name"   # first time on a Mac
#   ./scripts/dev.sh 9876543210 "Your Name"             # tools already installed
#   ./scripts/dev.sh                                     # after that
#   ./scripts/dev.sh --fast                              # as fast as it is for real
#
# Without --fast the dashboard runs in development mode: every page is
# compiled the first time you open it, which makes the first visit slow and
# animations stutter. --fast builds the dashboard once (about a minute) and
# runs the built app, which is how it behaves in production. Use it when
# judging how the app feels.
#
# The phone number becomes the demo studio's owner; sign in with it.
# --install uses Homebrew to add whatever is missing (Postgres 16, Node,
# pnpm) and rustup for Rust. Without it, the script only tells you what's
# missing. Postgres can also come from Docker, or be one you already run on
# localhost:5432. Stop with Ctrl+C.
set -euo pipefail
cd "$(dirname "$0")/.."

install=0
fast=0
while [ "${1:-}" = "--install" ] || [ "${1:-}" = "--fast" ]; do
  [ "$1" = "--install" ] && install=1
  [ "$1" = "--fast" ] && fast=1
  shift
done
phone="${1:-}"
name="${2:-Studio Owner}"

have() { command -v "$1" >/dev/null 2>&1; }
port_open() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

# Tools installed earlier in this same terminal aren't on PATH yet.
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if have brew; then
  pg_prefix="$(brew --prefix postgresql@16 2>/dev/null || true)"
  [ -n "$pg_prefix" ] && [ -d "$pg_prefix/bin" ] && PATH="$pg_prefix/bin:$PATH"
fi

if [ "$install" = 1 ]; then
  if ! have brew; then
    cat <<'HELP'
Homebrew isn't installed. Install it first (it asks for your Mac password):

  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

Then run this script again.
HELP
    exit 1
  fi
  have node || brew install node
  have pnpm || brew install pnpm
  if ! have cargo; then
    echo "Installing Rust with rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
  if ! port_open 5432 && ! (have docker && docker info >/dev/null 2>&1); then
    brew list postgresql@16 >/dev/null 2>&1 || brew install postgresql@16
    brew services start postgresql@16
    PATH="$(brew --prefix postgresql@16)/bin:$PATH"
    for _ in $(seq 1 30); do port_open 5432 && break; sleep 1; done
  fi
fi

missing=0
need() {
  if ! have "$1"; then
    echo "Missing $1. $2"
    missing=1
  fi
}
need cargo "Install Rust:  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
need node "Install Node:  brew install node"
need pnpm "Install pnpm:  brew install pnpm"
need openssl "Install OpenSSL (it ships with macOS and most Linux systems)."
if [ "$missing" != 0 ]; then
  echo "Or let this script install them:  ./scripts/dev.sh --install <your phone number> \"Your Name\""
  exit 1
fi

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

# Postgres: use what's already on 5432, else start it with Docker.
if ! port_open 5432; then
  if have docker && docker info >/dev/null 2>&1; then
    echo "Starting Postgres with Docker..."
    docker compose up -d postgres
    for _ in $(seq 1 30); do port_open 5432 && break; sleep 1; done
    sleep 2
  else
    cat <<'HELP'
No database is running on localhost:5432. The easiest fix on a Mac:

  ./scripts/dev.sh --install <your phone number> "Your Name"

That installs Postgres 16 with Homebrew and starts it. (Docker Desktop
running also works: the script starts Postgres in it.)
HELP
    exit 1
  fi
fi
port_open 5432 || { echo "Postgres didn't come up on localhost:5432."; exit 1; }

# A Postgres you run yourself (Homebrew, Postgres.app) won't have ncrm's
# user and database yet. Create them if we can't connect.
if have psql && ! psql "$DATABASE_URL" -tAc "SELECT 1" >/dev/null 2>&1; then
  echo "Creating the ncrm database user and database..."
  psql postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname = 'ncrm'" | grep -q 1 \
    || psql postgres -qc "CREATE ROLE ncrm LOGIN SUPERUSER PASSWORD 'ncrm'"
  psql postgres -tAc "SELECT 1 FROM pg_database WHERE datname = 'ncrm'" | grep -q 1 \
    || psql postgres -qc "CREATE DATABASE ncrm OWNER ncrm"
  psql "$DATABASE_URL" -tAc "SELECT 1" >/dev/null \
    || { echo "Still can't connect with DATABASE_URL from .env. Check the Postgres on port 5432."; exit 1; }
fi

for port in 8080 3000; do
  if port_open "$port"; then
    echo "Something is already using port $port. Stop it (or an earlier run of this script) first."
    exit 1
  fi
done

echo "Building the API (the first build takes a few minutes)..."
cargo build -q -p ncrm-api -p ncrm-worker

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

if [ "$fast" = 1 ]; then
  echo "Building the dashboard (about a minute)..."
  (cd web && rm -rf .next && pnpm -s build >/dev/null)
  web_cmd="start"
else
  web_cmd="dev"
fi

RUN_MIGRATIONS=1 ./target/debug/ncrm-api 2>&1 | sed -u 's/^/[api] /' &
(cd web && PORT=3000 pnpm "$web_cmd" 2>&1 | sed -u 's/^/[web] /') &

for _ in $(seq 1 60); do port_open 3000 && port_open 8080 && break; sleep 1; done
# The worker starts once the API has applied the migrations. It makes fee
# reminders and re-sends WhatsApp messages; every minute here, so you don't
# wait five to see it work.
WORKER_INTERVAL_SECS=60 ./target/debug/ncrm-worker 2>&1 | sed -u 's/^/[worker] /' &
cat <<BANNER

  ncrm is running.

    Dashboard:   http://localhost:3000   (use localhost, not 127.0.0.1)
    Sign in:     with the phone number you seeded${phone:+ ($phone)}
    The code:    appears below on a line like
                 [api] [ncrm dev] sign-in code for +91...: 123456

    WhatsApp:    test mode unless .env has the WHATSAPP_ settings: messages
                 are saved and shown, and reach nobody. Play a parent with
                 ./target/debug/ncrm-admin whatsapp say rhythm-house "60001 00000" "hi"
                 (in another terminal, after: set -a; . ./.env; set +a)

  No studio yet? Stop this (Ctrl+C) and run:  ./scripts/dev.sh <your phone number> "Your Name"
  Stop everything with Ctrl+C.

BANNER
wait
