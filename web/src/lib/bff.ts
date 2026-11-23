/**
 * Pure helpers for the `/api/*` proxy in `src/app/api/[...path]/route.ts`.
 * Kept separate so they can be unit-tested without a server.
 *
 * The browser only ever talks to the dashboard's own origin. The proxy
 * forwards a narrow set of requests to the Rust API, so the session cookie
 * stays first-party, no CORS is needed, and the API never has to be
 * reachable from the internet.
 */

/** Top-level API paths the dashboard may reach. Add new ones here. */
export const ALLOWED_PREFIXES = [
  "auth",
  "batches",
  "branches",
  "enrollments",
  "plans",
  "rooms",
  "staff",
  "students",
  "summary",
] as const;

/** Request headers passed on to the API. Everything else is dropped. */
export const FORWARDED_REQUEST_HEADERS = [
  "accept",
  "content-type",
  "cookie",
  "origin",
  "sec-fetch-site",
  "user-agent",
  // Set by our own nginx (it overwrites any value the client sends). Only
  // trustworthy when the dashboard is not reachable except through nginx.
  "x-real-ip",
] as const;

/** Response headers passed back to the browser. */
export const RETURNED_RESPONSE_HEADERS = ["content-type", "cache-control", "retry-after"] as const;

/** Largest request body forwarded, matching the API's own limit. */
export const MAX_BODY_BYTES = 64 * 1024;

/** Spreadsheet uploads (`/students/import...`) may be larger, as in the API. */
export const MAX_UPLOAD_BYTES = 2 * 1024 * 1024 + 4 * 1024;

/** The body limit for a path: uploads get more room, everything else 64 KB. */
export function maxBodyBytes(segments: string[]): number {
  return segments[0] === "students" && segments[1] === "import" ? MAX_UPLOAD_BYTES : MAX_BODY_BYTES;
}

/**
 * Read a request body, stopping as soon as it passes `limit` bytes. A
 * declared Content-Length can lie or be missing (chunked uploads), so the
 * bytes are counted as they arrive instead of trusting the header.
 */
export async function readCapped(body: ReadableStream<Uint8Array> | null, limit: number): Promise<Uint8Array | null> {
  if (!body) return new Uint8Array(0);
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.byteLength;
  }
  return out;
}

const SEGMENT = /^[A-Za-z0-9_-]+$/;

/**
 * Build the API URL for `/api/<segments>`, or `null` if the path is not one
 * the dashboard may reach. Segments are strictly validated, so `..`,
 * encoded slashes and other tricks can't climb out of the API.
 */
export function apiTarget(apiOrigin: string, segments: string[], search: string): URL | null {
  if (segments.length === 0 || segments.length > 8) return null;
  if (!segments.every((s) => SEGMENT.test(s))) return null;
  if (!(ALLOWED_PREFIXES as readonly string[]).includes(segments[0])) return null;
  const base = new URL(apiOrigin);
  const target = new URL(`/${segments.join("/")}${search}`, base);
  if (target.origin !== base.origin) return null;
  return target;
}

/** Copy only the allowed headers. */
export function pickHeaders(from: Headers, allowed: readonly string[]): Headers {
  const out = new Headers();
  for (const name of allowed) {
    const value = from.get(name);
    if (value !== null) out.set(name, value);
  }
  return out;
}
