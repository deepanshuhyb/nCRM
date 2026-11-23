import "server-only";

/**
 * Server-side settings. `server-only` makes the build fail if this file is
 * ever imported into code that runs in the browser, so the API's internal
 * address can't leak into a client bundle.
 */
export function apiOrigin(): string {
  const value = process.env.API_ORIGIN;
  if (!value) {
    throw new Error("API_ORIGIN is not set (for example http://127.0.0.1:8080)");
  }
  const url = new URL(value);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error("API_ORIGIN must be an http(s) URL");
  }
  return url.origin;
}
