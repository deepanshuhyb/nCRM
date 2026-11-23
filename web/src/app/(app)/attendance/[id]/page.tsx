import type { Metadata } from "next";
import Link from "next/link";

import { Travels } from "@/components/travel";
import { Page, PageHeader } from "@/components/ui";
import { getMe, getSession, getSummary, listStaff } from "@/lib/api-server";
import { formatDay } from "@/lib/format";

import { CancelClass, MarkSheet, TeacherCheckIn } from "./mark-sheet";

export const metadata: Metadata = { title: "Attendance" };

export default async function SessionPage({ params }: PageProps<"/attendance/[id]">) {
  const { id } = await params;
  const [me, session, summary] = await Promise.all([getMe(), getSession(id), getSummary()]);
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  const markAny = can.has("mark_any_attendance");
  const team = markAny && can.has("manage_staff") ? await listStaff() : [];
  const cancelled = session.status === "cancelled";
  const backHref = session.on_date === summary.today ? "/attendance" : `/attendance?date=${session.on_date}`;

  return (
    <Page narrow>
      <PageHeader
        back={{ href: backHref, label: session.on_date === summary.today ? "Today" : formatDay(session.on_date) }}
        title={
          <Travels kind="session" id={session.id}>
            <h1 className="h-page">{session.batch_name}</h1>
          </Travels>
        }
        lead={
          <>
            {formatDay(session.on_date)}, {session.starts_at}–{session.ends_at} · {[session.branch_name, session.room_name].filter(Boolean).join(" · ")} ·{" "}
            <Link href={`/classes/${session.batch_id}`}>Class page</Link>
          </>
        }
      >
        {cancelled ? <span className="tag tag-attn">Cancelled</span> : null}
      </PageHeader>

      {cancelled && session.cancel_reason ? <p className="attn-text m-0 font-semibold">Reason: {session.cancel_reason}</p> : null}

      {!cancelled && session.can_mark ? (
        <section aria-label="Teacher" className="panel p-3 sm:p-4">
          <TeacherCheckIn
            sessionId={session.id}
            instructorId={session.instructor_id}
            instructorName={session.instructor_id === studio.staff_id ? "You" : session.instructor_name}
            present={session.teacher_present}
            canMarkAny={markAny}
            team={team.filter((t) => t.active).map((t) => ({ id: t.id, name: t.name }))}
          />
        </section>
      ) : null}

      {session.roster === null ? (
        <section className="panel p-6">
          <p className="m-0 font-semibold">
            {session.expected} {session.expected === 1 ? "student is" : "students are"} expected, {session.came} came. The
            class&apos;s teacher and the front desk mark attendance.
          </p>
        </section>
      ) : cancelled ? (
        <section className="panel p-6">
          <p className="m-0 font-semibold">This class didn&apos;t happen, so there&apos;s nothing to mark.</p>
        </section>
      ) : (
        <MarkSheet sessionId={session.id} roster={session.roster} canMark={session.can_mark} canFindStudents={can.has("manage_students")} />
      )}

      {markAny ? <CancelClass sessionId={session.id} cancelled={cancelled} name={session.batch_name} /> : null}
    </Page>
  );
}
