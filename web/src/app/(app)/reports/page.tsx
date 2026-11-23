import type { Metadata } from "next";
import Link from "next/link";

import { DownloadCsv } from "@/components/download-csv";
import { Meter } from "@/components/graphs";
import { NotAllowed } from "@/components/not-allowed";
import { EmptyState, Page, PageHeader, SectionHeader, Stat } from "@/components/ui";
import { getMe, getPeriodReport, getSummary } from "@/lib/api-server";
import { addDays, formatDate, formatPercent, formatRupees, monthLong } from "@/lib/format";
import { PAYMENT_METHOD_LABEL } from "@/lib/types";

export const metadata: Metadata = { title: "Reports" };

const ISO = /^\d{4}-\d{2}-\d{2}$/;

/** The first day of the month `n` months from the one `iso` is in. */
function monthStart(iso: string, n: number): string {
  const d = new Date(Date.UTC(Number(iso.slice(0, 4)), Number(iso.slice(5, 7)) - 1 + n, 1));
  return d.toISOString().slice(0, 10);
}

export default async function ReportsPage({ searchParams }: { searchParams: Promise<{ from?: string; to?: string }> }) {
  const me = await getMe();
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  if (!can.has("view_financials") && !can.has("manage_staff")) return <NotAllowed what="reports" />;
  const { today } = await getSummary();
  const asked = await searchParams;

  // Three ready-made periods, worked out from the studio's own today.
  const presets = [
    { label: "This month", from: monthStart(today, 0), to: today },
    { label: "Last month", from: monthStart(today, -1), to: addDays(monthStart(today, 0), -1) },
    { label: "Last 3 months", from: monthStart(today, -2), to: today },
  ];
  const valid = asked.from && asked.to && ISO.test(asked.from) && ISO.test(asked.to) && asked.from <= asked.to;
  const from = valid ? asked.from! : presets[0].from;
  const to = valid ? asked.to! : presets[0].to;
  const preset = presets.find((p) => p.from === from && p.to === to);
  const report = await getPeriodReport(from, to);
  const stamp = `${report.from}_to_${report.to}`;

  const came = report.classes.reduce((n, c) => n + c.came, 0);
  const marked = came + report.classes.reduce((n, c) => n + c.away, 0);
  // Lowest attendance first: the classes to look at. Unmarked ones last.
  const classes = [...report.classes].sort((a, b) => {
    const ra = a.came + a.away === 0 ? 2 : a.came / (a.came + a.away);
    const rb = b.came + b.away === 0 ? 2 : b.came / (b.came + b.away);
    return ra - rb || a.name.localeCompare(b.name);
  });
  const money = report.money;
  const topMethod = money ? Math.max(1, ...money.by_method.map((m) => m.paise)) : 1;
  const topMonth = money ? Math.max(1, ...money.by_month.map((m) => m.collected_paise)) : 1;

  return (
    <Page>
      <PageHeader title="Reports" lead={`${formatDate(report.from)} to ${formatDate(report.to)}.`} />

      <div className="flex flex-wrap items-center gap-3">
        <nav className="segmented" aria-label="Period">
          {presets.map((p) => (
            <Link key={p.label} href={`/reports?from=${p.from}&to=${p.to}`} aria-current={preset === p ? "page" : undefined}>
              {p.label}
            </Link>
          ))}
        </nav>
        <details className="disclosure panel min-w-0 flex-[1_1_280px]" open={preset ? undefined : true}>
          <summary className="flex min-h-[44px] items-center px-4 text-sm font-semibold">Other dates</summary>
          <form method="get" action="/reports" className="flex flex-wrap items-end gap-2 px-4 pb-4">
            <div>
              <label htmlFor="report-from" className="field-label">
                From
              </label>
              <input id="report-from" name="from" type="date" className="field" defaultValue={from} max={today} required />
            </div>
            <div>
              <label htmlFor="report-to" className="field-label">
                To
              </label>
              <input id="report-to" name="to" type="date" className="field" defaultValue={to} max={today} required />
            </div>
            <button type="submit" className="btn btn-dark">
              Show
            </button>
          </form>
        </details>
      </div>

      <div className="grid grid-cols-2 gap-2.5 sm:gap-[18px] lg:grid-cols-4">
        {money ? (
          <Stat
            label="Collected"
            value={formatRupees(money.collected_paise)}
            note={`${money.payments} ${money.payments === 1 ? "payment" : "payments"}`}
          />
        ) : null}
        {money ? (
          <Stat
            label="Fell due"
            value={formatRupees(money.charged_paise)}
            // What came in isn't only for these fees (some paid old ones), so
            // this is a comparison of two totals, not "x% of these were paid".
            note={money.charged_paise > 0 ? `Collected equals ${formatPercent(money.collected_paise / money.charged_paise)} of this` : "Nothing fell due"}
          />
        ) : null}
        <Stat
          label="Attendance"
          value={marked === 0 ? "–" : formatPercent(came / marked)}
          note={marked === 0 ? "Nothing marked" : `${came} of ${marked} marks`}
        />
        <Stat
          label="Students"
          value={String(report.students.active)}
          note={`${report.students.joined} joined, ${report.students.left} left`}
        />
      </div>

      <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-2">
        <section aria-labelledby="classes-title" className="panel flex flex-col gap-3 p-4 sm:p-5">
          <SectionHeader
            id="classes-title"
            title="Classes"
            aside="Lowest attendance first"
            action={
              <DownloadCsv
                filename={`classes_${stamp}.csv`}
                headers={["Class", "Branch", "Meetings held", "Cancelled", "Came", "Away", "Attendance %"]}
                rows={classes.map((c) => [
                  c.name,
                  c.branch_name,
                  c.meetings,
                  c.cancelled,
                  c.came,
                  c.away,
                  c.came + c.away === 0 ? "" : Math.round((c.came / (c.came + c.away)) * 100),
                ])}
              />
            }
          />
          {classes.length === 0 ? (
            <EmptyState title="No classes met">Nothing was scheduled in these dates.</EmptyState>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {classes.map((c, i) => {
                const total = c.came + c.away;
                return (
                  <li key={c.batch_id} className="row flex flex-col gap-1.5 px-3.5 py-2.5">
                    <span className="flex items-baseline justify-between gap-3">
                      <Link href={`/classes/${c.batch_id}`} className="stretch min-w-0 truncate font-bold">
                        {c.name}
                      </Link>
                      <span className="shrink-0 font-bold tabular-nums">{total === 0 ? "Not marked" : formatPercent(c.came / total)}</span>
                    </span>
                    <Meter value={total === 0 ? 0 : c.came / total} tone={total > 0 && c.came / total < 0.7 ? "attn" : undefined} index={i} />
                    <span className="caption">
                      {c.branch_name} · {c.meetings} {c.meetings === 1 ? "meeting" : "meetings"}
                      {c.cancelled > 0 ? ` · ${c.cancelled} cancelled` : ""}
                      {total > 0 ? ` · ${c.came} of ${total} came` : ""}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <div className="flex min-w-0 flex-col gap-[18px]">
          {money ? (
            <section aria-labelledby="money-title" className="panel flex flex-col gap-3 p-4 sm:p-5">
              <SectionHeader
                id="money-title"
                title="Money received"
                action={
                  <DownloadCsv
                    filename={`money_${stamp}.csv`}
                    headers={["How", "Payments", "Rupees"]}
                    rows={money.by_method.map((m) => [PAYMENT_METHOD_LABEL[m.method], m.payments, m.paise / 100])}
                  />
                }
              />
              {money.by_method.length === 0 ? (
                <EmptyState title="Nothing came in">No payment was recorded in these dates.</EmptyState>
              ) : (
                <ul className="m-0 flex list-none flex-col gap-3 p-0" aria-label="By how it was paid">
                  {money.by_method.map((m, i) => (
                    <li key={m.method} className="flex flex-col gap-1.5">
                      <span className="flex items-baseline justify-between gap-3 text-sm">
                        <span className="font-semibold">
                          {PAYMENT_METHOD_LABEL[m.method]} <span className="muted font-normal">· {m.payments}</span>
                        </span>
                        <span className="font-semibold tabular-nums">{formatRupees(m.paise)}</span>
                      </span>
                      <Meter value={m.paise / topMethod} tone="neutral" index={i} />
                    </li>
                  ))}
                </ul>
              )}
              {money.by_month.length > 1 ? (
                <>
                  <h3 className="h-card mt-2">Month by month</h3>
                  <ul className="m-0 flex list-none flex-col gap-3 p-0">
                    {money.by_month.map((m, i) => (
                      <li key={m.month} className="flex flex-col gap-1.5">
                        <span className="flex items-baseline justify-between gap-3 text-sm">
                          <span className="font-semibold">{monthLong(m.month)}</span>
                          <span className="font-semibold tabular-nums">{formatRupees(m.collected_paise)}</span>
                        </span>
                        <Meter value={m.collected_paise / topMonth} index={i} />
                      </li>
                    ))}
                  </ul>
                </>
              ) : null}
            </section>
          ) : null}

          {report.teachers ? (
            <section aria-labelledby="teachers-title" className="panel flex flex-col gap-3 p-4 sm:p-5">
              <SectionHeader
                id="teachers-title"
                title="Teachers"
                aside="Classes taken"
                action={
                  <DownloadCsv
                    filename={`teachers_${stamp}.csv`}
                    headers={["Teacher", "Classes scheduled", "Taken", "Absent", "Not marked"]}
                    rows={report.teachers.map((t) => [t.name, t.classes, t.present, t.absent, t.unmarked])}
                  />
                }
              />
              {report.teachers.length === 0 ? (
                <EmptyState title="No classes with a teacher">Classes in these dates had no teacher set.</EmptyState>
              ) : (
                <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                  {report.teachers.map((t) => (
                    <li key={t.staff_id} className="row flex items-center justify-between gap-3 px-3.5 py-2.5">
                      <span className="min-w-0">
                        <span className="block truncate font-bold">{t.name}</span>
                        <span className="caption block">
                          {t.classes} scheduled
                          {t.absent > 0 ? ` · ${t.absent} absent` : ""}
                          {t.unmarked > 0 ? ` · ${t.unmarked} not marked` : ""}
                        </span>
                      </span>
                      <span className="shrink-0 text-right">
                        <span className="block font-bold tabular-nums">{t.present}</span>
                        <span className="caption">taken</span>
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          ) : null}
        </div>
      </div>
    </Page>
  );
}
