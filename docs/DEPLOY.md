# Running ncrm on a server

For one Linux server (a VPS) with Docker and nginx. Everything here is in
the `deploy/` folder.

**Not yet proven on a real server.** These files were written and
checked for syntax, but the machine they were written on has no Docker,
so the images have not been built and the stack has not been started from
them. Expect to fix a detail or two on the first run; tell Claude what
the error says.

**Read this before inviting anyone:** out of the box, sign-in codes are
written to the API's log (`CODE_SENDER=log`). That is fine while you are
the only user and can read the log. To let the team sign in, set up
"Sign-in codes over WhatsApp" below.

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
7. Open the address and enter your number. With `CODE_SENDER=log` the
   code is in
   `docker compose -f deploy/docker-compose.yml logs api | grep "sign-in code"`;
   with WhatsApp set up it arrives on your phone.

## Sign-in codes over WhatsApp

The code is sent by Meta's WhatsApp Cloud API, from a number your Meta
Business account owns. This part has been tested against a stand-in for
Meta's API, not against Meta itself: no credentials existed when it was
written. The first real send is the test.

What you need from Meta (the names of the screens change; search their
help for the term in quotes if a step has moved):

1. A Meta Business account with a "WhatsApp Business Account" and a
   phone number added to it. The number can't be in use on the normal
   WhatsApp app at the same time.
2. The number's **"Phone number ID"** (a long number shown beside it in
   the API setup page). This is `WHATSAPP_PHONE_NUMBER_ID`. It is not the
   phone number.
3. A **"System User" access token** with the
   `whatsapp_business_messaging` permission, set to never expire. This is
   `WHATSAPP_TOKEN`. Treat it like a password.
4. An approved **message template** in the *Authentication* category
   with a *Copy code* button. Name it `ncrm_sign_in_code` and create it
   in English (`en`), or put your own name and language code in
   `WHATSAPP_OTP_TEMPLATE` and `WHATSAPP_TEMPLATE_LANG`. Meta writes the
   message text for this category itself; approval usually takes minutes.

Then in `deploy/.env` set `CODE_SENDER=whatsapp` and the four
`WHATSAPP_` lines, and restart:
`docker compose -f deploy/docker-compose.yml --env-file deploy/.env up -d`.
The API refuses to start if the settings are half filled in, and says
which one is missing.

If a code doesn't arrive, the API log has a line "could not deliver
sign-in code" with Meta's error number and nothing else (never the code,
never the number's token). The common ones: 190, the token is wrong or
expired; 132001, the template name or language doesn't match an approved
template; 131026, that phone isn't on WhatsApp.

Meta charges per authentication message. The amount depends on the
country; check their current price list before a large rollout.

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
| `CODE_SENDER` | `deploy/.env` | `log` or `whatsapp` |
| `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_OTP_TEMPLATE`, `WHATSAPP_TEMPLATE_LANG` | `deploy/.env` | See "Sign-in codes over WhatsApp" |
| `WORKER_INTERVAL_SECS` | `deploy/.env` | Seconds between housekeeping passes (default 300) |

Cookies are `Secure` by default, so sign-in only works over https. More
settings (WhatsApp messages, Razorpay) are added to `deploy/.env.example`
as those features land; a feature whose keys are missing stays in a test mode
instead of failing.
