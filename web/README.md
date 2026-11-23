# ncrm dashboard

Next.js 16 app for studio owners and staff. It talks to the Rust API only
through its own `/api/*` route, so the browser never contacts the API
directly. How it fits together and why: `../docs/ARCHITECTURE.md`
("The dashboard") and `../docs/NOTES.md`.

This Next.js version differs from older ones (`proxy.ts` instead of
middleware, Cache Components). Read the guides in
`node_modules/next/dist/docs/` before changing framework-level code.

## Run it

Start the API first (see the root README) with
`ALLOWED_ORIGINS=http://localhost:3000` and `COOKIE_SECURE=0`, then:

```sh
cp .env.example .env.local
pnpm install
pnpm dev            # http://localhost:3000
```

## Checks

```sh
pnpm lint
pnpm typecheck
pnpm test           # security helpers: redirects, proxy paths, CSP
pnpm build
pnpm audit --prod
```
