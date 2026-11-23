"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useState } from "react";

import { Icon } from "@/components/icons";
import { apiSend } from "@/lib/api-client";
import { BOTTOM_NAV_SLOTS, type NavItem } from "@/lib/nav";

function isActive(pathname: string, href: string) {
  return href === "/" ? pathname === "/" : pathname === href || pathname.startsWith(`${href}/`);
}

/** The desktop sidebar's page list. */
export function SideNav({ items }: { items: NavItem[] }) {
  const pathname = usePathname();
  return (
    <nav aria-label="Main" className="flex flex-col gap-[3px]">
      {items.map((item) => (
        <Link key={item.href} href={item.href} className="nav-item" aria-current={isActive(pathname, item.href) ? "page" : undefined}>
          <Icon name={item.icon} />
          {item.label}
        </Link>
      ))}
    </nav>
  );
}

/**
 * The phone's bottom bar. With more pages than fit, the last slot becomes
 * "More", which opens a page listing the rest.
 */
export function BottomNav({ items }: { items: NavItem[] }) {
  const pathname = usePathname();
  const overflow = items.length > BOTTOM_NAV_SLOTS + 1;
  const shown = overflow ? items.slice(0, BOTTOM_NAV_SLOTS) : items;
  const hidden = overflow ? items.slice(BOTTOM_NAV_SLOTS) : [];
  const inMore = pathname === "/more" || hidden.some((i) => isActive(pathname, i.href));
  return (
    <nav
      aria-label="Main"
      className="bottom-nav float"
      style={{ gridTemplateColumns: `repeat(${shown.length + (overflow ? 1 : 0)}, minmax(0, 1fr))` }}
    >
      {shown.map((item) => (
        <Link key={item.href} href={item.href} aria-current={isActive(pathname, item.href) ? "page" : undefined}>
          <Icon name={item.icon} />
          {item.label}
        </Link>
      ))}
      {overflow ? (
        <Link href="/more" aria-current={inMore ? "page" : undefined}>
          <Icon name="more" />
          More
        </Link>
      ) : null}
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
      className="btn btn-ghost btn-icon"
      onClick={toggle}
      aria-label={theme === "daylight" ? "Switch to dark theme" : "Switch to light theme"}
      title={theme === "daylight" ? "Dark theme" : "Light theme"}
    >
      <Icon name={theme === "daylight" ? "moon" : "sun"} size={18} />
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
    <button type="button" className="btn btn-quiet justify-start" onClick={signOut} disabled={busy}>
      <Icon name="out" size={18} />
      {busy ? "Signing out…" : "Sign out"}
    </button>
  );
}
