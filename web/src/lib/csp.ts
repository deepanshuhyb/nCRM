/**
 * The Content Security Policy for dashboard pages.
 *
 * Scripts only run if they carry this request's nonce (Next.js adds it to
 * its own scripts automatically). Everything else (fonts, styles, API
 * calls) must come from our own origin. No framing, no plugins, no base-tag
 * tricks, forms only post to us.
 *
 * `style-src-attr 'unsafe-inline'` allows inline `style="…"` attributes
 * (used for chart bar widths and by Next.js itself). Attributes can't run
 * script or load anything from another origin under this policy, so the
 * risk is cosmetic. `<style>` elements still need the nonce.
 */
export function buildCsp(nonce: string, isDev: boolean): string {
  const directives = [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${isDev ? " 'unsafe-eval'" : ""}`,
    `style-src 'self' ${isDev ? "'unsafe-inline'" : `'nonce-${nonce}'`}`,
    "style-src-attr 'unsafe-inline'",
    "img-src 'self' data: blob:",
    "font-src 'self'",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "manifest-src 'self'",
    "worker-src 'self'",
  ];
  // No `upgrade-insecure-requests`: everything is same-origin, HSTS already
  // forces HTTPS in production, and it would break a local http run.
  return directives.join("; ");
}

/** A fresh, unguessable nonce: 128 random bits, base64. */
export function newNonce(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return btoa(String.fromCharCode(...bytes));
}
