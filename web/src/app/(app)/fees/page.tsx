import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { Bars, Meter, Ranked } from "@/components/graphs";
import { TravelLink, Travels } from "@/components/travel";
import { Page, PageHeader } from "@/components/ui";
import { getFeesReport, getFeesSummary, getMe, getSummary, listBranches, listDues, listPayments } from "@/lib/api-server";
import { addDays, formatDay, formatDayShort, formatPercent, formatRupees } from "@/lib/format";
import { PAYMENT_METHOD_LABEL } from "@/lib/types";

import { MonthlyFeesSetup } from "./fee-settings";

export const metadata: Metadata = { title: "Fees" };

const AGE_LABEL = { not_due: "Not due yet", week: "Up to a week late", month: "Up to a month late", older: "More than a month late" } as const;

const PAGE = 50;
/** How far back the payments list goes for roles without financial reports. */
const DESK_DAYS = 7;

function monthName(iso: string): string {
  const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  return months[Number(iso.slice(5, 7)) - 1] ?? "";
}

export default async function FeesPage({ searchParams }: PageProps<"/fees">) {
  const me = await getMe();
  const can = new Set(me.studio!.capabilities);
  if (!can.has("manage_fees")) return <NotAllowed what="fees" />;

  const params = await searchParams;
  const [fees, home, branches, report] = await Promise.all([getFeesSummary(), getSummary(), listBranches(), getFeesReport()]);
  const today = home.today;
  const branch = branches.find((b) => b.id === params.branch);
  const page = Math.max(1, Math.min(10_000, Number(params.page) || 1));
  const oldest = addDays(today, can.has("view_financials") ? -365 : -DESK_DAYS);
  const asked = typeof params.day === "string" && /^\d{4}-\d{2}-\d{2}$/.test(params.day) ? params.day : today;
  const day = asked > today || asked < oldest ? today : asked;
  const [dues, payments] = await Promise.all([
    listDues({ branchId: branch?.id, limit: PAGE, offset: (page - 1) * PAGE }),
    listPayments(day),
  ]);

  const href = (next: { branch?: string | null; page?: number; day?: string }) => {
    const sp = new URLSearchParams();
    const b = next.branch === undefined ? branch?.id : next.branch;
    if (b) sp.set("branch", b);
    const pg = next.page ?? (next.branch === undefined ? page : 1);
    if (pg > 1) sp.set("page", String(pg));
    const d = next.day ?? day;
    if (d !== today) sp.set("day", d);
    const qs = sp.toString();
    return `/fees${qs ? `?${qs}` : ""}`;
  };
  const pages = Math.max(1, Math.ceil(dues.total / PAGE));
  const thisMonth = monthName(today);

  const ageMax = Math.max(1, ...report.ageing.map((x) => x.due_paise));
  const collection = report.collection;
  const dayMax = collection ? Math.max(1, ...collection.days.map((d) => d.collected_paise)) : 1;
  const ageing = (
    <Ranked
      label="Fees owed by how late they are"
      rows={report.ageing.map((x) => ({
        key: x.key,
        name: AGE_LABEL[x.key],
        value: x.due_paise > 0 ? `${formatRupees(x.due_paise)} · ${x.students} ${x.students === 1 ? "student" : "students"}` : "None",
        share: x.due_paise / ageMax,
        tone: x.key === "not_due" ? ("neutral" as const) : ("attn" as const),
      }))}
    />
  );

  return (
    <Page>
      <PageHeader
        title="Fees"
        lead={
          fees.due_paise === 0
            ? "Nothing to collect right now."
            : `${formatRupees(fees.due_paise)} to collect from ${fees.students_owing} ${fees.students_owing === 1 ? "student" : "students"}.`
        }
      />

      <div className="grid grid-cols-1 gap-[18px] lg:grid-cols-3">
        <section aria-labelledby="fees-hero" className="hero flex flex-col gap-5 p-6 sm:p-8 lg:col-span-2">
          {collection ? (
            <>
              <h2 id="fees-hero" className="stat-label m-0 text-sm">
                Collected in {thisMonth}
              </h2>
              <div className="flex flex-wrap items-end gap-x-5 gap-y-2">
                <span className="num text-[44px] sm:text-[60px]">{formatRupees(collection.collected_paise)}</span>
                {collection.charged_paise > 0 ? (
                  <p className="m-0 max-w-[30ch] pb-2 font-semibold" style={{ color: "var(--hero-soft)" }}>
                    of {formatRupees(collection.charged_paise)} due this month, {formatPercent(Math.min(1, collection.collected_paise / collection.charged_paise))} in.
                  </p>
                ) : null}
              </div>
              {collection.charged_paise > 0 ? <Meter value={collection.collected_paise / collection.charged_paise} /> : null}
              <Bars
                height={110}
                bars={collection.days.map((d) => ({
                  key: formatDayShort(d.date),
                  value: d.collected_paise === 0 ? null : d.collected_paise / dayMax,
                  tip: `${formatDayShort(d.date)}: ${formatRupees(d.collected_paise)}`,
                }))}
                label={`Money received each day in ${thisMonth}, ${formatRupees(collection.collected_paise)} in all.`}
                from={`1 ${thisMonth.slice(0, 3)}`}
                to="Today"
              />
            </>
          ) : (
            <>
              <h2 id="fees-hero" className="stat-label m-0 text-sm">
                To collect
              </h2>
              <span className="num text-[44px] sm:text-[60px]">{formatRupees(fees.due_paise)}</span>
              <p className="m-0 font-semibold" style={{ color: "var(--hero-soft)" }}>
                {fees.overdue_paise > 0 ? `${formatRupees(fees.overdue_paise)} of it is past its due date.` : "Nothing is late."}
              </p>
            </>
          )}
        </section>

        <section aria-labelledby="age-title" className="panel flex flex-col gap-4 p-5 sm:p-6">
          <div>
            <h2 id="age-title" className="h-card">
              {fees.overdue_paise > 0
                ? `${formatRupees(fees.overdue_paise)} is overdue`
                : fees.due_paise > 0
                  ? "Nothing is overdue yet"
                  : "Nobody owes anything"}
            </h2>
            <p className="caption m-0 mt-1">
              What&apos;s owed, by how late it is
            </p>
          </div>
          {ageing}
        </section>
      </div>

      {can.has("manage_classes") ? (
        <MonthlyFeesSetup feesFrom={fees.fees_from} dueDay={fees.fee_due_day} today={today} />
      ) : fees.fees_from === null ? (
        <p className="panel m-0 p-5 font-semibold">Monthly fees aren&apos;t being created yet. An owner or manager switches them on here.</p>
      ) : null}

      <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-5">
        <section aria-labelledby="dues-title" className="panel flex min-w-0 flex-col gap-3 p-4 sm:p-6 lg:col-span-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="dues-title" className="h-section">
              {dues.total === 0 ? "Nobody owes anything" : `${dues.total} ${dues.total === 1 ? "student owes" : "students owe"}`}
              {branch ? ` at ${branch.name}` : ""}
            </h2>
            {dues.total > 0 ? <p className="caption m-0 w-full">Most overdue first. Tap a student to take a payment.</p> : null}
            {branches.length > 1 ? (
              <nav aria-label="Branch" className="segmented">
                <Link href={href({ branch: null })} aria-current={!branch ? "page" : undefined}>
                  All
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
          </div>
          {dues.items.length === 0 ? (
            <p className="soft m-0">
              {fees.due_paise === 0 ? "When a fee is due or a pack is sold on credit, it shows up here." : "Nobody at this branch owes anything."}
            </p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0" data-live>
              {dues.items.map((d) => (
                <li key={d.student_id}>
                  <TravelLink
                    kind="student"
                    id={d.student_id}
                    label={d.student_name}
                    href={`/students/${d.student_id}#fees`}
                    className="row flex min-h-[60px] items-center justify-between gap-3 px-3.5 py-2.5 no-underline"
                  >
                    <span className="min-w-0">
                      <Travels kind="student" id={d.student_id}>
                        <span className="block truncate font-bold">{d.student_name}</span>
                      </Travels>
                      <span className={`block truncate text-[13px] ${d.overdue_paise > 0 ? "attn-text font-semibold" : "muted"}`}>
                        {d.overdue_paise > 0 ? "Overdue since" : "Due"} {formatDayShort(d.oldest_due_on)}
                        {d.branch_name ? ` · ${d.branch_name}` : ""}
                      </span>
                    </span>
                    <span className="shrink-0 text-base font-bold tabular-nums">{formatRupees(d.due_paise)}</span>
                  </TravelLink>
                </li>
              ))}
            </ul>
          )}
          {pages > 1 ? (
            <nav aria-label="Pages" className="flex items-center justify-between gap-3">
              {page > 1 ? (
                <Link href={href({ page: page - 1 })} className="btn btn-ghost">
                  Previous
                </Link>
              ) : (
                <span />
              )}
              <span className="muted text-sm">
                Page {page} of {pages}
              </span>
              {page < pages ? (
                <Link href={href({ page: page + 1 })} className="btn btn-ghost">
                  Next
                </Link>
              ) : (
                <span />
              )}
            </nav>
          ) : null}
        </section>

        <section aria-labelledby="payments-title" className="panel flex min-w-0 flex-col gap-3 p-4 sm:p-6 lg:col-span-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 id="payments-title" className="h-section">
              {day === today ? "Received today" : `Received on ${formatDay(day)}`}
            </h2>
            <nav aria-label="Day" className="flex gap-2">
              {day > oldest ? (
                <Link href={href({ day: addDays(day, -1) })} className="btn btn-quiet">
                  Day before
                </Link>
              ) : null}
              {day < today ? (
                <Link href={href({ day: addDays(day, 1) })} className="btn btn-quiet">
                  Day after
                </Link>
              ) : null}
            </nav>
          </div>
          <p className="num m-0 text-[34px]">{formatRupees(payments.collected_paise)}</p>
          {payments.by_method.length > 0 ? (
            <ul className="m-0 flex list-none flex-wrap gap-1.5 p-0" aria-label="By method">
              {payments.by_method.map((m) => (
                <li key={m.method} className="chip">
                  {PAYMENT_METHOD_LABEL[m.method]} {formatRupees(m.amount_paise)}
                </li>
              ))}
            </ul>
          ) : null}
          {payments.items.length === 0 ? (
            <p className="soft m-0">No payments recorded for this day.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {payments.items.map((p) => (
                <li key={p.id}>
                  <Link
                    href={`/fees/receipts/${p.id}`}
                    className="row flex items-center justify-between gap-3 p-3 no-underline"
                    style={{ color: "inherit" }}
                  >
                    <span className="min-w-0">
                      <span className="block font-semibold">{p.student_name}</span>
                      <span className="muted text-[13px]">
                        Receipt {p.receipt_no} · {PAYMENT_METHOD_LABEL[p.method]} · {p.received_by_name}
                      </span>
                    </span>
                    <span className="flex shrink-0 flex-col items-end">
                      <span className={`font-bold tabular-nums ${p.voided_at ? "muted line-through" : ""}`}>{formatRupees(p.amount_paise)}</span>
                      {p.voided_at ? <span className="tag tag-attn">Cancelled</span> : null}
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </Page>
  );
}
