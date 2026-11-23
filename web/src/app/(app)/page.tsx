import type { Metadata } from "next";
import Link from "next/link";

import { Bars, Flow, Meter, Ranked, type Bar } from "@/components/graphs";
import { Icon } from "@/components/icons";
import { Counts, EmptyState, Page, PageHeader, SectionHeader, Stat } from "@/components/ui";
import {
  getAttendanceAlerts,
  getAttendanceReport,
  getFeesReport,
  getMe,
  getStudentsReport,
  getSummary,
  listBatches,
} from "@/lib/api-server";
import { formatDay, formatDayShort, formatPercent, formatRupees, formatRupeesShort, monthLong, monthShort } from "@/lib/format";
import { ROLE_LABEL, type DayAttendance } from "@/lib/types";

export const metadata: Metadata = { title: "Home" };

const AGE_LABEL = { not_due: "Not due yet", week: "Up to a week late", month: "Up to a month late", older: "More than a month late" } as const;

/** Share who came out of everyone marked, or null when nothing was marked. */
function rate(days: DayAttendance[]): number | null {
  const came = days.reduce((n, d) => n + d.came, 0);
  const marked = came + days.reduce((n, d) => n + d.away, 0);
  return marked === 0 ? null : came / marked;
}

/**
 * Home answers "how is the studio doing, and what needs me today?".
 * One inverted block (attendance over four weeks), a short list of things
 * to act on, four figures, then three small graphs. Money only appears for
 * roles that handle fees: the API sends null to everyone else.
 */
