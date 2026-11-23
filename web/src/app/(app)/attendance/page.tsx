import type { Metadata } from "next";
import Link from "next/link";

import { Meter } from "@/components/graphs";
import { SessionRow, sessionState } from "@/components/session-row";
import { EmptyState, Page, PageHeader, SectionHeader } from "@/components/ui";
import { getAttendanceAlerts, getMe, getSummary, listBranches, listSessions } from "@/lib/api-server";
import { addDays, formatDay, minutesOf } from "@/lib/format";

export const metadata: Metadata = { title: "Today" };

/**
 * One day's classes. The inverted block is the day as a strip (each class
 * a block as wide as it is long, filled once it's marked), so "what's left
 * to do today" is answered before reading a word.
 */
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

  const running = sessions.filter((s) => s.status === "scheduled");
  const done = running.filter((s) => sessionState(s).state === "done").length;
  const due = running.filter((s) => sessionState(s).state === "due").length;
  const came = running.reduce((n, s) => n + s.came, 0);
  const marked = came + running.reduce((n, s) => n + s.away, 0);

  return (
    <Page>
      <PageHeader
        title={isToday ? "Today" : formatDay(date)}
        lead={
          sessions.length === 0
            ? `${isToday ? `${formatDay(date)}. ` : ""}No classes on this day.`
            : `${isToday ? `${formatDay(date)}. ` : ""}${sessions.length} ${sessions.length === 1 ? "class" : "classes"}. Tap one to mark who came.`
        }
        actions={
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
        }
      />

      {running.length > 0 ? (
        <section aria-labelledby="day-title" className="hero flex flex-col gap-5 p-6 sm:p-7">
          <div className="flex flex-wrap items-end justify-between gap-x-8 gap-y-3">
            <div>
              <h2 id="day-title" className="stat-label m-0 text-sm">
                {isToday ? "The day so far" : "How the day went"}
              </h2>
              <p className="num m-0 mt-2 text-[44px] sm:text-[56px]">
                {done} of {running.length}
              </p>
              <p className="m-0 mt-2 font-semibold" style={{ color: "var(--hero-soft)" }}>
                {running.length === 1 ? "class" : "classes"} marked
                {due > 0 ? `. ${due} ${due === 1 ? "is" : "are"} over and waiting.` : marked > 0 ? `. ${came} of ${marked} students came.` : "."}
              </p>
            </div>
            <div className="min-w-[180px] flex-1 sm:max-w-[320px]">
              <Meter value={done / running.length} />
            </div>
          </div>
          <nav aria-label="Classes through the day" className="day-strip">
            {sessions.map((s) => {
              const st = sessionState(s);
              return (
                <Link
                  key={s.id}
                  href={`/attendance/${s.id}`}
                  data-state={st.state}
                  style={{ ["--w" as string]: Math.max(30, minutesOf(s.ends_at) - minutesOf(s.starts_at)) }}
                  aria-label={`${s.batch_name}, ${s.starts_at}, ${st.label}`}
                  title={`${s.batch_name}: ${st.label}`}
                >
                  <span className="tabular-nums">{s.starts_at}</span>
                  <span className="hidden truncate sm:block">{s.batch_name}</span>
                </Link>
              );
            })}
          </nav>
        </section>
      ) : null}

      {branches.length > 1 ? (
        <nav aria-label="Branch" className="segmented self-start">
          <Link href={href({ branch: null })} aria-current={!branch ? "page" : undefined}>
            All<span className="max-sm:hidden">&nbsp;branches</span>
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

      <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-5">
        <section aria-labelledby="classes-title" className="panel flex flex-col gap-3 p-4 sm:p-6 lg:col-span-3">
          <SectionHeader id="classes-title" title="Classes" aside={sessions.length > 0 ? `${sessions.length}` : undefined} />
          {sessions.length === 0 ? (
            <EmptyState
              title="Nothing on the timetable"
              action={
                <Link href="/classes" className="btn btn-ghost">
                  See the week
                </Link>
              }
            >
              No class meets on this day{branch ? ` at ${branch.name}` : ""}.
            </EmptyState>
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

        <aside className="flex min-w-0 flex-col gap-[18px] lg:col-span-2">
          <section aria-labelledby="unmarked-title" className={`panel ${otherDays.length > 0 ? "panel-attn" : ""} flex flex-col gap-3 p-4 sm:p-6`}>
            <SectionHeader id="unmarked-title" title="Still to mark" aside={otherDays.length > 0 ? `${otherDays.length} earlier` : undefined} />
            {otherDays.length > 0 ? (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {otherDays.slice(0, 8).map((s) => (
                  <li key={s.id}>
                    <SessionRow s={s} mine={s.instructor_id === studio.staff_id} showDate />
                  </li>
                ))}
              </ul>
            ) : (
              <p className="soft m-0">Classes from the last week are all marked.</p>
            )}
          </section>

          <section aria-labelledby="streaks-title" className="panel flex flex-col gap-3 p-4 sm:p-6">
            <SectionHeader id="streaks-title" title="Missing class after class" aside={alerts.streaks.length > 0 ? `${alerts.streaks.length}` : undefined} />
            {alerts.streaks.length === 0 ? (
              <p className="soft m-0">Nobody has missed three classes in a row.</p>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {alerts.streaks.slice(0, 10).map((s) => (
                  <li key={`${s.student_id}-${s.batch_id}`} className="row flex items-center justify-between gap-3 p-3">
                    <span className="min-w-0">
                      {studio.capabilities.includes("manage_students") ? (
                        <Link href={`/students/${s.student_id}`} className="stretch block font-bold">
                          {s.student_name}
                        </Link>
                      ) : (
                        <span className="block font-bold">{s.student_name}</span>
                      )}
                      <span className="caption">{s.batch_name}</span>
                    </span>
                    <span className="tag tag-attn shrink-0">Missed {s.absences} in a row</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>
      </div>
    </Page>
  );
}
