import type { NextRequest } from "next/server";

import {
  apiTarget,
  FORWARDED_REQUEST_HEADERS,
  maxBodyBytes,
  pickHeaders,
  readCapped,
  RETURNED_RESPONSE_HEADERS,
} from "@/lib/bff";
import { apiOrigin } from "@/lib/env";

/**
 * Same-origin gateway to the Rust API: `/api/branches` → `${API_ORIGIN}/branches`.
 *
 * Only allowlisted paths, request headers and response headers pass. Bodies
 * are capped, redirects are never followed, nothing is cached. The API does
 * the real security checks (session, studio, role, origin); this layer just
 * keeps the surface small. Full flow: docs/ARCHITECTURE.md.
 */
async function forward(request: NextRequest, ctx: RouteContext<"/api/[...path]">) {
  const { path } = await ctx.params;
  const target = apiTarget(apiOrigin(), path, request.nextUrl.search);
  if (!target) {
    return problem(404, "not_found", "That doesn't exist.");
  }

  let body: Uint8Array<ArrayBuffer> | undefined;
  if (request.method !== "GET" && request.method !== "HEAD") {
    const limit = maxBodyBytes(path);
    const declared = Number(request.headers.get("content-length") ?? "0");
    if (declared > limit) {
      return problem(413, "too_large", "That request is too large.");
    }
    const read = await readCapped(request.body, limit);
    if (read === null) {
      return problem(413, "too_large", "That request is too large.");
    }
    body = read as Uint8Array<ArrayBuffer>;
  }

  let upstream: Response;
  try {
    upstream = await fetch(target, {
      method: request.method,
      headers: pickHeaders(request.headers, FORWARDED_REQUEST_HEADERS),
      body,
      redirect: "manual",
      cache: "no-store",
      signal: AbortSignal.timeout(30_000),
    });
  } catch {
    return problem(502, "api_unreachable", "The server isn't answering. Try again in a moment.");
  }

  const headers = pickHeaders(upstream.headers, RETURNED_RESPONSE_HEADERS);
  for (const cookie of upstream.headers.getSetCookie()) {
    headers.append("set-cookie", cookie);
  }
  headers.set("cache-control", "no-store");
  headers.set("x-content-type-options", "nosniff");
  return new Response(upstream.body, { status: upstream.status, headers });
}

function problem(status: number, code: string, message: string) {
  return Response.json(
    { error: { code, message } },
    { status, headers: { "cache-control": "no-store" } },
  );
}

export { forward as GET, forward as POST, forward as PUT, forward as PATCH, forward as DELETE };
