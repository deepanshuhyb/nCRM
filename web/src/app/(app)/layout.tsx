import { cookies } from "next/headers";
import Link from "next/link";
import { redirect } from "next/navigation";

import { BottomNav, SideNav, SignOutButton, StudioSwitcher, ThemeToggle } from "@/components/shell-controls";
import { getMe } from "@/lib/api-server";
import { navFor } from "@/lib/nav";

/**
 * The signed-in app. The API confirms the session on every render; anyone
 * whose session is invalid is sent to sign in, anyone without a studio to
 * the picker. Client components only receive the few fields they show.
 *
 * Desktop has a sidebar; phones have a slim top bar and a bottom bar. Both
 * are built from the same list (lib/nav.ts) and only one is displayed.
 */
export default async function AppLayout({ children }: LayoutProps<"/">) {
  const me = await getMe();
  if (!me.studio) redirect("/choose-studio");
  const theme = (await cookies()).get("ncrm_theme")?.value === "emerald" ? "emerald" : "daylight";
  const nav = navFor(me.studio.capabilities);
  const studios = me.memberships.map((m) => ({ orgId: m.org_id, name: m.org_name }));

  return (
    <div className="app">
      <a href="#main" className="sr-only focus:not-sr-only">
        Skip to content
      </a>
      <aside className="sidebar" aria-label="Studio">
        <Link href="/" className="brand px-3 pb-1">
          ncrm
        </Link>
        <p className="caption m-0 mb-4 truncate px-3 font-semibold">{me.studio.org_name}</p>
        <SideNav items={nav} />
        <div className="flex-1" />
        <StudioSwitcher current={me.studio.org_id} studios={studios} />
        <div className="flex items-center justify-between gap-2">
          <SignOutButton />
          <ThemeToggle initial={theme} />
        </div>
      </aside>
      <div className="min-w-0">
        <header className="topbar">
          <Link href="/" className="brand">
            ncrm
          </Link>
          <div className="flex-1" />
          <StudioSwitcher current={me.studio.org_id} studios={studios} />
          <ThemeToggle initial={theme} />
          <SignOutButton />
        </header>
        <main id="main" className="main">
          {children}
        </main>
      </div>
      <BottomNav items={nav} />
    </div>
  );
}
