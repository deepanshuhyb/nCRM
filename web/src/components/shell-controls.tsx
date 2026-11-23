"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useState } from "react";

import { apiSend } from "@/lib/api-client";

export type NavItem = { href: string; label: string };

function isActive(pathname: string, href: string) {
  return href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${href}/`);
}

export function TopNav({ items }: { items: NavItem[] }) {
  const pathname = usePathname();
  return (
    <nav aria-label="Main" className="top-nav segmented glass">
      {items.map((item) => (
        <Link key={item.href} href={item.href} aria-current={isActive(pathname, item.href) ? "page" : undefined}>
          {item.label}
        </Link>
      ))}
    </nav>
  );
}

export function BottomNav({ items }: { items: NavItem[] }) {
  const pathname = usePathname();
  return (
    <nav
      aria-label="Main"
      className="bottom-nav glass"
      style={{ gridTemplateColumns: `repeat(${items.length}, minmax(0, 1fr))` }}
    >
      {items.map((item) => (
        <Link key={item.href} href={item.href} aria-current={isActive(pathname, item.href) ? "page" : undefined}>
          {item.label}
        </Link>
      ))}
    </nav>
  );
}

export function StudioSwitcher({
  current,
  studios,
}: {
  current: string;
  studios: { orgId: string; name: string }[];
}) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);

  if (studios.length < 2) return null;

  async function change(orgId: string) {
    setBusy(true);
    try {
      await apiSend("POST", "/auth/studio", { org_id: orgId });
      router.replace("/");
      router.refresh();
    } finally {
      setBusy(false);
    }
  }

  return (
    <label className="inline-flex items-center gap-2">
      <span className="sr-only">Studio</span>
      <select className="select" value={current} disabled={busy} onChange={(e) => change(e.target.value)}>
        {studios.map((s) => (
          <option key={s.orgId} value={s.orgId}>
            {s.name}
          </option>
        ))}
      </select>
    </label>
  );
}

export function ThemeToggle({ initial }: { initial: "daylight" | "emerald" }) {
  const [theme, setTheme] = useState(initial);

  function toggle() {
    const nextTheme = theme === "daylight" ? "emerald" : "daylight";
    setTheme(nextTheme);
    document.documentElement.dataset.theme = nextTheme;
    const secure = location.protocol === "https:" ? "; secure" : "";
    document.cookie = `ncrm_theme=${nextTheme}; path=/; max-age=31536000; samesite=lax${secure}`;
  }

  return (
    <button
      type="button"
      className="btn btn-ghost min-h-[44px] w-[44px] px-0"
      onClick={toggle}
      aria-label={theme === "daylight" ? "Switch to dark theme" : "Switch to light theme"}
      title={theme === "daylight" ? "Dark theme" : "Light theme"}
    >
      {theme === "daylight" ? (
        <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none">
          <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" />
        </svg>
      ) : (
        <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="4" stroke="currentColor" strokeWidth="2" />
          <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
      )}
    </button>
  );
}

export function SignOutButton() {
  const router = useRouter();
  const [busy, setBusy] = useState(false);

  async function signOut() {
    setBusy(true);
    try {
      await apiSend("POST", "/auth/sign-out");
    } finally {
      router.replace("/sign-in");
      router.refresh();
    }
  }

  return (
    <button type="button" className="btn btn-ghost min-h-[44px]" onClick={signOut} disabled={busy}>
      {busy ? "Signing out…" : "Sign out"}
    </button>
  );
}
