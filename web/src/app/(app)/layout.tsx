import { cookies } from "next/headers";
import { redirect } from "next/navigation";

import { BottomNav, SignOutButton, StudioSwitcher, ThemeToggle, TopNav } from "@/components/shell-controls";
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

  return (
    <div className="has-bottom-nav">
      <header className="mx-auto flex w-full max-w-[1376px] flex-wrap items-center gap-3 px-[18px] pt-5 sm:px-8">
        <a href="#main" className="sr-only focus:not-sr-only">
          Skip to content
        </a>
        <div className="font-display mr-2 text-[24px] font-semibold">ncrm</div>
        <TopNav />
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
      <BottomNav />
    </div>
  );
}
