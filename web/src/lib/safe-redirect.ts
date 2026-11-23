/**
 * Turn an untrusted `?next=` value into a path on this site, or `/`.
 *
 * Without this, a link like `/sign-in?next=https://evil.example` would send
 * someone to another site right after they sign in (an open redirect,
 * a favourite phishing trick).
 */
export function safeNextPath(next: string | null | undefined): string {
  if (!next) return "/";
  // Must be a single-slash absolute path. Rejects `//evil.example`,
  // `/\evil.example` (browsers treat `\` like `/`), `https://…`, and
  // `javascript:`.
  if (!next.startsWith("/") || next.startsWith("//") || next.startsWith("/\\")) {
    return "/";
  }
  if (/[\u0000-\u001f\\]/.test(next)) return "/";
  try {
    const url = new URL(next, "http://ncrm.invalid");
    if (url.origin !== "http://ncrm.invalid") return "/";
    if (url.pathname.startsWith("/api/")) return "/";
    return url.pathname + url.search + url.hash;
  } catch {
    return "/";
  }
}
