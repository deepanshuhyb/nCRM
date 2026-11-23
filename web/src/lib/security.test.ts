import { describe, expect, it } from "vitest";

import { apiTarget, FORWARDED_REQUEST_HEADERS, pickHeaders } from "./bff";
import { buildCsp, newNonce } from "./csp";
import { safeNextPath } from "./safe-redirect";

const API = "http://127.0.0.1:8080";

describe("safeNextPath", () => {
  it("keeps paths on this site", () => {
    expect(safeNextPath("/branches")).toBe("/branches");
    expect(safeNextPath("/branches?tab=all#x")).toBe("/branches?tab=all#x");
    expect(safeNextPath("/")).toBe("/");
  });

  it.each([
    "https://evil.example",
    "//evil.example",
    "/\\evil.example",
    "\\\\evil.example",
    "javascript:alert(1)",
    "/%0d%0aSet-Cookie:x",
    "/\u0000evil",
    "evil.example",
    "",
    null,
    undefined,
  ])("sends %s home instead", (input) => {
    const out = safeNextPath(input as string | null | undefined);
    expect(out.startsWith("/")).toBe(true);
    expect(out.startsWith("//")).toBe(false);
    expect(new URL(out, "https://app.example").origin).toBe("https://app.example");
  });

  it("never redirects into the API proxy", () => {
    expect(safeNextPath("/api/auth/sign-out")).toBe("/");
  });
});

describe("apiTarget", () => {
  it("maps allowed paths onto the API origin", () => {
    expect(apiTarget(API, ["auth", "me"], "")?.toString()).toBe(`${API}/auth/me`);
    expect(apiTarget(API, ["branches"], "?active=true")?.toString()).toBe(`${API}/branches?active=true`);
    expect(apiTarget(API, ["students", "0b8f3e1c-1f2a-4c3d-9e8f-123456789abc", "contacts"], "")?.pathname).toBe(
      "/students/0b8f3e1c-1f2a-4c3d-9e8f-123456789abc/contacts",
    );
    expect(apiTarget(API, ["students", "0b8f3e1c-1f2a-4c3d-9e8f-123456789abc", "credits", "take-back"], "")?.pathname).toBe(
      "/students/0b8f3e1c-1f2a-4c3d-9e8f-123456789abc/credits/take-back",
    );
    for (const top of ["batches", "plans", "rooms", "enrollments"]) {
      expect(apiTarget(API, [top], "")?.pathname).toBe(`/${top}`);
    }
  });

  it("reaches attendance, and nothing that only looks like it", () => {
    const id = "0b8f3e1c-1f2a-4c3d-9e8f-123456789abc";
    expect(apiTarget(API, ["sessions"], "?date=2026-11-23")?.toString()).toBe(`${API}/sessions?date=2026-11-23`);
    expect(apiTarget(API, ["sessions", id, "attendance"], "")?.pathname).toBe(`/sessions/${id}/attendance`);
    expect(apiTarget(API, ["sessions", id, "drop-ins", id], "")?.pathname).toBe(`/sessions/${id}/drop-ins/${id}`);
    expect(apiTarget(API, ["attendance", "alerts"], "")?.pathname).toBe("/attendance/alerts");
    expect(apiTarget(API, ["session"], "")).toBeNull();
    expect(apiTarget(API, ["sessions-admin"], "")).toBeNull();
  });

  it.each([
    [[]],
    [["healthz"]],
    [["admin"]],
    [["..", "admin"]],
    [["auth", ".."]],
    [["auth", "%2e%2e"]],
    [["auth", "a/b"]],
    [["auth", "@evil.example"]],
    [["auth", "me:80"]],
    [["auth", "x\\y"]],
    [["auth", ""]],
  ])("refuses %j", (segments) => {
    expect(apiTarget(API, segments as string[], "")).toBeNull();
  });

  it("always stays on the API origin", () => {
    const t = apiTarget(API, ["branches"], "?next=//evil.example");
    expect(t?.origin).toBe(API);
  });
});

describe("pickHeaders", () => {
  it("drops everything not on the list", () => {
    const incoming = new Headers({
      cookie: "__Host-ncrm_session=abc",
      "content-type": "application/json",
      host: "evil.example",
      "x-forwarded-for": "1.2.3.4",
      authorization: "Bearer stolen",
      "x-real-ip": "203.0.113.9",
    });
    const out = pickHeaders(incoming, FORWARDED_REQUEST_HEADERS);
    expect(out.get("cookie")).toBe("__Host-ncrm_session=abc");
    expect(out.get("content-type")).toBe("application/json");
    expect(out.get("host")).toBeNull();
    expect(out.get("x-forwarded-for")).toBeNull();
    expect(out.get("authorization")).toBeNull();
    expect(out.get("x-real-ip")).toBe("203.0.113.9");
  });
});

