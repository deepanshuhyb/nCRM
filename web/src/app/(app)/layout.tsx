import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import {
  BottomNav,
  SignOutButton,
  StudioSwitcher,
  ThemeToggle,
  TopNav,
  type NavItem,
} from "@/components/shell-controls";
import { getMe } from "@/lib/api-server";

/**
 * The signed-in app. The API confirms the session on every render; anyone
 * whose session is invalid is sent to sign in, anyone without a studio to
 * the picker. Client components only receive the few fields they show.
 */
export default async function AppLayout({ children }: LayoutProps<"/">) {
  const me = await getMe();
  if (!me.studio) redirect("/choose-studio");
  const theme = (await cookies()).get("ncrm_theme")?.value === "emerald" ? "emerald" : "daylight";
  const can = new Set(me.studio.capabilities);
  // Only show what the role can open. The API refuses the rest anyway.
  const nav: NavItem[] = [
    { href: "/", label: "Home" },
    ...(can.has("manage_students") ? [{ href: "/students", label: "Students" }] : []),
    ...(can.has("manage_staff") ? [{ href: "/team", label: "Team" }] : []),
    { href: "/branches", label: "Branches" },
  ];

  return (
    <div className="has-bottom-nav">
      <header className="mx-auto flex w-full max-w-[1376px] flex-wrap items-center gap-3 px-[18px] pt-5 sm:px-8">
        <a href="#main" className="sr-only focus:not-sr-only">
          Skip to content
        </a>
        <div className="font-display mr-2 text-[24px] font-semibold">ncrm</div>
        <TopNav items={nav} />
        <div className="flex-1" />
        <StudioSwitcher
          current={me.studio.org_id}
          studios={me.memberships.map((m) => ({ orgId: m.org_id, name: m.org_name }))}
        />
        <ThemeToggle initial={theme} />
        <SignOutButton />
      </header>
      <main id="main" className="mx-auto w-full max-w-[1376px] px-[18px] pt-5 pb-16 sm:px-8">
        {children}
      </main>
      <BottomNav items={nav} />
    </div>
  );
}
