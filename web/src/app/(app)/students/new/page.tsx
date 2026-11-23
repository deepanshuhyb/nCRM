import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { Page } from "@/components/ui";
import { getMe, getSummary, listBranches } from "@/lib/api-server";

import { NewStudentForm } from "./new-student-form";

export const metadata: Metadata = { title: "Add a student" };

export default async function NewStudentPage() {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_students")) return <NotAllowed what="students" />;
  const [branches, summary] = await Promise.all([listBranches(), getSummary()]);

  return (
    <Page narrow>
      <div>
        <Link href="/students" className="back-link">
          All students
        </Link>
        <h1 className="h-page">Add a student</h1>
        <p className="lead">
          Add who we should message about them. For adults, that&apos;s usually themselves.
        </p>
      </div>
      <NewStudentForm branches={branches.filter((b) => b.active).map((b) => ({ id: b.id, name: b.name }))} today={summary.today} />
    </Page>
  );
}
