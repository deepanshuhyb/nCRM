import type { Metadata } from "next";
import Link from "next/link";

import { BackdatedPill } from "@/components/backdated-pill";
import { Dots } from "@/components/graphs";
import { Tabs } from "@/components/tabs";
import { Travels } from "@/components/travel";
import { Page, PageHeader, SectionHeader, Stat } from "@/components/ui";
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
import { ageFrom, formatDate, formatPercent, formatPhone, formatRupees, scheduleSummary } from "@/lib/format";
import { ATTENDANCE_LABEL, PAYMENT_METHOD_LABEL, RELATION_LABEL, STATUS_LABEL } from "@/lib/types";

import { AdjustCredits, JoinClassForm, SellPackForm } from "./student-classes";
import { AddContactForm, EditStudentForm, RemoveContactButton } from "./student-editors";
import { AddChargeForm, CancelEntry, DiscountEntry, TakePaymentForm } from "./student-fees";

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

  const marks = [...attendance.recent].reverse();
  const markedTotal = attendance.came + attendance.missed;
  const tabs = [
    { id: "classes", label: "Classes", badge: current.length > 0 ? <span className="tag tag-quiet hidden sm:inline-flex">{current.length}</span> : undefined },
    ...(fees
      ? [
          {
            id: "fees",
            label: "Fees",
            badge: fees.overdue_paise > 0 ? <span className="tag tag-attn">Overdue</span> : fees.due_paise > 0 ? <span className="tag tag-quiet">Due</span> : undefined,
          },
        ]
      : []),
    { id: "attendance", label: "Attendance" },
    { id: "details", label: "Details" },
  ];
  const attendanceDots =
    marks.length === 0 ? null : (
      <Dots
        label={`The last ${marks.length} classes, oldest first.`}
        dots={marks.map((m, i) => ({
          key: `${m.on_date}-${m.batch_name}-${i}`,
          tone: m.status === "absent" ? ("attn" as const) : m.status === "excused" ? ("neutral" as const) : m.status === "late" ? ("late" as const) : undefined,
          tip: `${short(m.on_date)}: ${ATTENDANCE_LABEL[m.status]}, ${m.batch_name}${m.drop_in ? " (drop-in)" : ""}`,
        }))}
      />
    );

  const classesPanel = (
    <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-5">
        <section aria-labelledby="classes-title" className="panel flex min-w-0 flex-col gap-3 p-5 sm:p-6 lg:col-span-3">
          <h2 id="classes-title" className="h-section">
            Classes
          </h2>
          {current.length === 0 ? (
            <p className="soft m-0">Not in a class yet.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {current.map((e) => (
                <li key={e.id} className="row px-3.5 pt-1 pb-2.5">
                  {/* Name and action share the first line; the details get the full width below. */}
                  <div className="flex flex-wrap items-center justify-between gap-x-2">
                    <Link href={`/classes/${e.batch_id}`} className="stretch min-w-0 font-bold">
                      {e.batch_name}
                    </Link>
                    <TakeOutButton enrollmentId={e.id} name={student.name} />
                  </div>
                  <span className="caption -mt-1 block">
                    {scheduleSummary(e.slots)} · {e.branch_name}
                  </span>
                  <span className="caption flex flex-wrap items-center gap-x-1.5">
                    {e.plan_name && e.plan_price_paise !== null ? `${formatRupees(e.plan_price_paise)} a month` : "Pays with class credits"}
                    {e.starts_on > summary.today ? ` · starts ${short(e.starts_on)}` : ` · since ${short(e.starts_on)}`}
                    <BackdatedPill date={e.starts_on} enteredOn={e.entered_on} />
                  </span>
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

        <section aria-labelledby="credits-title" className="panel flex min-w-0 flex-col gap-3 p-5 sm:p-6 lg:col-span-2">
          <h2 id="credits-title" className="h-section">
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
  );

  const feesPanel = fees ? (
        <section aria-labelledby="fees-title" className="panel flex flex-col gap-4 p-5 sm:p-6">
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <h2 id="fees-title" className="h-section">
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
                      <li key={c.id} className="row flex flex-col gap-1 p-3">
                        {/* What and how much on top; the actions get a line of
                            their own, so they never squeeze the description. */}
                        <div className="flex items-start justify-between gap-3">
                          <span className="min-w-0">
                            <span className={`block font-semibold ${c.voided_at ? "muted line-through" : ""}`}>{c.description}</span>
                            <span className="muted block text-[13px]">
                              {c.voided_at ? `Cancelled: ${c.void_reason}` : `Due ${short(c.due_on)}`}
                              {!c.voided_at && c.discount_paise > 0 ? ` · ${formatRupees(c.discount_paise)} off: ${c.discount_reason}` : ""}
                            </span>
                          </span>
                          <span className="flex shrink-0 flex-col items-end gap-0.5">
                            <span className={`font-bold tabular-nums ${c.voided_at ? "muted line-through" : ""}`}>{formatRupees(c.amount_paise)}</span>
                            {c.voided_at ? null : left === 0 ? (
                              <span className="tag tag-good">Paid</span>
                            ) : (
                              <span className={c.overdue ? "tag tag-attn" : "tag tag-quiet"}>
                                {c.paid_paise > 0 ? `${formatRupees(left)} left` : c.overdue ? "Overdue" : "Not paid"}
                              </span>
                            )}
                          </span>
                        </div>
                        {canAdjust && !c.voided_at ? (
                          <div className="flex flex-wrap items-center justify-end gap-x-1">
                            {/* Nothing left to take off once it is paid in full. */}
                            {left > 0 ? <DiscountEntry id={c.id} label={c.description} leftPaise={left} /> : null}
                            <CancelEntry kind="charge" id={c.id} label={c.description} />
                          </div>
                        ) : null}
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
                          <Link href={`/fees/receipts/${p.id}`} className="stretch block font-semibold">
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
  ) : null;

  const attendancePanel = (
    <section aria-labelledby="attendance-title" className="panel flex flex-col gap-4 p-5 sm:p-6">
      <SectionHeader
        id="attendance-title"
        title={markedTotal === 0 ? "Nothing marked in the last 30 days" : `Came to ${attendance.came} of ${markedTotal} classes in the last 30 days`}
      />
      {attendanceDots ?? <p className="soft m-0">No classes marked yet.</p>}
      {marks.length > 0 ? (
        <>
          <p className="caption m-0 flex flex-wrap gap-x-4 gap-y-1 font-semibold" aria-hidden="true">
            <span>
              <i className="g-swatch" style={{ borderRadius: 999 }} />
              Came
            </span>
            <span>
              <i className="g-swatch" data-tone="attn" />
              Missed
            </span>
            <span>Ring: late or excused</span>
          </p>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {attendance.recent.slice(0, 10).map((m, i) => (
              <li key={`${m.on_date}-${m.batch_name}-${i}`} className="row flex items-center justify-between gap-3 px-3.5 py-2.5">
                <span className="min-w-0">
                  <span className="block truncate font-semibold">{m.batch_name}</span>
                  <span className="caption">
                    {short(m.on_date)}
                    {m.drop_in ? " · drop-in" : ""}
                  </span>
                </span>
                <span className={m.status === "absent" ? "tag tag-attn" : m.status === "excused" ? "tag tag-quiet" : "tag tag-good"}>{ATTENDANCE_LABEL[m.status]}</span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </section>
  );

  const detailsPanel = (
    <div className="grid grid-cols-1 items-start gap-[18px] lg:grid-cols-2">
        <section aria-labelledby="contacts-title" className="panel flex min-w-0 flex-col gap-3 p-5 sm:p-6">
          <h2 id="contacts-title" className="h-section">
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
      <div className="flex flex-col gap-[18px]">
        {student.notes ? (
          <section aria-label="Notes" className="panel p-5 sm:p-6">
            <h2 className="h-card mb-2">Notes</h2>
            <p className="m-0 whitespace-pre-line">{student.notes}</p>
          </section>
        ) : null}
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
    </div>
  );

  return (
    <Page>
      <PageHeader
        back={{ href: "/students", label: "All students" }}
        title={
          <Travels kind="student" id={student.id}>
            <h1 className="h-page">{student.name}</h1>
          </Travels>
        }
        lead={
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
            {[age !== null ? `Age ${age}` : null, branch?.name, `Joined ${formatDate(student.joined_on)}`].filter(Boolean).join(" · ")}
            <BackdatedPill date={student.joined_on} enteredOn={student.entered_on} />
          </span>
        }
      >
        <span className={statusClass}>{STATUS_LABEL[student.status]}</span>
      </PageHeader>

      <div className={`grid gap-2.5 sm:gap-[18px] ${fees ? "grid-cols-3" : "grid-cols-2"}`}>
        {fees ? (
          <Stat
            label="Fees"
            value={fees.due_paise > 0 ? formatRupees(fees.due_paise) : fees.advance_paise > 0 ? formatRupees(fees.advance_paise) : "Paid up"}
            note={fees.overdue_paise > 0 ? `${formatRupees(fees.overdue_paise)} overdue` : fees.due_paise > 0 ? "To pay" : fees.advance_paise > 0 ? "Paid in advance" : "Nothing to pay"}
            tone={fees.overdue_paise > 0 ? "attn" : undefined}
          />
        ) : null}
        <Stat
          label="Attendance"
          value={markedTotal === 0 ? "–" : formatPercent(attendance.came / markedTotal)}
          note={markedTotal === 0 ? "Nothing marked" : `${attendance.came} of ${markedTotal} in 30 days`}
          tone={attendance.missed >= 3 && attendance.missed > attendance.came ? "attn" : undefined}
        />
        <Stat
          label="Classes"
          value={current.length}
          note={credits.available > 0 ? `${credits.available} ${credits.available === 1 ? "credit" : "credits"} left` : current.length === 0 ? "Not in one yet" : "No credits"}
        />
      </div>

      <Tabs label={`${student.name}'s record`} tabs={tabs} panels={[classesPanel, ...(fees ? [feesPanel] : []), attendancePanel, detailsPanel]} />
    </Page>
  );
}
