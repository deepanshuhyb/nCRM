"use client";

import type { ApiErrorBody } from "@/lib/types";

/** An API error with the stable code and a message fit to show people. */
export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

/**
 * Calls the API from the browser through the same-origin `/api` proxy.
 * The session cookie travels automatically (it's HttpOnly, so this code
 * never sees it). Bodies are always JSON, which together with the API's
 * origin check blocks cross-site request forgery.
 */
export async function apiSend<T>(
  method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
  path: `/${string}`,
  body?: unknown,
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`/api${path}`, {
      method,
      credentials: "same-origin",
      cache: "no-store",
      headers: body === undefined ? { accept: "application/json" } : {
        accept: "application/json",
        "content-type": "application/json",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "offline", "Can't reach the server. Check your connection and try again.");
  }

  if (res.status === 204) return undefined as T;
  const data = (await res.json().catch(() => null)) as T | ApiErrorBody | null;
  if (res.ok) return data as T;

  const err = (data as ApiErrorBody | null)?.error;
  throw new ApiError(
    res.status,
    err?.code ?? "unknown",
    err?.message ?? "Something went wrong. Try again.",
  );
}
