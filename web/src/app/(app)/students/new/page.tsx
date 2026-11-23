import type { Metadata } from "next";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, listBranches } from "@/lib/api-server";

import { NewStudentForm } from "./new-student-form";

export const metadata: Metadata = { title: "Add a student" };

export default async function NewStudentPage() {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_students")) return <NotAllowed what="students" />;
  const branches = await listBranches();

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-[18px]">
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Add a student</h1>
        <p className="soft mt-2 mb-0">
          Add who we should message about them. For adults, that&apos;s usually themselves.
        </p>
      </div>
      <NewStudentForm branches={branches.filter((b) => b.active).map((b) => ({ id: b.id, name: b.name }))} />
    </div>
  );
}
