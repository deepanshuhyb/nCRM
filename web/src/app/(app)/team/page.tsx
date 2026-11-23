import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, listBranches, listStaff } from "@/lib/api-server";
import { formatPhone } from "@/lib/format";
import { ROLE_LABEL } from "@/lib/types";

import { EditMember, HireForm } from "./team-editors";

export const metadata: Metadata = { title: "Team" };

export default async function TeamPage() {
  const me = await getMe();
  const studio = me.studio!;
  if (!studio.capabilities.includes("manage_staff")) return <NotAllowed what="the team" />;
  const [team, branches] = await Promise.all([listStaff(), listBranches()]);
  const branchName = new Map(branches.map((b) => [b.id, b.name]));
  const isOwner = studio.role === "owner";
  const active = team.filter((t) => t.active);
  const inactive = team.filter((t) => !t.active);
  const branchOptions = branches.map((b) => ({ id: b.id, name: b.name }));

  return (
    <div className="flex flex-col gap-[18px]">
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Team</h1>
        <p className="soft mt-2 mb-0">
          {active.length} active. Everyone here signs in with their own number. Deactivating someone signs them out at once.
        </p>
      </div>

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-label="Team members" className="flex min-w-0 flex-[2_1_440px] flex-col gap-3">
          {[...active, ...inactive].map((t) => {
            const isMe = t.id === studio.staff_id;
            const locked = isMe || (t.role === "owner" && !isOwner);
            return (
              <article key={t.id} className="glass flex flex-col gap-3 p-5">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="m-0 text-[17px] font-bold">
                      {t.name} {isMe ? <span className="muted font-normal">(you)</span> : null}
                    </h2>
                    <p className="muted m-0 text-[13px]">
                      {formatPhone(t.phone)}, {t.branch_id ? branchName.get(t.branch_id) ?? "Unknown branch" : "All branches"}
                    </p>
                  </div>
                  <span className="flex flex-wrap gap-2">
                    <span className="tag tag-quiet">{ROLE_LABEL[t.role]}</span>
                    {t.active ? null : <span className="tag tag-attn">Deactivated</span>}
                  </span>
                </div>
                {locked ? null : (
                  <EditMember
                    member={{ id: t.id, name: t.name, phone: t.phone, role: t.role, branch_id: t.branch_id, active: t.active }}
                    branches={branchOptions}
                    canGrantOwner={isOwner}
                  />
                )}
              </article>
            );
          })}
        </section>
        <aside className="min-w-0 flex-[1_1_320px]">
          <HireForm branches={branchOptions} canGrantOwner={isOwner} />
        </aside>
      </div>
    </div>
  );
}
