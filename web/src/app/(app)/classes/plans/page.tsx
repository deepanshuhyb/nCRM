import type { Metadata } from "next";
import Link from "next/link";

import { NotAllowed } from "@/components/not-allowed";
import { Page } from "@/components/ui";
import { getMe, listPlans } from "@/lib/api-server";
import { formatRupees } from "@/lib/format";
import type { Plan, PlanKind } from "@/lib/types";

import { EditPlan, NewPlanForm } from "./plan-editors";

export const metadata: Metadata = { title: "Prices and packs" };

const SECTIONS: { kind: PlanKind; title: string; empty: string }[] = [
  { kind: "monthly", title: "Monthly fees", empty: "No monthly fees yet. Students in a regular class usually pay one." },
  { kind: "pack", title: "Class packs", empty: "No packs yet. A pack is several classes to use within some days." },
  { kind: "drop_in", title: "Drop-ins", empty: "No drop-in price yet. A drop-in is a single class." },
];

/** What the plan gives, in words. The price sits next to it, so it isn't repeated. */
function terms(p: Plan): string {
  if (p.kind === "monthly") return "Every month, while they're in a class";
  if (p.kind === "drop_in") return `One class, used within ${p.valid_days} days`;
  const each = Math.round(p.price_paise / (p.classes ?? 1));
  return `${p.classes} classes at ${formatRupees(each)} each, used within ${p.valid_days} days`;
}

export default async function PlansPage() {
  const me = await getMe();
  const can = new Set(me.studio!.capabilities);
  if (!can.has("manage_students")) return <NotAllowed what="prices" />;
  const plans = await listPlans();
  const canEdit = can.has("manage_classes");

  return (
    <Page>
      <Link href="/classes" className="back-link self-start">
        Classes
      </Link>
      <div>
        <h1 className="h-page">Prices and packs</h1>
        <p className="lead">
          {canEdit
            ? "Changing a price doesn't change packs already sold."
            : "Front desk sells packs from a student's profile. Owners and managers set the prices."}
        </p>
      </div>

      <div className="flex flex-wrap items-start gap-[18px]">
        <div className="flex min-w-0 flex-[2_1_460px] flex-col gap-[18px]">
          {SECTIONS.map((section) => {
            const list = plans.filter((p) => p.kind === section.kind);
            return (
              <section key={section.kind} aria-labelledby={`plans-${section.kind}`} className="panel flex flex-col gap-2 p-5">
                <h2 id={`plans-${section.kind}`} className="h-section mb-1">
                  {section.title}
                </h2>
                {list.length === 0 ? (
                  <p className="muted m-0">{section.empty}</p>
                ) : (
                  <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                    {list.map((p) => (
                      <li key={p.id} className="row flex flex-col gap-2 p-3.5">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="min-w-0">
                            <span className="block font-bold">{p.name}</span>
                            <span className="muted text-[13px]">{terms(p)}</span>
                          </span>
                          <span className="flex items-center gap-2">
                            <span className="font-display text-[20px] font-semibold tabular-nums">{formatRupees(p.price_paise)}</span>
                            {p.active ? null : <span className="tag tag-quiet">Not offered</span>}
                          </span>
                        </div>
                        {canEdit ? <EditPlan plan={p} /> : null}
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            );
          })}
        </div>
        {canEdit ? (
          <aside className="min-w-0 flex-[1_1_320px]">
            <NewPlanForm />
          </aside>
        ) : null}
      </div>
    </Page>
  );
}