describe("buildCsp", () => {
  it("only runs scripts with the nonce in production", () => {
    const csp = buildCsp("abc123", false);
    expect(csp).toContain("script-src 'self' 'nonce-abc123' 'strict-dynamic'");
    expect(csp).not.toContain("unsafe-eval");
    expect(csp).not.toMatch(/script-src[^;]*'unsafe-inline'/);
    expect(csp).toContain("frame-ancestors 'none'");
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("base-uri 'self'");
    expect(csp).toContain("form-action 'self'");
    expect(csp).toContain("connect-src 'self'");
  });

  it("allows eval only in development", () => {
    expect(buildCsp("n", true)).toContain("'unsafe-eval'");
  });

  it("makes a new unpredictable nonce every time", () => {
    const seen = new Set(Array.from({ length: 200 }, () => newNonce()));
    expect(seen.size).toBe(200);
    for (const n of seen) expect(atob(n)).toHaveLength(16);
  });
});

describe("format helpers", async () => {
  const { ageFrom, formatDate, formatPhone } = await import("./format");
  it("formats Indian numbers and leaves others alone", () => {
    expect(formatPhone("+919811122233")).toBe("+91 98111 22233");
    expect(formatPhone("+14155552671")).toBe("+14155552671");
  });
  it("works out age on and around birthdays", () => {
    const today = new Date(2026, 10, 23);
    expect(ageFrom("2015-11-23", today)).toBe(11);
    expect(ageFrom("2015-11-24", today)).toBe(10);
    expect(ageFrom("2030-01-01", today)).toBeNull();
    expect(ageFrom("garbage", today)).toBeNull();
  });
  it("formats dates without time zones getting involved", () => {
    expect(formatDate("2026-11-03")).toBe("3 Nov 2026");
  });
});

describe("class and money helpers", async () => {
  const { addDays, formatRupees, isoWeekday, parseRupees, scheduleSummary } = await import("./format");
  it("formats rupees the Indian way", () => {
    expect(formatRupees(12450000)).toBe("₹1,24,500");
    expect(formatRupees(99950)).toBe("₹999.50");
    expect(formatRupees(0)).toBe("₹0");
  });
  it("reads prices people type, and refuses what isn't one", () => {
    expect(parseRupees("1,500")).toBe(150000);
    expect(parseRupees("₹ 999.5")).toBe(99950);
    expect(parseRupees("0")).toBe(0);
    expect(parseRupees("-5")).toBeNull();
    expect(parseRupees("12.345")).toBeNull();
    expect(parseRupees("abc")).toBeNull();
    expect(parseRupees("")).toBeNull();
  });
  it("groups days that share a time", () => {
    expect(
      scheduleSummary([
        { weekday: 1, starts_at: "18:00", ends_at: "19:00" },
        { weekday: 3, starts_at: "18:00", ends_at: "19:00" },
        { weekday: 6, starts_at: "10:00", ends_at: "11:30" },
      ]),
    ).toBe("Mon, Wed 18:00–19:00 · Sat 10:00–11:30");
  });
  it("knows weekdays and adds days without time zones", () => {
    expect(isoWeekday("2026-11-23")).toBe(1);
    expect(isoWeekday("2026-11-29")).toBe(7);
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
  });
});

describe("upload limits at the gateway", async () => {
  const { maxBodyBytes, readCapped, MAX_BODY_BYTES, MAX_UPLOAD_BYTES } = await import("./bff");
  const stream = (sizes: number[]) =>
    new ReadableStream<Uint8Array>({
      start(c) {
        for (const n of sizes) c.enqueue(new Uint8Array(n));
        c.close();
      },
    });
  it("gives only the import paths more room", () => {
    expect(maxBodyBytes(["students", "import"])).toBe(MAX_UPLOAD_BYTES);
    expect(maxBodyBytes(["students", "import", "preview"])).toBe(MAX_UPLOAD_BYTES);
    expect(maxBodyBytes(["students"])).toBe(MAX_BODY_BYTES);
    expect(maxBodyBytes(["students", "0b8f3e1c-1f2a-4c3d-9e8f-123456789abc"])).toBe(MAX_BODY_BYTES);
  });
  it("counts the bytes that arrive instead of trusting the header", async () => {
    expect((await readCapped(stream([10, 20]), 30))?.byteLength).toBe(30);
    expect(await readCapped(stream([10, 20, 1]), 30)).toBeNull();
    expect((await readCapped(null, 30))?.byteLength).toBe(0);
  });
});
