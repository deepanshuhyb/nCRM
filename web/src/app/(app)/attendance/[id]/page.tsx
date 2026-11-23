import type { Metadata } from "next";
import Link from "next/link";

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
    <div className="mx-auto flex max-w-[860px] flex-col gap-[18px]">
      <Link href={backHref} className="btn btn-quiet self-start px-0">
        {session.on_date === summary.today ? "Today" : formatDay(session.on_date)}
      </Link>

      <section aria-labelledby="session-title" className="glass flex flex-col gap-4 p-6">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <h1 id="session-title" className="font-display m-0 text-[28px] leading-tight font-semibold sm:text-[34px]">
              {session.batch_name}
            </h1>
            {cancelled ? <span className="tag tag-attn">Cancelled</span> : null}
          </div>
          <p className="soft m-0 mt-2">
            {formatDay(session.on_date)}, {session.starts_at}–{session.ends_at} ·{" "}
            {[session.branch_name, session.room_name].filter(Boolean).join(" · ")} ·{" "}
            <Link href={`/classes/${session.batch_id}`}>Class page</Link>
          </p>
          {cancelled && session.cancel_reason ? <p className="attn-text m-0 mt-2 font-semibold">Reason: {session.cancel_reason}</p> : null}
        </div>
        {!cancelled && session.can_mark ? (
          <TeacherCheckIn
            sessionId={session.id}
            instructorId={session.instructor_id}
            instructorName={session.instructor_id === studio.staff_id ? "You" : session.instructor_name}
            present={session.teacher_present}
            canMarkAny={markAny}
            team={team.filter((t) => t.active).map((t) => ({ id: t.id, name: t.name }))}
          />
        ) : null}
      </section>

      {session.roster === null ? (
        <section className="glass p-6">
          <p className="m-0 font-semibold">
            {session.expected} {session.expected === 1 ? "student is" : "students are"} expected, {session.came} came. The
            class&apos;s teacher and the front desk mark attendance.
          </p>
        </section>
      ) : cancelled ? (
        <section className="glass p-6">
          <p className="m-0 font-semibold">This class didn&apos;t happen, so there&apos;s nothing to mark.</p>
        </section>
      ) : (
        <MarkSheet sessionId={session.id} roster={session.roster} canMark={session.can_mark} canFindStudents={can.has("manage_students")} />
      )}

      {markAny ? <CancelClass sessionId={session.id} cancelled={cancelled} name={session.batch_name} /> : null}
    </div>
  );
}
