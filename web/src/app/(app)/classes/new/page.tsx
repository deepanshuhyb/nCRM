import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { Page } from "@/components/ui";
import { getMe, getSummary, listBranches, listRooms, listStaff } from "@/lib/api-server";

import { ClassForm } from "../class-form";

export const metadata: Metadata = { title: "Add a class" };

export default async function NewClassPage() {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_classes")) return <NotAllowed what="class setup" />;
  const [branches, rooms, team, summary] = await Promise.all([listBranches(), listRooms(), listStaff(), getSummary()]);

  return (
    <Page narrow>
      <div>
        <h1 className="h-page">Add a class</h1>
        <p className="lead">A class that meets every week. Students join it from their profile or from the class page.</p>
      </div>
      {branches.length === 0 ? (
        <section className="panel p-7">
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
    </Page>
  );
}