export default async function HomePage() {
  const me = await getMe();
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  const [summary, days, months, batches, alerts, fees] = await Promise.all([
    getSummary(),
    getAttendanceReport(28),
    getStudentsReport(),
    listBatches(),
    getAttendanceAlerts(),
    can.has("manage_fees") ? getFeesReport() : null,
  ]);
  const today = summary.today;

  // ------------------------------------------------------- attendance
  const thisWeek = rate(days.slice(-7));
  const lastWeek = rate(days.slice(-14, -7));
  const points = thisWeek !== null && lastWeek !== null ? Math.round((thisWeek - lastWeek) * 100) : null;
  const finding =
    thisWeek === null
      ? rate(days) === null
        ? "Nothing has been marked yet. Mark a class and this fills in."
        : "Nothing marked in the last seven days."
      : points === null
        ? "of the students marked this week came to class."
        : Math.abs(points) < 2
          ? "came this week, about the same as last week."
          : points > 0
            ? `came this week, up ${points} points on last week.`
            : `came this week, down ${-points} points on last week.`;
  const bars: Bar[] = days.map((d, i) => {
    const marked = d.came + d.away;
    return {
      key: formatDay(d.date),
      value: marked === 0 ? null : d.came / marked,
      tone: i < days.length - 7 ? "soft" : undefined,
      gap: i > 0 && i % 7 === 0,
      tip: marked === 0 ? `${formatDayShort(d.date)}: nothing marked` : `${formatDayShort(d.date)}: ${d.came} of ${marked} came (${formatPercent(d.came / marked)})`,
    };
  });

  // ------------------------------------------------- needs you today
  const overdue = fees ? fees.ageing.filter((b) => b.key !== "not_due") : [];
  const overduePaise = overdue.reduce((n, b) => n + b.due_paise, 0);
  const needs = [
    ...alerts.unmarked.slice(0, 3).map((s) => ({
      key: `s-${s.id}`,
      href: `/attendance/${s.id}`,
      title: s.batch_name,
      detail: `${s.on_date === today ? "Today" : formatDayShort(s.on_date)}, ${s.starts_at} · ${s.unmarked} not marked`,
      action: "Mark",
    })),
    ...alerts.streaks.slice(0, 2).map((s) => ({
      key: `a-${s.student_id}-${s.batch_id}`,
      href: can.has("manage_students") ? `/students/${s.student_id}` : `/classes/${s.batch_id}`,
      title: s.student_name,
      detail: `Missed ${s.absences} in a row · ${s.batch_name}`,
      action: "Open",
    })),
    ...(overduePaise > 0
      ? [
          {
            key: "fees",
            href: "/fees",
            title: `${formatRupees(overduePaise)} overdue`,
            detail: "Fees past their due date",
            action: "Collect",
          },
        ]
      : []),
  ];

  // ------------------------------------------------------------ classes
  const sized = batches.filter((b) => b.capacity !== null && b.capacity > 0);
  const full = sized.filter((b) => b.enrolled >= b.capacity!).length;
  const biggest = Math.max(1, ...batches.map((b) => b.enrolled));
  const classRows = (sized.length > 0 ? sized : batches)
    .map((b) => ({ b, share: b.capacity ? b.enrolled / b.capacity : b.enrolled / biggest }))
    .sort((x, y) => y.share - x.share || y.b.enrolled - x.b.enrolled)
    .slice(0, 6)
    .map(({ b, share }) => {
      const isFull = b.capacity !== null && b.enrolled >= b.capacity;
      return {
        key: b.id,
        name: b.name,
        href: `/classes/${b.id}`,
        share,
        tone: isFull ? ("attn" as const) : undefined,
        value: (
          <>
            {b.capacity !== null ? `${b.enrolled} of ${b.capacity}` : b.enrolled}
            {isFull ? <span className="tag tag-attn ml-2">Full</span> : null}
          </>
        ),
      };
    });

  // ----------------------------------------------------------- students
  const month = months[months.length - 1];
  const collection = fees?.collection ?? null;
  const dayMax = collection ? Math.max(1, ...collection.days.map((d) => d.collected_paise)) : 1;

  // -------------------------------------------------------------- setup
  const steps = [
    { title: "Add your branches", done: summary.branches > 0, href: can.has("manage_branches") ? "/branches" : null, action: "Add branches" },
    { title: "Add teachers and front desk", done: summary.active_team > 1, href: can.has("manage_staff") ? "/team" : null, action: "Add people" },
    { title: "Set up classes", done: summary.classes > 0, href: can.has("manage_classes") ? "/classes/new" : null, action: "Add a class" },
    { title: "Set prices and packs", done: summary.plans > 0, href: can.has("manage_classes") ? "/classes/plans" : null, action: "Set prices" },
    { title: "Add students", done: summary.active_students > 0, href: can.has("manage_students") ? "/students/new" : null, action: "Add a student" },
  ];
  const remaining = steps.filter((s) => !s.done).length;

  const n = summary.classes_today;
  return (
    <>
      <Counts />
      <Page>
        <PageHeader
          title={n === 0 ? "No classes today" : `${n} ${n === 1 ? "class" : "classes"} today`}
          lead={
            <>
              {formatDay(today)} at {studio.org_name}.{" "}
              {summary.unmarked > 0
                ? `${summary.unmarked} ${summary.unmarked === 1 ? "class still needs" : "classes still need"} marking.`
                : n > 0
                  ? "Attendance is up to date."
                  : ""}
            </>
          }
          actions={
            <>
              {can.has("manage_students") ? (
                <Link href="/students/new" className="btn btn-ghost">
                  <Icon name="plus" size={18} />
                  Add a student
                </Link>
              ) : null}
              <Link href="/attendance" className="btn btn-primary">
                Mark attendance
              </Link>
            </>
          }
        />

        {remaining > 0 ? (
          <section aria-labelledby="setup-title" className="panel flex flex-col gap-3 p-5 sm:p-6">
            <SectionHeader
              id="setup-title"
              title={`${remaining} ${remaining === 1 ? "step" : "steps"} to get ${studio.org_name} running`}
              aside={`${steps.length - remaining} of ${steps.length} done`}
            />
            <ol className="m-0 flex list-none flex-col gap-1.5 p-0">
              {steps.map((s) => (
                <li key={s.title} className="row flex min-h-[56px] items-center gap-3 px-3 py-2">
                  <span className={`flex size-7 shrink-0 items-center justify-center rounded-full ${s.done ? "tag-good" : "tag-quiet"}`}>
                    {s.done ? <Icon name="check" size={16} /> : null}
                  </span>
                  <span className={`min-w-0 flex-1 font-semibold ${s.done ? "muted" : ""}`}>
                    {s.title}
                    <span className="sr-only">{s.done ? " (done)" : " (to do)"}</span>
                  </span>
                  {!s.done && s.href ? (
                    <Link href={s.href} className="btn btn-dark min-h-[40px]">
                      {s.action}
                    </Link>
                  ) : null}
                </li>
              ))}
            </ol>
          </section>
        ) : null}

        <div className="grid grid-cols-1 gap-[18px] lg:grid-cols-3">
          <section aria-labelledby="att-title" className="hero flex flex-col gap-5 p-6 sm:p-8 lg:col-span-2">
            <h2 id="att-title" className="stat-label m-0 text-sm">
              Attendance, last four weeks
            </h2>
            <div className="flex flex-wrap items-end gap-x-5 gap-y-2">
              <span className="num text-[56px] sm:text-[72px]">{thisWeek === null ? "–" : formatPercent(thisWeek)}</span>
              <p className="m-0 max-w-[30ch] pb-2 text-base font-semibold" style={{ color: "var(--hero-soft)" }}>
                {finding}
              </p>
            </div>
            <Bars
              bars={bars}
              height={132}
              label={`Share of marked students who came, each day for four weeks. ${thisWeek === null ? "Nothing marked this week." : `This week ${formatPercent(thisWeek)}.`}`}
              from={formatDayShort(days[0].date)}
              to="Today"
            />
          </section>

          <section aria-labelledby="needs-title" className="panel flex flex-col gap-3 p-5 sm:p-6">
            <SectionHeader id="needs-title" title="Needs you" aside={needs.length > 0 ? String(needs.length) : undefined} />
            {needs.length === 0 ? (
              <EmptyState title="Nothing right now">Classes are marked, nobody is slipping and no fee is overdue.</EmptyState>
            ) : (
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {needs.map((item) => (
                  <li key={item.key}>
                    <Link href={item.href} className="row flex min-h-[60px] items-center justify-between gap-3 px-3.5 py-2.5 no-underline">
                      <span className="min-w-0">
                        <span className="block truncate font-bold">{item.title}</span>
                        <span className="caption block truncate">{item.detail}</span>
                      </span>
                      <span className="tag tag-attn shrink-0">{item.action}</span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        <div className="grid grid-cols-2 gap-[18px] lg:grid-cols-4">
          <Stat
            label="Classes today"
            value={n}
            note={summary.unmarked > 0 ? `${summary.unmarked} to mark` : "All marked"}
            tone={summary.unmarked > 0 ? "attn" : undefined}
            href="/attendance"
          />
          <Stat
            label="Active students"
            value={summary.active_students}
            note={month.joined > 0 ? `${month.joined} joined in ${monthLong(month.month)}` : "Paused and left aren't counted"}
            tone={month.joined > 0 ? "good" : undefined}
            href={can.has("manage_students") ? "/students" : undefined}
          />
          {summary.dues_paise !== null ? (
            <Stat
              label="Fees to collect"
              value={formatRupeesShort(summary.dues_paise)}
              note={overduePaise > 0 ? `${formatRupeesShort(overduePaise)} overdue` : summary.dues_paise > 0 ? `From ${summary.students_owing} students` : "Nobody owes anything"}
              tone={overduePaise > 0 ? "attn" : undefined}
              href="/fees"
            />
          ) : null}
          <Stat
            label="Classes running"
            value={summary.classes}
            note={`${summary.active_team} on the team. You're ${ROLE_LABEL[studio.role].toLowerCase()}.`}
            href="/classes"
          />
        </div>

        <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-3">
          {fees ? (
            <section aria-labelledby="money-title" className="panel flex flex-col gap-4 p-5 sm:p-6">
              {collection ? (
                <>
                  <div>
                    <h2 id="money-title" className="h-card">
                      {collection.charged_paise === 0
                        ? `${formatRupees(collection.collected_paise)} collected in ${monthLong(fees.month)}`
                        : `${formatRupeesShort(collection.collected_paise)} of ${formatRupeesShort(collection.charged_paise)} collected`}
                    </h2>
                    <p className="caption m-0 mt-1">Money received each day in {monthLong(fees.month)}</p>
                  </div>
                  {collection.charged_paise > 0 ? <Meter value={collection.collected_paise / collection.charged_paise} /> : null}
                  <Bars
                    height={84}
                    bars={collection.days.map((d) => ({
                      key: formatDayShort(d.date),
                      value: d.collected_paise === 0 ? null : d.collected_paise / dayMax,
                      tip: `${formatDayShort(d.date)}: ${formatRupees(d.collected_paise)}`,
                    }))}
                    label={`Money received each day in ${monthLong(fees.month)}, ${formatRupees(collection.collected_paise)} in all.`}
                    from={`1 ${monthShort(fees.month)}`}
                    to="Today"
                  />
                </>
              ) : (
                <>
                  <div>
                    <h2 id="money-title" className="h-card">
                      {overduePaise > 0 ? `${formatRupees(overduePaise)} is overdue` : "No fee is overdue"}
                    </h2>
                    <p className="caption m-0 mt-1">What&apos;s owed, by how late it is</p>
                  </div>
                  <Ranked
                    label="Fees owed by how late they are"
                    rows={fees.ageing.map((b) => ({
                      key: b.key,
                      name: AGE_LABEL[b.key],
                      value: b.due_paise > 0 ? formatRupees(b.due_paise) : "None",
                      share: b.due_paise / Math.max(1, ...fees.ageing.map((x) => x.due_paise)),
                      tone: b.key === "not_due" ? ("neutral" as const) : ("attn" as const),
                    }))}
                  />
                </>
              )}
              <Link href="/fees" className="btn btn-ghost self-start">
                Open fees
              </Link>
            </section>
          ) : null}

          <section aria-labelledby="fill-title" className="panel flex flex-col gap-4 p-5 sm:p-6">
            <div>
              <h2 id="fill-title" className="h-card">
                {batches.length === 0
                  ? "No classes yet"
                  : sized.length === 0
                    ? "Your biggest classes"
                    : full > 0
                      ? `${full} ${full === 1 ? "class is" : "classes are"} full`
                      : "Room in every class"}
              </h2>
              <p className="caption m-0 mt-1">{sized.length > 0 ? "Places taken, fullest first" : "Students in each class"}</p>
            </div>
            {classRows.length > 0 ? (
              <Ranked rows={classRows} label="Classes by how full they are" />
            ) : (
              <p className="soft m-0">Set up the timetable and classes appear here.</p>
            )}
            <Link href="/classes" className="btn btn-ghost self-start">
              Open the timetable
            </Link>
          </section>

          <section aria-labelledby="growth-title" className={`panel flex flex-col gap-4 p-5 sm:p-6 ${fees ? "" : "lg:col-span-2"}`}>
            <div>
              <h2 id="growth-title" className="h-card">
                {month.joined === 0 && month.left === 0
                  ? `Nobody joined or left in ${monthLong(month.month)}`
                  : `${month.joined} joined, ${month.left} left in ${monthLong(month.month)}`}
              </h2>
              <p className="caption m-0 mt-1">Students per month, last six months</p>
            </div>
            <Flow
              months={months.map((m) => ({ key: m.month, label: monthShort(m.month), up: m.joined, down: m.left }))}
              upName="Joined"
              downName="Left"
              label="Students who joined and who left in each of the last six months."
              height={84}
            />
          </section>
        </div>
      </Page>
    </>
  );
}
