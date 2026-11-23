import type { Metadata } from "next";
import Link from "next/link";

import { Grain } from "@/components/grain";
import { getMe, getSummary } from "@/lib/api-server";
import { formatRupees } from "@/lib/format";
import { ROLE_LABEL } from "@/lib/types";

export const metadata: Metadata = { title: "Home" };

/**
 * Home. The hero is the setup checklist (each step done when its data
 * exists); the tiles show today's real numbers. Money only appears for
 * roles that handle fees: the API sends null to everyone else.
 */
export default async function HomePage() {
  const [me, summary] = await Promise.all([getMe(), getSummary()]);
  const studio = me.studio!;
  const can = new Set(studio.capabilities);
  const firstName = studio.staff_name.split(" ")[0];

  const steps = [
    {
      title: "Add your branches",
      done: summary.branches > 0,
      detail: summary.branches > 0 ? `${summary.branches} added` : "Start with where you teach",
      action: can.has("manage_branches") ? { href: "/branches", label: "Add branches" } : null,
      view: "/branches",
    },
    {
      title: "Add teachers and front desk",
      done: summary.active_team > 1,
      detail: summary.active_team > 1 ? `${summary.active_team} people on the team` : "So they can sign in and mark attendance",
      action: can.has("manage_staff") ? { href: "/team", label: "Add people" } : null,
      view: can.has("manage_staff") ? "/team" : null,
    },
    {
      title: "Set up classes",
      done: summary.classes > 0,
      detail: summary.classes > 0 ? `${summary.classes} on the timetable` : "Days, times, room and teacher",
      action: can.has("manage_classes") ? { href: "/classes/new", label: "Add a class" } : null,
      view: "/classes",
    },
    {
      title: "Set prices and packs",
      done: summary.plans > 0,
      detail: summary.plans > 0 ? `${summary.plans} offered` : "Monthly fees, packs and drop-ins",
      action: can.has("manage_classes") ? { href: "/classes/plans", label: "Set prices" } : null,
      view: can.has("manage_students") ? "/classes/plans" : null,
    },
    {
      title: "Add students",
      done: summary.active_students > 0,
      detail: summary.active_students > 0 ? `${summary.active_students} active` : "With the number we should message",
      action: can.has("manage_students") ? { href: "/students/new", label: "Add a student" } : null,
      view: can.has("manage_students") ? "/students" : null,
    },
    { title: "Connect WhatsApp", done: false, detail: "Comes next", action: null, view: null },
  ];
  const remaining = steps.filter((s) => !s.done).length;

  return (
    <div className="flex flex-col gap-[18px]">
      <div className="flex flex-wrap items-stretch gap-[18px]">
        <section aria-labelledby="setup-title" className="glass-hero min-w-0 flex-[7_1_520px] p-6 sm:p-[34px]">
          <Grain />
          <div>
            <p className="soft m-0 mb-2.5 text-base font-medium">
              Welcome back, {firstName}. You&apos;re in {studio.org_name}.
            </p>
            <h1 id="setup-title" className="font-display m-0 max-w-[15ch] text-[34px] leading-[1.04] font-semibold sm:text-[52px]">
              {remaining === 0
                ? "Your studio is set up."
                : `${remaining} ${remaining === 1 ? "step" : "steps"} to get ${studio.org_name} running.`}
            </h1>
          </div>
          <ol className="m-0 mt-6 flex list-none flex-col gap-2.5 p-0">
            {steps.map((step, i) => (
              <li key={step.title} className="row flex flex-wrap items-center gap-x-4 gap-y-2 py-3 pr-3 pl-3.5">
                <span
                  aria-hidden="true"
                  className="font-display flex size-11 shrink-0 items-center justify-center rounded-xl text-base font-semibold"
                  style={{
                    background: step.done ? "var(--good)" : "var(--track)",
                    color: step.done ? "var(--good-text)" : "var(--soft)",
                  }}
                >
                  {step.done ? "✓" : i + 1}
                </span>
                <span className="min-w-[180px] flex-1">
                  <span className="block text-base font-bold">{step.title}</span>
                  <span className="muted text-[13px]">
                    {step.done ? "Done, " : ""}
                    {step.detail}
                  </span>
                </span>
                {!step.done && step.action ? (
                  <Link href={step.action.href} className="btn btn-primary">
                    {step.action.label}
                  </Link>
                ) : step.done && step.view ? (
                  <Link href={step.view} className="btn btn-ghost">
                    View
                  </Link>
                ) : null}
              </li>
            ))}
          </ol>
        </section>

        <section aria-label="At a glance" className="grid min-w-0 flex-[5_1_380px] grid-cols-2 gap-[18px]">
          <Tile
            label="Active students"
            value={String(summary.active_students)}
            note={summary.active_students === 0 ? "Add your first student" : "Paused and left aren't counted"}
            href={can.has("manage_students") ? "/students" : null}
          />
          <Tile
            label="Team"
            value={String(summary.active_team)}
            note={`You're ${ROLE_LABEL[studio.role].toLowerCase()}`}
            href={can.has("manage_staff") ? "/team" : null}
          />
          <Tile
            label="Classes"
            value={String(summary.classes)}
            note={summary.classes === 0 ? "Set up your timetable" : `${summary.plans} ${summary.plans === 1 ? "price" : "prices"} set`}
            href="/classes"
          />
          <Tile
            label="Classes today"
            value={String(summary.classes_today)}
            note={
              summary.unmarked > 0
                ? `${summary.unmarked} ${summary.unmarked === 1 ? "class needs" : "classes need"} marking`
                : "Attendance is up to date"
            }
            href="/attendance"
            attention={summary.unmarked > 0}
          />
          {summary.dues_paise !== null ? (
            <Tile
              label="Fees to collect"
              value={formatRupees(summary.dues_paise)}
              note={
                summary.dues_paise > 0
                  ? `From ${summary.students_owing} ${summary.students_owing === 1 ? "student" : "students"}`
                  : "Nobody owes anything"
              }
              href="/fees"
              wide
            />
          ) : null}
        </section>
      </div>
    </div>
  );
}

function Tile({
  label,
  value,
  note,
  href,
  attention,
  wide,
}: {
  label: string;
  value: string;
  note: string;
  href: string | null;
  attention?: boolean;
  /** Spans both columns; for long values like money. */
  wide?: boolean;
}) {
  const body = (
    <>
      <span className="muted text-sm font-medium">{label}</span>
      <span className={`font-display leading-none font-semibold tabular-nums ${wide ? "text-[36px]" : "text-[48px]"}`}>{value}</span>
      <span className={attention ? "attn-text text-[13px] font-semibold" : "soft text-[13px]"}>{note}</span>
    </>
  );
  return href ? (
    <Link
      href={href}
      className={`glass ${attention ? "glass-attn" : ""} ${wide ? "col-span-2" : ""} flex flex-col justify-between gap-4 p-[22px] no-underline`}
      style={{ color: "inherit" }}
    >
      {body}
    </Link>
  ) : (
    <div className={`glass ${wide ? "col-span-2" : ""} flex flex-col justify-between gap-4 p-[22px]`}>{body}</div>
  );
}
