import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, getSummary, listBranches } from "@/lib/api-server";

import { ImportFlow } from "./import-flow";

export const metadata: Metadata = { title: "Import students" };

const COLUMNS: { name: string; note: string }[] = [
  { name: "Name", note: "required" },
  { name: "Phone", note: "WhatsApp number" },
  { name: "Contact name", note: "whose number" },
  { name: "Relation", note: "Mother, Father…" },
  { name: "Mother's phone", note: "and name" },
  { name: "Father's phone", note: "and name" },
  { name: "Date of birth", note: "DD/MM/YYYY" },
  { name: "Joined on", note: "can be in the past" },
  { name: "Branch", note: "by name" },
  { name: "Notes", note: "" },
];

export default async function ImportStudentsPage() {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_students")) return <NotAllowed what="student import" />;
  const [branches, summary] = await Promise.all([listBranches(), getSummary()]);
  const open = branches.filter((b) => b.active).map((b) => ({ id: b.id, name: b.name }));

  return (
    <div className="flex flex-col gap-[18px]">
      <Link href="/students" className="btn btn-quiet self-start px-0">
        All students
      </Link>
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Bring in your student list</h1>
        <p className="soft mt-2 mb-0 max-w-[68ch]">
          From Excel or a CSV file. You&apos;ll see what happens to every row before anything is saved, and students who are
          already here are skipped, so importing the same file twice is safe.
        </p>
      </div>

      <div className="flex flex-wrap items-start gap-[18px]">
        <div className="min-w-0 flex-[2_1_520px]">
          <ImportFlow branches={open} today={summary.today} />
        </div>
        <section aria-labelledby="columns-title" className="glass flex min-w-0 flex-[1_1_300px] flex-col gap-3 p-6">
          <h2 id="columns-title" className="font-display m-0 text-[20px] font-semibold">
            Your sheet or ours
          </h2>
          <p className="soft m-0 text-sm">
            Column titles go in the first row. We recognise common names too, like Student name, Mobile or Date of joining.
          </p>
          <ul className="m-0 flex list-none flex-wrap gap-1.5 p-0" aria-label="Columns we read">
            {COLUMNS.map((c) => (
              <li key={c.name} className="chip">
                {c.name}
                {c.note ? <span className="muted ml-1.5 font-normal">{c.note}</span> : null}
              </li>
            ))}
          </ul>
          <p className="soft m-0 text-sm">
            Joining dates in the past are kept and marked <span className="tag tag-quiet">Backdated</span>.
          </p>
          <div className="flex flex-wrap gap-2 pt-1">
            <a href="/templates/ncrm-students.xlsx" download className="btn btn-ghost">
              Excel template
            </a>
            <a href="/templates/ncrm-students.csv" download className="btn btn-quiet">
              CSV template
            </a>
          </div>
        </section>

      </div>
    </div>
  );
}
