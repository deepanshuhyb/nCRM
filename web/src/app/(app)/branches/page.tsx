import type { Metadata } from "next";

import { getMe, listBranches, listRooms } from "@/lib/api-server";

import { NewBranchForm } from "./new-branch-form";
import { RoomsEditor } from "./rooms-editor";

export const metadata: Metadata = { title: "Branches" };

export default async function BranchesPage() {
  const [me, branches, rooms] = await Promise.all([getMe(), listBranches(), listRooms()]);
  const canManage = me.studio!.capabilities.includes("manage_branches");

  return (
    <div className="flex flex-col gap-[18px]">
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Branches</h1>
        <p className="soft mt-2 mb-0">
          {branches.length === 0
            ? "No branches yet. Add the places you teach."
            : `${branches.length} ${branches.length === 1 ? "branch" : "branches"} in ${me.studio!.org_name}.`}
        </p>
      </div>

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-label="All branches" className="flex min-w-0 flex-[2_1_420px] flex-col gap-3">
          {branches.length === 0 ? (
            <div className="glass p-7">
              <p className="m-0 text-base font-semibold">Nothing here yet.</p>
              <p className="muted mt-1 mb-0">
                {canManage ? "Use the form to add your first branch." : "An owner or manager can add branches."}
              </p>
            </div>
          ) : (
            branches.map((b) => (
              <article key={b.id} className="glass flex flex-col gap-4 p-5 sm:p-6">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="font-display m-0 text-[22px] font-semibold">{b.name}</h2>
                    <p className="muted mt-1 mb-0">{b.address ?? "No address added"}</p>
                  </div>
                  <span className={b.active ? "tag tag-good" : "tag tag-quiet"}>{b.active ? "Open" : "Closed"}</span>
                </div>
                <RoomsEditor
                  branchId={b.id}
                  branchName={b.name}
                  rooms={rooms.filter((r) => r.branch_id === b.id)}
                  canManage={canManage && b.active}
                />
              </article>
            ))
          )}
        </section>

        <aside className="min-w-0 flex-[1_1_320px]">
          {canManage ? (
            <NewBranchForm />
          ) : (
            <div className="glass p-6">
              <p className="m-0 font-semibold">Only owners and managers can add branches.</p>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
