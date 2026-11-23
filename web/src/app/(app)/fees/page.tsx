import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { getFeesSummary, getMe, getSummary, listBranches, listDues, listPayments } from "@/lib/api-server";
import { addDays, formatDate, formatDay, formatRupees } from "@/lib/format";
import { PAYMENT_METHOD_LABEL } from "@/lib/types";

import { MonthlyFeesSetup } from "./fee-settings";

export const metadata: Metadata = { title: "Fees" };

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
  const [fees, home, branches] = await Promise.all([getFeesSummary(), getSummary(), listBranches()]);
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

  return (
    <div className="flex flex-col gap-[18px]">
      <div>
        <h1 className="font-display m-0 text-[32px] leading-tight font-semibold">Fees</h1>
        <p className="soft mt-2 mb-0">
          {fees.due_paise === 0
            ? "Nothing to collect right now."
            : `${formatRupees(fees.due_paise)} to collect from ${fees.students_owing} ${fees.students_owing === 1 ? "student" : "students"}.`}
        </p>
      </div>

      <div className="grid grid-cols-1 gap-[18px] sm:grid-cols-3">
        <div className="glass flex flex-col gap-2 p-[22px]">
          <span className="muted text-sm font-medium">To collect</span>
          <span className="font-display text-[34px] leading-none font-semibold tabular-nums">{formatRupees(fees.due_paise)}</span>
          <span className="soft text-[13px]">Everything charged and not yet paid</span>
        </div>
        <div className={`glass ${fees.overdue_paise > 0 ? "glass-attn" : ""} flex flex-col gap-2 p-[22px]`}>
          <span className="muted text-sm font-medium">Overdue</span>
          <span className="font-display text-[34px] leading-none font-semibold tabular-nums">{formatRupees(fees.overdue_paise)}</span>
          <span className={fees.overdue_paise > 0 ? "attn-text text-[13px] font-semibold" : "soft text-[13px]"}>
            {fees.overdue_paise > 0 ? "Past its due date" : "Nothing is late"}
          </span>
        </div>
        {fees.collected_this_month_paise !== null ? (
          <div className="glass flex flex-col gap-2 p-[22px]">
            <span className="muted text-sm font-medium">Collected in {thisMonth}</span>
            <span className="font-display text-[34px] leading-none font-semibold tabular-nums">
              {formatRupees(fees.collected_this_month_paise)}
            </span>
            <span className="soft text-[13px]">Money received so far this month</span>
          </div>
        ) : null}
      </div>

      {can.has("manage_classes") ? (
        <MonthlyFeesSetup feesFrom={fees.fees_from} dueDay={fees.fee_due_day} today={today} />
      ) : fees.fees_from === null ? (
        <p className="glass m-0 p-5 text-sm font-semibold">
          Monthly fees aren&apos;t being created yet. An owner or manager switches them on here.
        </p>
      ) : null}

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-labelledby="dues-title" className="glass flex min-w-0 flex-[3_1_440px] flex-col gap-3 p-4 sm:p-6">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="dues-title" className="font-display m-0 text-[20px] font-semibold">
              {dues.total === 0 ? "Nobody owes anything" : `${dues.total} ${dues.total === 1 ? "student owes" : "students owe"}`}
              {branch ? ` at ${branch.name}` : ""}
            </h2>
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
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {dues.items.map((d) => (
                <li key={d.student_id} className="row flex flex-wrap items-center justify-between gap-3 p-3">
                  <span className="min-w-0 flex-1">
                    <Link href={`/students/${d.student_id}#fees`} className="block font-bold">
                      {d.student_name}
                    </Link>
                    <span className="muted text-[13px]">{d.branch_name ?? "No branch"}</span>
                  </span>
                  <span className="flex flex-col items-end">
                    <span className="font-bold tabular-nums">{formatRupees(d.due_paise)}</span>
                    {d.overdue_paise > 0 ? (
                      <span className="attn-text text-[13px] font-semibold">Overdue since {formatDate(d.oldest_due_on).replace(/ \d{4}$/, "")}</span>
                    ) : (
                      <span className="muted text-[13px]">Due {formatDate(d.oldest_due_on).replace(/ \d{4}$/, "")}</span>
                    )}
                  </span>
                  <Link href={`/students/${d.student_id}#fees`} className="btn btn-ghost">
                    Take payment
                  </Link>
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

        <section aria-labelledby="payments-title" className="glass flex min-w-0 flex-[2_1_340px] flex-col gap-3 p-4 sm:p-6">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 id="payments-title" className="font-display m-0 text-[20px] font-semibold">
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
          <p className="font-display m-0 text-[34px] leading-none font-semibold tabular-nums">{formatRupees(payments.collected_paise)}</p>
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
    </div>
  );
}
