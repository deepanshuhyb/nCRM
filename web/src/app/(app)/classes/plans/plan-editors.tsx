"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { parseRupees } from "@/lib/format";
import { PLAN_KIND_LABEL, type Plan, type PlanKind } from "@/lib/types";

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

function rupeesText(paise: number) {
  return paise % 100 === 0 ? String(paise / 100) : (paise / 100).toFixed(2);
}

type Draft = { name: string; price: string; classes: string; validDays: string };

function PlanFields({ id, kind, draft, onChange }: { id: string; kind: PlanKind; draft: Draft; onChange: (d: Draft) => void }) {
  return (
    <>
      <div>
        <label htmlFor={`${id}-name`} className="field-label">
          Name
        </label>
        <input
          id={`${id}-name`}
          className="field"
          value={draft.name}
          maxLength={80}
          autoComplete="off"
          placeholder={kind === "monthly" ? "Monthly, twice a week" : kind === "pack" ? "8 classes" : "Drop-in"}
          onChange={(e) => onChange({ ...draft, name: e.target.value })}
        />
      </div>
      <div className={kind === "pack" ? "grid grid-cols-3 gap-3" : kind === "drop_in" ? "grid grid-cols-2 gap-3" : undefined}>
        <div>
          <label htmlFor={`${id}-price`} className="field-label">
            Price (₹)
          </label>
          <input
            id={`${id}-price`}
            className="field"
            inputMode="decimal"
            value={draft.price}
            maxLength={12}
            placeholder="2500"
            onChange={(e) => onChange({ ...draft, price: e.target.value })}
          />
        </div>
        {kind === "pack" ? (
          <div>
            <label htmlFor={`${id}-classes`} className="field-label">
              Classes
            </label>
            <input
              id={`${id}-classes`}
              className="field"
              inputMode="numeric"
              value={draft.classes}
              maxLength={3}
              placeholder="8"
              onChange={(e) => onChange({ ...draft, classes: e.target.value })}
            />
          </div>
        ) : null}
        {kind !== "monthly" ? (
          <div>
            <label htmlFor={`${id}-days`} className="field-label">
              Valid days
            </label>
            <input
              id={`${id}-days`}
              className="field"
              inputMode="numeric"
              value={draft.validDays}
              maxLength={3}
              placeholder="30"
              onChange={(e) => onChange({ ...draft, validDays: e.target.value })}
            />
          </div>
        ) : null}
      </div>
    </>
  );
}

/** Turn a draft into an API body, or a sentence saying what's wrong. */
function toBody(kind: PlanKind, d: Draft): { body: Record<string, unknown> } | { error: string } {
  if (!d.name.trim()) return { error: "Give the plan a name." };
  const price = parseRupees(d.price);
  if (price === null) return { error: "Type the price in rupees, like 2500." };
  const whole = (s: string) => (/^\d+$/.test(s.trim()) ? Number(s.trim()) : null);
  const classes = kind === "pack" ? whole(d.classes) : null;
  if (kind === "pack" && classes === null) return { error: "Say how many classes the pack has." };
  const validDays = kind === "monthly" ? null : d.validDays.trim() ? whole(d.validDays) : null;
  if (kind !== "monthly" && d.validDays.trim() && validDays === null) return { error: "Valid days is a whole number." };
  if (kind === "pack" && validDays === null) return { error: "Say how many days the pack is valid." };
  return { body: { name: d.name.trim(), price_paise: price, classes, valid_days: validDays } };
}

export function NewPlanForm() {
  const router = useRouter();
  const [kind, setKind] = useState<PlanKind>("monthly");
  const [draft, setDraft] = useState<Draft>({ name: "", price: "", classes: "", validDays: "" });
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(null);
    const result = toBody(kind, draft);
    if ("error" in result) return setError(result.error);
    setBusy(true);
    try {
      await apiSend("POST", "/plans", { ...result.body, kind });
      setSaved(`${draft.name.trim()} added.`);
      setDraft({ name: "", price: "", classes: "", validDays: "" });
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="glass flex flex-col gap-4 p-6" aria-labelledby="new-plan-title">
      <h2 id="new-plan-title" className="font-display m-0 text-[20px] font-semibold">
        Add a price
      </h2>
      <div className="segmented self-start" role="group" aria-label="Kind of price">
        {(Object.keys(PLAN_KIND_LABEL) as PlanKind[]).map((k) => (
          <button key={k} type="button" aria-pressed={kind === k} onClick={() => setKind(k)}>
            {PLAN_KIND_LABEL[k]}
          </button>
        ))}
      </div>
      <PlanFields id="new-plan" kind={kind} draft={draft} onChange={setDraft} />
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <p role="status" className="accent-text m-0 font-semibold empty:hidden">
        {saved}
      </p>
      <button type="submit" className="btn btn-primary" disabled={busy}>
        {busy ? "Adding…" : "Add price"}
      </button>
    </form>
  );
}

export function EditPlan({ plan }: { plan: Plan }) {
  const router = useRouter();
  const [draft, setDraft] = useState<Draft>({
    name: plan.name,
    price: rupeesText(plan.price_paise),
    classes: plan.classes != null ? String(plan.classes) : "",
    validDays: plan.valid_days != null ? String(plan.valid_days) : "",
  });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save(active: boolean) {
    setError(null);
    const result = toBody(plan.kind, draft);
    if ("error" in result) return setError(result.error);
    setBusy(true);
    try {
      await apiSend("PUT", `/plans/${plan.id}`, { ...result.body, active });
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="disclosure">
      <summary className="flex min-h-[44px] items-center text-sm font-semibold">Edit {plan.name}</summary>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save(plan.active);
        }}
        noValidate
        className="flex flex-col gap-3 pt-2"
      >
        <PlanFields id={`plan-${plan.id}`} kind={plan.kind} draft={draft} onChange={setDraft} />
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <div className="flex flex-wrap gap-2">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "Saving…" : "Save"}
          </button>
          <button type="button" className={plan.active ? "btn btn-ghost" : "btn btn-dark"} disabled={busy} onClick={() => save(!plan.active)}>
            {plan.active ? "Stop offering" : "Offer again"}
          </button>
        </div>
      </form>
    </details>
  );
}
