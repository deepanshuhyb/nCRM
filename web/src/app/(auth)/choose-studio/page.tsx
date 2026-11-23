import type { Metadata } from "next";

import { getMe } from "@/lib/api-server";
import { safeNextPath } from "@/lib/safe-redirect";

import { StudioPicker } from "./studio-picker";

export const metadata: Metadata = { title: "Choose a studio" };

export default async function ChooseStudioPage({ searchParams }: PageProps<"/choose-studio">) {
  const me = await getMe();
  const params = await searchParams;
  const next = safeNextPath(typeof params.next === "string" ? params.next : null);

  return (
    <section className="glass p-6 sm:p-8" aria-labelledby="choose-title">
      <h1 id="choose-title" className="font-display m-0 text-[30px] leading-[1.08] font-semibold">
        Which studio?
      </h1>
      <p className="soft mt-3 mb-6">You work at more than one. You can switch any time from the top bar.</p>
      {me.memberships.length === 0 ? (
        <p className="attn-text font-semibold">
          This number isn&apos;t active at any studio anymore. Ask the studio owner to add you again.
        </p>
      ) : (
        <StudioPicker
          next={next}
          currentOrgId={me.studio?.org_id ?? null}
          studios={me.memberships.map((m) => ({ orgId: m.org_id, name: m.org_name, role: m.role }))}
        />
      )}
    </section>
  );
}
