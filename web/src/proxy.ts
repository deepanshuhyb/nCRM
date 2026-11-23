import { NextResponse, type NextRequest } from "next/server";

import { buildCsp, newNonce } from "@/lib/csp";

/** Session cookie names: `__Host-` in production (HTTPS), plain in local dev. */
export const SESSION_COOKIES = ["__Host-ncrm_session", "ncrm_session"] as const;

/** Pages anyone may open without signing in. */
const PUBLIC_PAGES = ["/sign-in"];

/**
 * Runs before every page:
 * 1. Generates a fresh CSP nonce and sets the Content Security Policy.
 * 2. Sends people without a session cookie to the sign-in page.
 *
 * Step 2 is only a convenience. Whether the cookie is *valid* is decided by
 * the API on every request; the app layout redirects if it isn't.
 */
export function proxy(request: NextRequest) {
  const { pathname, search } = request.nextUrl;
  const hasSession = SESSION_COOKIES.some((name) => request.cookies.has(name));
  const isPublic = PUBLIC_PAGES.some((p) => pathname === p || pathname.startsWith(`${p}/`));

  if (!hasSession && !isPublic) {
    const url = request.nextUrl.clone();
    url.pathname = "/sign-in";
    url.search = pathname === "/" ? "" : `?next=${encodeURIComponent(pathname + search)}`;
    return NextResponse.redirect(url);
  }

  const nonce = newNonce();
  const csp = buildCsp(nonce, process.env.NODE_ENV === "development");

  const requestHeaders = new Headers(request.headers);
  requestHeaders.set("x-nonce", nonce);
  requestHeaders.set("content-security-policy", csp);

  const response = NextResponse.next({ request: { headers: requestHeaders } });
  response.headers.set("content-security-policy", csp);
  // Pages show personal data: never store them in shared caches.
  response.headers.set("cache-control", "private, no-store");
  return response;
}

export const config = {
  matcher: [
    {
      source: "/((?!api|_next/static|_next/image|favicon.ico|robots.txt).*)",
      missing: [
        { type: "header", key: "next-router-prefetch" },
        { type: "header", key: "purpose", value: "prefetch" },
      ],
    },
  ],
};
