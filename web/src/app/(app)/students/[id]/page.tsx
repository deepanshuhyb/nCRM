import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { getMe, getStudent, listBranches } from "@/lib/api-server";
import { ageFrom, formatDate, formatPhone } from "@/lib/format";
import { RELATION_LABEL, STATUS_LABEL } from "@/lib/types";

import { AddContactForm, EditStudentForm, RemoveContactButton } from "./student-editors";

export const metadata: Metadata = { title: "Student" };

export default async function StudentPage({ params }: PageProps<"/students/[id]">) {
  const me = await getMe();
  if (!me.studio!.capabilities.includes("manage_students")) return <NotAllowed what="students" />;
  const { id } = await params;
  const [student, branches] = await Promise.all([getStudent(id), listBranches()]);

  const age = student.date_of_birth ? ageFrom(student.date_of_birth) : null;
  const branch = branches.find((b) => b.id === student.branch_id);
  const statusClass = student.status === "active" ? "tag tag-good" : student.status === "paused" ? "tag tag-quiet" : "tag tag-attn";

  return (
    <div className="flex flex-col gap-[18px]">
      <Link href="/students" className="btn btn-quiet self-start px-0">
        All students
      </Link>

      <div className="flex flex-wrap items-stretch gap-[18px]">
        <section aria-labelledby="student-name" className="glass min-w-0 flex-[3_1_420px] p-6 sm:p-8">
          <div className="flex flex-wrap items-center gap-3">
            <h1 id="student-name" className="font-display m-0 text-[32px] leading-tight font-semibold sm:text-[40px]">
              {student.name}
            </h1>
            <span className={statusClass}>{STATUS_LABEL[student.status]}</span>
          </div>
          <dl className="mt-6 grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4">
            <div>
              <dt className="muted text-[13px]">Age</dt>
              <dd className="font-display m-0 text-[22px] font-semibold">{age ?? "Not set"}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Branch</dt>
              <dd className="m-0 text-base font-semibold">{branch?.name ?? "Not set"}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Joined</dt>
              <dd className="m-0 text-base font-semibold">{formatDate(student.joined_on)}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Contacts</dt>
              <dd className="font-display m-0 text-[22px] font-semibold">{student.contacts.length}</dd>
            </div>
          </dl>
          {student.notes ? (
            <div className="row mt-6 p-4">
              <p className="muted m-0 mb-1 text-[13px]">Notes</p>
              <p className="m-0 whitespace-pre-line">{student.notes}</p>
            </div>
          ) : null}
        </section>

        <section aria-labelledby="contacts-title" className="glass flex min-w-0 flex-[2_1_340px] flex-col gap-3 p-6">
          <h2 id="contacts-title" className="font-display m-0 text-[20px] font-semibold">
            Who we message
          </h2>
          {student.contacts.length === 0 ? (
            <p className="attn-text m-0 font-semibold">No contact yet. Reminders can&apos;t reach anyone.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-2 p-0">
              {student.contacts.map((c) => (
                <li key={c.contact_id} className="row flex flex-wrap items-center justify-between gap-2 p-3.5">
                  <span className="min-w-0">
                    <span className="block font-bold">
                      {c.name} {c.is_primary ? <span className="tag tag-good ml-1">Main</span> : null}
                    </span>
                    <span className="muted text-[13px]">
                      {RELATION_LABEL[c.relation]}, {formatPhone(c.phone)}
                      {c.student_count > 1 ? `, also for ${c.student_count - 1} other student${c.student_count > 2 ? "s" : ""}` : ""}
                    </span>
                  </span>
                  <RemoveContactButton studentId={student.id} contactId={c.contact_id} name={c.name} />
                </li>
              ))}
            </ul>
          )}
          {student.contacts.length < 4 ? <AddContactForm studentId={student.id} /> : null}
        </section>
      </div>

      <EditStudentForm
        student={{
          id: student.id,
          name: student.name,
          date_of_birth: student.date_of_birth,
          branch_id: student.branch_id,
          status: student.status,
          notes: student.notes,
        }}
        branches={branches.map((b) => ({ id: b.id, name: b.name }))}
      />
    </div>
  );
}
