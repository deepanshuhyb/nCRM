import type { Metadata } from "next";
import Link from "next/link";

import { Grain } from "@/components/grain";
import { getMe, listBranches } from "@/lib/api-server";
import { ROLE_LABEL } from "@/lib/types";

export const metadata: Metadata = { title: "Home" };

/**
 * The owner's home. Students, classes and fees aren't built yet, so instead
 * of fake numbers the hero is the setup checklist, and the tiles say
 * plainly what will appear there.
 */
export default async function HomePage() {
  const [me, branches] = await Promise.all([getMe(), listBranches()]);
  const studio = me.studio!;
  const firstName = studio.staff_name.split(" ")[0];
  const canManageBranches = studio.capabilities.includes("manage_branches");

  const steps = [
    {
      title: "Add your branches",
      done: branches.length > 0,
      detail: branches.length > 0 ? `${branches.length} added` : "Start with where you teach",
      href: "/branches",
    },
    { title: "Add teachers and front desk", done: false, detail: "Coming next", href: null },
    { title: "Add students and batches", done: false, detail: "Coming next", href: null },
    { title: "Connect WhatsApp", done: false, detail: "Coming next", href: null },
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
            <h1 id="setup-title" className="font-display m-0 max-w-[14ch] text-[34px] leading-[1.04] font-semibold sm:text-[52px]">
              {remaining === 0 ? "Your studio is set up." : `${remaining} steps to get ${studio.org_name} running.`}
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
                {step.href && !step.done && canManageBranches ? (
                  <Link href={step.href} className="btn btn-primary">
                    Add branches
                  </Link>
                ) : step.href && step.done ? (
                  <Link href={step.href} className="btn btn-ghost">
                    View
                  </Link>
                ) : null}
              </li>
            ))}
          </ol>
        </section>

        <section aria-label="At a glance" className="grid min-w-0 flex-[5_1_380px] grid-cols-2 gap-[18px]">
          <Link href="/branches" className="glass flex flex-col justify-between gap-4 p-[22px] no-underline" style={{ color: "inherit" }}>
            <span className="muted text-sm font-medium">Branches</span>
            <span className="font-display text-[48px] leading-none font-semibold">{branches.length}</span>
            <span className="soft text-[13px]">{branches.filter((b) => b.active).length} open</span>
          </Link>
          <div className="glass flex flex-col justify-between gap-4 p-[22px]">
            <span className="muted text-sm font-medium">Your role</span>
            <span className="font-display text-[26px] leading-tight font-semibold">{ROLE_LABEL[studio.role]}</span>
            <span className="soft text-[13px]">{studio.capabilities.length} kinds of access</span>
          </div>
          <div className="glass flex flex-col justify-between gap-4 p-[22px]">
            <span className="muted text-sm font-medium">Students</span>
            <span className="font-display text-[26px] leading-tight font-semibold">Not yet</span>
            <span className="soft text-[13px]">They&apos;ll show here once you add them.</span>
          </div>
          <div className="glass flex flex-col justify-between gap-4 p-[22px]">
            <span className="muted text-sm font-medium">Attendance</span>
            <span className="font-display text-[26px] leading-tight font-semibold">Not yet</span>
            <span className="soft text-[13px]">Starts once classes are set up.</span>
          </div>
        </section>
      </div>
    </div>
  );
}
