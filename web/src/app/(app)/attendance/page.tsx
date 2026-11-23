import type { Metadata } from "next";
import Link from "next/link";

import { getAttendanceAlerts, getMe, getSummary, listBranches, listSessions } from "@/lib/api-server";
import { SessionRow } from "@/components/session-row";
import { addDays, formatDay } from "@/lib/format";

export const metadata: Metadata = { title: "Today" };

export default async function AttendancePage({ searchParams }: PageProps<"/attendance">) {
  const params = await searchParams;
  const [me, summary, branches] = await Promise.all([getMe(), getSummary(), listBranches()]);
  const studio = me.studio!;
  const today = summary.today;
  const asked = typeof params.date === "string" && /^\d{4}-\d{2}-\d{2}$/.test(params.date) ? params.date : today;
  const date = asked > today || asked < addDays(today, -90) ? today : asked;
  const branch = branches.find((b) => b.id === params.branch);
  const [sessions, alerts] = await Promise.all([listSessions({ date, branchId: branch?.id }), getAttendanceAlerts()]);
  const isToday = date === today;
  const otherDays = alerts.unmarked.filter((s) => s.on_date !== date);

  const href = (next: { date?: string; branch?: string | null }) => {
    const sp = new URLSearchParams();
    const d = next.date ?? date;
    if (d !== today) sp.set("date", d);
    const b = next.branch === undefined ? branch?.id : next.branch;
    if (b) sp.set("branch", b);
    const qs = sp.toString();
    return `/attendance${qs ? `?${qs}` : ""}`;
  };

  return (
    <div className="flex flex-col gap-[18px]">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">{isToday ? "Today" : formatDay(date)}</h1>
          <p className="soft mt-2 mb-0">
            {isToday ? `${formatDay(date)}. ` : ""}
            {sessions.length === 0
              ? "No classes on this day."
              : `${sessions.length} ${sessions.length === 1 ? "class" : "classes"}. Tap one to mark who came.`}
          </p>
        </div>
        <nav aria-label="Day" className="flex flex-wrap gap-2">
          {date > addDays(today, -90) ? (
            <Link href={href({ date: addDays(date, -1) })} className="btn btn-ghost">
              Day before
            </Link>
          ) : null}
          {!isToday ? (
            <>
              <Link href={href({ date: addDays(date, 1) })} className="btn btn-ghost">
                Day after
              </Link>
              <Link href={href({ date: today })} className="btn btn-dark">
                Today
              </Link>
            </>
          ) : null}
        </nav>
      </div>

      {branches.length > 1 ? (
        <nav aria-label="Branch" className="segmented glass self-start">
          <Link href={href({ branch: null })} aria-current={!branch ? "page" : undefined}>
            All branches
          </Link>
          {branches
            .filter((b) => b.active)
            .map((b) => (
              <Link key={b.id} href={href({ branch: b.id })} aria-current={branch?.id === b.id ? "page" : undefined}>
                {b.name}
              </Link>
            ))}
        </nav>
      ) : null}

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-label="Classes" className="glass min-w-0 flex-[3_1_420px] p-2">
          {sessions.length === 0 ? (
            <p className="m-0 p-5 font-semibold">
              Nothing on the timetable for this day. <Link href="/classes">See the week</Link>.
            </p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {sessions.map((s) => (
                <li key={s.id}>
                  <SessionRow s={s} mine={s.instructor_id === studio.staff_id} />
                </li>
              ))}
            </ul>
          )}
        </section>

        <aside className="flex min-w-0 flex-[2_1_320px] flex-col gap-[18px]">
          <section aria-labelledby="unmarked-title" className={`glass ${otherDays.length > 0 ? "glass-attn" : ""} flex flex-col gap-2 p-5`}>
            <h2 id="unmarked-title" className="font-display m-0 text-[20px] font-semibold">
              {otherDays.length > 0 ? `${otherDays.length} earlier ${otherDays.length === 1 ? "class needs" : "classes need"} marking` : "Nothing waiting from earlier days"}
            </h2>
            {otherDays.length > 0 ? (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {otherDays.slice(0, 8).map((s) => (
                  <li key={s.id}>
                    <SessionRow s={s} mine={s.instructor_id === studio.staff_id} showDate />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="soft m-0 text-sm">Classes from the last week are all marked.</p>
            )}
          </section>

          <section aria-labelledby="streaks-title" className="glass flex flex-col gap-2 p-5">
            <h2 id="streaks-title" className="font-display m-0 text-[20px] font-semibold">
              Missing class after class
            </h2>
            {alerts.streaks.length === 0 ? (
              <p className="soft m-0 text-sm">Nobody has missed three classes in a row.</p>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {alerts.streaks.slice(0, 10).map((s) => (
                  <li key={`${s.student_id}-${s.batch_id}`} className="row flex items-center justify-between gap-3 p-3">
                    <span className="min-w-0">
                      {studio.capabilities.includes("manage_students") ? (
                        <Link href={`/students/${s.student_id}`} className="block font-bold">
                          {s.student_name}
                        </Link>
                      ) : (
                        <span className="block font-bold">{s.student_name}</span>
                      )}
                      <span className="muted text-[13px]">{s.batch_name}</span>
                    </span>
                    <span className="tag tag-attn shrink-0">Missed {s.absences} in a row</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>
      </div>
    </div>
  );
}
