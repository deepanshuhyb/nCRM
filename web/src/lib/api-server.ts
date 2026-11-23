import "server-only";

import { cookies } from "next/headers";
import { cache } from "react";
import { redirect } from "next/navigation";

import { apiOrigin } from "@/lib/env";
import type { ApiErrorBody, Branch, Me } from "@/lib/types";

/**
 * Calls the Rust API from Server Components, on behalf of the person whose
 * request is being rendered. Only their session cookie is forwarded; the
 * API decides what they may see.
 *
 * Failures that mean "go somewhere else" (not signed in, no studio picked)
 * become redirects. Anything else throws and lands on the error page,
 * which shows a generic message.
 */
const SESSION_COOKIES = ["__Host-ncrm_session", "ncrm_session"];

async function sessionCookieHeader(): Promise<string | null> {
  const jar = await cookies();
  for (const name of SESSION_COOKIES) {
    const value = jar.get(name)?.value;
    if (value) return `${name}=${value}`;
  }
  return null;
}

export class ApiRequestError extends Error {
  constructor(
    public status: number,
    public code: string,
  ) {
    super(`API request failed: ${status} ${code}`);
  }
}

async function apiGet<T>(path: string): Promise<T> {
  const cookie = await sessionCookieHeader();
  if (!cookie) redirect("/sign-in");

  const res = await fetch(new URL(path, apiOrigin()), {
    headers: { cookie, accept: "application/json" },
    cache: "no-store",
    redirect: "manual",
    signal: AbortSignal.timeout(15_000),
  });

  if (res.ok) return (await res.json()) as T;

  const code = ((await res.json().catch(() => null)) as ApiErrorBody | null)?.error?.code ?? "unknown";
  if (code === "not_signed_in") redirect("/sign-in");
  if (code === "choose_studio") redirect("/choose-studio");
  throw new ApiRequestError(res.status, code);
}

/**
 * The signed-in person, or a redirect to sign-in. `cache` makes repeated
 * calls during one render (layout + page) share a single API request.
 */
export const getMe = cache((): Promise<Me> => apiGet<Me>("/auth/me"));

/** Branches of the current studio. */
export function listBranches(): Promise<Branch[]> {
  return apiGet<Branch[]>("/branches");
}
