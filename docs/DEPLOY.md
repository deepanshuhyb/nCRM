# Running ncrm on a server

For one Linux server (a VPS) with Docker and nginx. Everything here is in
the `deploy/` folder.

**Not yet proven on a real server.** These files were written and
checked for syntax, but the machine they were written on has no Docker,
so the images have not been built and the stack has not been started from
them. Expect to fix a detail or two on the first run; tell Claude what
the error says.

**Read this before inviting anyone:** sign-in codes can so far only be
written to the API's log (`CODE_SENDER=log`). That is fine while you are
the only user and can read the log. It is not a way to run a studio:
codes over WhatsApp come with the WhatsApp phase.

## What runs

```
internet ──https──▶ nginx (host, ports 80/443)
                      │  127.0.0.1:3000
                      ▼
                  web  (dashboard, Next.js)        ┐
                      │  http://api:8080           │ Docker network,
                      ▼                            │ nothing else
                  api  (Rust)  ◀── worker (Rust)   │ published
                      │                            │
                      ▼                            │
                  postgres (volume: ncrm-pg)       ┘
```

- **nginx** is the only public listener. It ends TLS and sets
  `X-Real-IP` (overwriting anything the visitor sent).
- **web** is published on `127.0.0.1:3000` only.
- **api** has no published port. The browser never calls it; the
  dashboard's `/api` gateway does. It trusts `X-Real-IP` for rate limits
  because only our own nginx and gateway can reach it.
- **worker** runs housekeeping every few minutes (see
  `docs/ARCHITECTURE.md`, "Background jobs").
- **postgres** keeps its data in the `ncrm-pg` volume.

## First time

1. Point a DNS record (for example `app.yourstudio.in`) at the server.
2. On the server: install Docker (with the compose plugin), nginx and
   certbot, then clone the repository.
3. Settings:
   ```sh
   cp deploy/.env.example deploy/.env
   nano deploy/.env        # PUBLIC_ORIGIN, POSTGRES_PASSWORD, AUTH_SECRET
   chmod 600 deploy/.env
   ```
4. Start it:
   ```sh
   docker compose -f deploy/docker-compose.yml --env-file deploy/.env up -d --build
   docker compose -f deploy/docker-compose.yml ps        # all "healthy" / "running"
   ```
   The first build compiles Rust and takes several minutes.
5. nginx: copy `deploy/nginx.conf` to `/etc/nginx/sites-available/ncrm`,
   replace `app.example.com`, link it into `sites-enabled`, run
   `sudo certbot --nginx -d app.yourstudio.in`, then
   `sudo nginx -t && sudo systemctl reload nginx`.
6. Create the studio and its first owner. There is no sign-up page yet,
   so this is done once by hand; ask Claude for the exact command for
   your studio's name and number (it creates one `organizations` row and
   one owner in `staff`). Do **not** run `ncrm-dev-seed` on a server: it
   fills a demo studio and refuses to run with secure cookies on.
7. Open the address, enter your number, and read the code from
   `docker compose -f deploy/docker-compose.yml logs api | grep "sign-in code"`.

## Updating

```sh
git pull
docker compose -f deploy/docker-compose.yml --env-file deploy/.env up -d --build
```

Database migrations run when the API starts (`RUN_MIGRATIONS=1`). They
only ever add; nothing is dropped.

## Backups

The data is the `ncrm-pg` volume. A nightly dump is the minimum:

```sh
docker compose -f deploy/docker-compose.yml exec -T postgres \
  pg_dump -U ncrm -Fc ncrm > /var/backups/ncrm-$(date +%F).dump
```

Put that in root's crontab, copy the files off the server, and try a
restore once before you rely on it.

## Settings

| Variable | Where | What |
|---|---|---|
| `PUBLIC_ORIGIN` | `deploy/.env` | The dashboard's address. Requests that change anything are refused from any other origin |
| `POSTGRES_PASSWORD` | `deploy/.env` | Database password |
| `AUTH_SECRET` | `deploy/.env` | Signs sign-in codes. Changing it cancels codes that were just sent; people already signed in stay signed in |
| `CODE_SENDER` | `deploy/.env` | `log` until the WhatsApp sender exists |
| `WORKER_INTERVAL_SECS` | `deploy/.env` | Seconds between housekeeping passes (default 300) |

Cookies are `Secure` by default, so sign-in only works over https. More
settings (WhatsApp, Razorpay) are added to `deploy/.env.example` as those
features land; a feature whose keys are missing stays in a test mode
instead of failing.
