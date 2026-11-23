import type { Metadata } from "next";
import Link from "next/link";

import { BackdatedPill } from "@/components/backdated-pill";
import { NotAllowed } from "@/components/not-allowed";
import { TakeOutButton } from "@/components/take-out-button";
import {
  getMe,
  getStudent,
  getStudentAttendance,
  getStudentCredits,
  getStudentEnrollments,
  getStudentFees,
  getSummary,
  listBatches,
  listBranches,
  listPlans,
} from "@/lib/api-server";
import { ageFrom, formatDate, formatPhone, formatRupees, scheduleSummary } from "@/lib/format";
import { ATTENDANCE_LABEL, PAYMENT_METHOD_LABEL, RELATION_LABEL, STATUS_LABEL } from "@/lib/types";

import { AdjustCredits, JoinClassForm, SellPackForm } from "./student-classes";
import { AddContactForm, EditStudentForm, RemoveContactButton } from "./student-editors";
import { AddChargeForm, CancelEntry, TakePaymentForm } from "./student-fees";

export const metadata: Metadata = { title: "Student" };

export default async function StudentPage({ params }: PageProps<"/students/[id]">) {
  const me = await getMe();
  const can = new Set(me.studio!.capabilities);
  if (!can.has("manage_students")) return <NotAllowed what="students" />;
  const { id } = await params;
  const canFees = can.has("manage_fees");
  const canAdjust = can.has("adjust_fees");
  const [student, branches, enrollments, credits, batches, plans, summary, attendance, fees] = await Promise.all([
    getStudent(id),
    listBranches(),
    getStudentEnrollments(id),
    getStudentCredits(id),
    listBatches(),
    listPlans(),
    getSummary(),
    getStudentAttendance(id),
    canFees ? getStudentFees(id) : null,
  ]);
  const short = (iso: string) => formatDate(iso).replace(/ \d{4}$/, "");
  const current = enrollments.filter((e) => e.ended_on === null);
  const past = enrollments.filter((e) => e.ended_on !== null);
  const inClass = new Set(current.map((e) => e.batch_id));
  const monthly = plans.filter((p) => p.kind === "monthly" && p.active);
  const packs = plans.filter((p) => p.kind !== "monthly" && p.active);

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
              <dd className="m-0 flex flex-wrap items-center gap-1.5 text-base font-semibold">
                {formatDate(student.joined_on)}
                <BackdatedPill date={student.joined_on} enteredOn={student.entered_on} />
              </dd>
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

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-labelledby="classes-title" className="glass flex min-w-0 flex-[3_1_420px] flex-col gap-3 p-6">
          <h2 id="classes-title" className="font-display m-0 text-[20px] font-semibold">
            Classes
          </h2>
          {current.length === 0 ? (
            <p className="soft m-0">Not in a class yet.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {current.map((e) => (
                <li key={e.id} className="row flex flex-wrap items-center justify-between gap-2 p-3.5">
                  <span className="min-w-0">
                    <Link href={`/classes/${e.batch_id}`} className="block font-bold">
                      {e.batch_name}
                    </Link>
                    <span className="muted text-[13px]">
                      {scheduleSummary(e.slots)}, {e.branch_name}
                    </span>
                    <span className="soft block text-[13px]">
                      {e.plan_name && e.plan_price_paise !== null
                        ? `${e.plan_name}, ${formatRupees(e.plan_price_paise)} a month`
                        : "Pays with class credits"}
                      {e.starts_on > summary.today ? `, starts ${formatDate(e.starts_on)}` : `, since ${formatDate(e.starts_on)}`}
                    </span>
                    <span className="mt-1 inline-flex">
                      <BackdatedPill date={e.starts_on} enteredOn={e.entered_on} />
                    </span>
                  </span>
                  <TakeOutButton enrollmentId={e.id} name={student.name} />
                </li>
              ))}
            </ul>
          )}
          {student.status === "active" ? (
            <JoinClassForm
              studentId={student.id}
              today={summary.today}
              classes={batches.map((b) => ({
                id: b.id,
                label: `${b.name}, ${scheduleSummary(b.slots)}${b.capacity !== null ? `, ${b.enrolled} of ${b.capacity}` : ""}`,
                full: b.capacity !== null && b.enrolled >= b.capacity,
                already: inClass.has(b.id),
              }))}
              plans={monthly.map((p) => ({ id: p.id, label: `${p.name}, ${formatRupees(p.price_paise)} a month` }))}
            />
          ) : (
            <p className="muted m-0 text-sm">Set them as active to put them in a class.</p>
          )}
          {past.length > 0 ? (
            <details className="disclosure">
              <summary className="flex min-h-[44px] items-center text-sm font-semibold">
                {past.length} past {past.length === 1 ? "class" : "classes"}
              </summary>
              <ul className="m-0 flex list-none flex-col gap-1 p-0 pt-1">
                {past.map((e) => (
                  <li key={e.id} className="muted text-sm">
                    {e.batch_name}, {formatDate(e.starts_on)} to {formatDate(e.ended_on!)}
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </section>

        <section aria-labelledby="credits-title" className="glass flex min-w-0 flex-[2_1_340px] flex-col gap-3 p-6">
          <h2 id="credits-title" className="font-display m-0 text-[20px] font-semibold">
            Class credits
          </h2>
          <div>
            <p className="font-display m-0 text-[44px] leading-none font-semibold tabular-nums">{credits.available}</p>
            <p className="soft m-0 mt-1">
              {credits.available === 0
                ? "No classes left to use."
                : `${credits.available === 1 ? "class" : "classes"} left to use${
                    credits.next_expiry
                      ? `. ${credits.next_expiry.classes} ${credits.next_expiry.on <= summary.today ? "expire today" : `expire on ${formatDate(credits.next_expiry.on)}`}.`
                      : "."
                  }`}
            </p>
          </div>
          {credits.lots.length > 0 ? (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label="Packs and free classes">
              {credits.lots.slice(0, 6).map((l) => (
                <li key={l.id} className="row flex items-center justify-between gap-3 p-3">
                  <span className="min-w-0">
                    <span className="block font-semibold">{l.plan_name ?? "Free classes"}</span>
                    <span className="muted text-[13px]">
                      {l.plan_name ? `${formatRupees(l.price_paise)}, ` : l.note ? `${l.note}, ` : ""}
                      {l.expired ? `expired ${formatDate(l.expires_on!)}` : l.expires_on ? `use by ${formatDate(l.expires_on)}` : "no expiry"}
                    </span>
                  </span>
                  <span className={`shrink-0 text-sm font-bold tabular-nums ${l.expired ? "muted line-through" : ""}`}>
                    {l.remaining} of {l.classes}
                  </span>
                </li>
              ))}
            </ul>
          ) : null}
          {can.has("manage_fees") && student.status !== "left" ? (
            <SellPackForm
              studentId={student.id}
              packs={packs.map((p) => ({
                id: p.id,
                label: `${p.name}, ${formatRupees(p.price_paise)}${p.kind === "pack" ? `, ${p.classes} classes in ${p.valid_days} days` : ""}`,
              }))}
            />
          ) : null}
          {can.has("manage_classes") && student.status !== "left" ? (
            <AdjustCredits studentId={student.id} available={credits.available} />
          ) : null}
        </section>
      </div>

      {fees ? (
        <section id="fees" aria-labelledby="fees-title" className="glass flex scroll-mt-6 flex-col gap-4 p-6">
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <h2 id="fees-title" className="font-display m-0 text-[20px] font-semibold">
              Fees
            </h2>
            <p className="m-0 text-sm font-semibold">
              {fees.due_paise > 0 ? (
                <span className={fees.overdue_paise > 0 ? "attn-text" : ""}>
                  {formatRupees(fees.due_paise)} to pay
                  {fees.overdue_paise > 0 ? `, ${formatRupees(fees.overdue_paise)} of it overdue` : ""}
                </span>
              ) : fees.advance_paise > 0 ? (
                <span>{formatRupees(fees.advance_paise)} paid in advance</span>
              ) : (
                <span className="soft">Nothing to pay</span>
              )}
            </p>
          </div>

          <div className="flex flex-wrap items-start gap-[18px]">
            <div className="flex min-w-0 flex-[3_1_380px] flex-col gap-3">
              {fees.charges.length === 0 ? (
                <p className="soft m-0">No charges yet. Monthly fees, packs and anything you add by hand appear here.</p>
              ) : (
                <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label="Charges">
                  {fees.charges.slice(0, 12).map((c) => {
                    const left = c.amount_paise - c.paid_paise;
                    return (
                      <li key={c.id} className="row flex flex-wrap items-center justify-between gap-x-3 gap-y-1 p-3">
                        <span className="min-w-0 flex-1">
                          <span className={`block font-semibold ${c.voided_at ? "muted line-through" : ""}`}>{c.description}</span>
                          <span className="muted text-[13px]">
                            {c.voided_at ? `Cancelled: ${c.void_reason}` : `Due ${short(c.due_on)}`}
                          </span>
                        </span>
                        <span className="flex flex-col items-end">
                          <span className={`font-bold tabular-nums ${c.voided_at ? "muted line-through" : ""}`}>{formatRupees(c.amount_paise)}</span>
                          {c.voided_at ? null : left === 0 ? (
                            <span className="tag tag-good">Paid</span>
                          ) : (
                            <span className={c.overdue ? "tag tag-attn" : "tag tag-quiet"}>
                              {c.paid_paise > 0 ? `${formatRupees(left)} left` : c.overdue ? "Overdue" : "Not paid"}
                            </span>
                          )}
                        </span>
                        {canAdjust && !c.voided_at ? <CancelEntry kind="charge" id={c.id} label={c.description} /> : null}
                      </li>
                    );
                  })}
                </ul>
              )}
              <AddChargeForm studentId={student.id} today={summary.today} />
              {fees.payments.length > 0 ? (
                <>
                  <h3 className="m-0 text-base font-bold">Payments</h3>
                  <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label="Payments">
                    {fees.payments.slice(0, 12).map((p) => (
                      <li key={p.id} className="row flex flex-wrap items-center justify-between gap-x-3 gap-y-1 p-3">
                        <span className="min-w-0 flex-1">
                          <Link href={`/fees/receipts/${p.id}`} className="block font-semibold">
                            Receipt {p.receipt_no}
                          </Link>
                          <span className="muted text-[13px]">
                            {short(p.received_on)} · {PAYMENT_METHOD_LABEL[p.method]} · {p.received_by_name}
                            {p.voided_at ? ` · Cancelled: ${p.void_reason}` : ""}
                          </span>
                        </span>
                        <span className={`font-bold tabular-nums ${p.voided_at ? "muted line-through" : ""}`}>{formatRupees(p.amount_paise)}</span>
                        {canAdjust && !p.voided_at ? <CancelEntry kind="payment" id={p.id} label={`receipt ${p.receipt_no}`} /> : null}
                      </li>
                    ))}
                  </ul>
                </>
              ) : null}
            </div>
            <div className="min-w-0 flex-[2_1_320px]">
              <TakePaymentForm studentId={student.id} duePaise={fees.due_paise} today={summary.today} />
            </div>
          </div>
        </section>
      ) : null}

      <section aria-labelledby="attendance-title" className="glass flex flex-col gap-3 p-6">
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <h2 id="attendance-title" className="font-display m-0 text-[20px] font-semibold">
            Attendance
          </h2>
          <p className="soft m-0 text-sm font-semibold">
            {attendance.came + attendance.missed === 0
              ? "Nothing marked in the last 30 days"
              : `Last 30 days: came ${attendance.came}, missed ${attendance.missed}`}
          </p>
        </div>
        {attendance.recent.length === 0 ? (
          <p className="soft m-0">No classes marked yet.</p>
        ) : (
          <ul className="m-0 flex list-none flex-wrap gap-1.5 p-0">
            {attendance.recent.map((m, i) => (
              <li
                key={`${m.on_date}-${m.batch_name}-${i}`}
                className={m.status === "absent" ? "tag tag-attn" : m.status === "excused" ? "tag tag-quiet" : "tag tag-good"}
                title={`${m.batch_name}${m.drop_in ? " (drop-in)" : ""}`}
              >
                {formatDate(m.on_date).replace(/ \d{4}$/, "")}: {ATTENDANCE_LABEL[m.status]}
                <span className="sr-only">, {m.batch_name}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <EditStudentForm
        student={{
          id: student.id,
          name: student.name,
          date_of_birth: student.date_of_birth,
          branch_id: student.branch_id,
          status: student.status,
          notes: student.notes,
          joined_on: student.joined_on,
          entered_on: student.entered_on,
        }}
        today={summary.today}
        branches={branches.map((b) => ({ id: b.id, name: b.name }))}
      />
    </div>
  );
}
