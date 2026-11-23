import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, getSummary, listBranches, listRooms, listStaff } from "@/lib/api-server";

import { ClassForm } from "../class-form";

export const metadata: Metadata = { title: "Add a class" };

export default async function NewClassPage() {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_classes")) return <NotAllowed what="class setup" />;
  const [branches, rooms, team, summary] = await Promise.all([listBranches(), listRooms(), listStaff(), getSummary()]);

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-[18px]">
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Add a class</h1>
        <p className="soft mt-2 mb-0">A class that meets every week. Students join it from their profile or from the class page.</p>
      </div>
      {branches.length === 0 ? (
        <section className="glass p-7">
          <p className="m-0 font-semibold">Add a branch first, so the class has somewhere to meet.</p>
        </section>
      ) : (
        <ClassForm
          branches={branches.filter((b) => b.active).map((b) => ({ id: b.id, name: b.name }))}
          rooms={rooms.filter((r) => r.active)}
          team={team.filter((t) => t.active).map((t) => ({ id: t.id, name: t.name }))}
          today={summary.today}
        />
      )}
    </div>
  );
}
