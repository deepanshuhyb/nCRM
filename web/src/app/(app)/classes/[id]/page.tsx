import type { Metadata } from "next";
import Link from "next/link";

import { BackdatedPill } from "@/components/backdated-pill";
import { TakeOutButton } from "@/components/take-out-button";
import { getBatch, getMe, getSummary, listBranches, listPlans, listRooms, listStaff } from "@/lib/api-server";
import { WEEKDAY_LONG, formatDate, formatRupees } from "@/lib/format";
import { STATUS_LABEL } from "@/lib/types";

import { ClassForm } from "../class-form";
import { AddToClass } from "./roster-editors";

export const metadata: Metadata = { title: "Class" };

export default async function ClassPage({ params }: PageProps<"/classes/[id]">) {
  const { id } = await params;
  const [me, batch, summary] = await Promise.all([getMe(), getBatch(id), getSummary()]);
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  const manageStudents = can.has("manage_students");
  const manageClasses = can.has("manage_classes");

  const [plans, branches, rooms, team] = await Promise.all([
    manageStudents ? listPlans() : Promise.resolve([]),
    manageClasses ? listBranches() : Promise.resolve([]),
    manageClasses ? listRooms() : Promise.resolve([]),
    manageClasses ? listStaff() : Promise.resolve([]),
  ]);
  const monthlyPlans = plans.filter((p) => p.kind === "monthly" && p.active);

  const full = batch.capacity !== null && batch.enrolled >= batch.capacity;
  const ratio = batch.capacity ? Math.min(1, batch.enrolled / batch.capacity) : 0;
  const mine = batch.instructor_id === studio.staff_id;

  return (
    <div className="flex flex-col gap-[18px]">
      <Link href="/classes" className="btn btn-quiet self-start px-0">
        Timetable
      </Link>

      <div className="flex flex-wrap items-start gap-[18px]">
        <section aria-labelledby="class-name" className="glass min-w-0 flex-[3_1_420px] p-6 sm:p-8">
          <div className="flex flex-wrap items-center gap-3">
            <h1 id="class-name" className="font-display m-0 text-[30px] leading-tight font-semibold sm:text-[38px]">
              {batch.name}
            </h1>
            {batch.style ? <span className="tag tag-quiet">{batch.style}</span> : null}
            {!batch.active ? <span className="tag tag-attn">Archived</span> : null}
          </div>

          <ul className="m-0 mt-5 flex list-none flex-wrap gap-2 p-0" aria-label="When it meets">
            {batch.slots.map((s) => (
              <li key={`${s.weekday}-${s.starts_at}`} className="chip">
                {WEEKDAY_LONG[s.weekday - 1]} {s.starts_at}–{s.ends_at}
              </li>
            ))}
          </ul>

          <dl className="mt-6 grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-4">
            <div>
              <dt className="muted text-[13px]">Branch</dt>
              <dd className="m-0 text-base font-semibold">{batch.branch_name}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Room</dt>
              <dd className="m-0 text-base font-semibold">{batch.room_name ?? "Not set"}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Teacher</dt>
              <dd className="m-0 text-base font-semibold">{mine ? "You" : batch.instructor_name ?? "Not decided"}</dd>
            </div>
            <div>
              <dt className="muted text-[13px]">Runs</dt>
              <dd className="m-0 text-base font-semibold">
                {batch.ends_on ? `${formatDate(batch.starts_on)} to ${formatDate(batch.ends_on)}` : `From ${formatDate(batch.starts_on)}`}
              </dd>
            </div>
          </dl>

          <div className="mt-6">
            <div className="flex items-baseline justify-between gap-3">
              <span className="font-display text-[34px] leading-none font-semibold tabular-nums">
                {batch.enrolled}
                {batch.capacity !== null ? <span className="muted text-[20px]"> of {batch.capacity}</span> : null}
              </span>
              <span className={full ? "attn-text font-semibold" : "soft font-semibold"}>
                {batch.capacity === null
                  ? "No limit set"
                  : full
                    ? "Full"
                    : `${batch.capacity - batch.enrolled} ${batch.capacity - batch.enrolled === 1 ? "place" : "places"} left`}
              </span>
            </div>
            {batch.capacity !== null ? (
              <div
                className="meter mt-3"
                data-full={full ? "" : undefined}
                role="img"
                aria-label={`${batch.enrolled} of ${batch.capacity} places taken`}
              >
                <span style={{ width: `${ratio * 100}%` }} />
              </div>
            ) : null}
          </div>
        </section>

        <section aria-labelledby="roster-title" className="glass flex min-w-0 flex-[2_1_360px] flex-col gap-3 p-6">
          <h2 id="roster-title" className="font-display m-0 text-[20px] font-semibold">
            Students
          </h2>
          {batch.roster === null ? (
            <p className="soft m-0">
              {batch.enrolled} {batch.enrolled === 1 ? "student is" : "students are"} in this class. Their names are shown to the
              class&apos;s teacher and the front desk.
            </p>
          ) : batch.roster.length === 0 ? (
            <p className="soft m-0">No one in this class yet.</p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {batch.roster.map((r) => (
                <li key={r.enrollment_id} className="row flex flex-wrap items-center justify-between gap-2 p-3">
                  <span className="min-w-0">
                    {manageStudents ? (
                      <Link href={`/students/${r.student_id}`} className="block font-bold">
                        {r.student_name}
                      </Link>
                    ) : (
                      <span className="block font-bold">{r.student_name}</span>
                    )}
                    <span className="muted text-[13px]">
                      {r.plan_name ?? "Pays with class credits"}
                      {r.starts_on > summary.today ? `, starts ${formatDate(r.starts_on)}` : `, since ${formatDate(r.starts_on)}`}
                      {r.student_status !== "active" ? `, ${STATUS_LABEL[r.student_status].toLowerCase()}` : ""}
                    </span>
                    <span className="mt-1 inline-flex">
                      <BackdatedPill date={r.starts_on} enteredOn={r.entered_on} />
                    </span>
                  </span>
                  {manageStudents ? <TakeOutButton enrollmentId={r.enrollment_id} name={r.student_name} /> : null}
                </li>
              ))}
            </ul>
          )}
          {manageStudents && batch.active ? (
            full ? (
              <p className="attn-text m-0 font-semibold">The class is full. Raise its capacity to add more.</p>
            ) : (
              <AddToClass
                batchId={batch.id}
                inClass={(batch.roster ?? []).map((r) => r.student_id)}
                plans={monthlyPlans.map((p) => ({ id: p.id, label: `${p.name}, ${formatRupees(p.price_paise)} a month` }))}
                today={summary.today}
              />
            )
          ) : null}
        </section>
      </div>

      {manageClasses ? (
        <details className="glass disclosure">
          <summary className="flex min-h-[60px] items-center px-6 font-semibold">Edit class, schedule or archive</summary>
          <div className="px-6 pb-6">
            <ClassForm
              branches={branches.filter((b) => b.active || b.id === batch.branch_id).map((b) => ({ id: b.id, name: b.name }))}
              rooms={rooms.filter((r) => r.active || r.id === batch.room_id)}
              team={team.filter((t) => t.active || t.id === batch.instructor_id).map((t) => ({ id: t.id, name: t.name }))}
              today={summary.today}
              initial={{
                id: batch.id,
                name: batch.name,
                style: batch.style,
                branch_id: batch.branch_id,
                room_id: batch.room_id,
                instructor_id: batch.instructor_id,
                capacity: batch.capacity,
                starts_on: batch.starts_on,
                ends_on: batch.ends_on,
                active: batch.active,
                slots: batch.slots,
                enrolled: batch.enrolled,
              }}
            />
          </div>
        </details>
      ) : null}
    </div>
  );
}
