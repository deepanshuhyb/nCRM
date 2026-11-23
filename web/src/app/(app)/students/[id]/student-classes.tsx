"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { BackdatableDate } from "@/components/backdatable-date";
import { ApiError, apiSend } from "@/lib/api-client";
import { addDays } from "@/lib/format";
import { PAYMENT_METHOD_LABEL, PAYMENT_METHODS, type PaymentMethod } from "@/lib/types";

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

type ClassOption = { id: string; label: string; full: boolean; already: boolean };

/** Put this student in a class. */
export function JoinClassForm({
  studentId,
  classes,
  plans,
  today,
}: {
  studentId: string;
  classes: ClassOption[];
  plans: { id: string; label: string }[];
  today: string;
}) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [batchId, setBatchId] = useState("");
  const [planId, setPlanId] = useState(plans.length === 1 ? plans[0].id : "");
  const [startsOn, setStartsOn] = useState(today);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!open) {
    return (
      <button type="button" className="btn btn-ghost self-start" onClick={() => setOpen(true)} disabled={classes.length === 0}>
        {classes.length === 0 ? "No classes to join yet" : "Put in a class"}
      </button>
    );
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!batchId) return setError("Choose a class.");
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/students/${studentId}/enrollments`, {
        batch_id: batchId,
        plan_id: planId || null,
        starts_on: startsOn || null,
      });
      setOpen(false);
      setBatchId("");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="row flex flex-col gap-3 p-4">
      <div>
        <label htmlFor="join-class" className="field-label">
          Class
        </label>
        <select id="join-class" className="select w-full" style={{ minHeight: 52 }} value={batchId} onChange={(e) => setBatchId(e.target.value)}>
          <option value="">Choose…</option>
          {classes.map((c) => (
            <option key={c.id} value={c.id} disabled={c.full || c.already}>
              {c.label}
              {c.already ? " (already in)" : c.full ? " (full)" : ""}
            </option>
          ))}
        </select>
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1.4fr_1fr]">
        <div>
          <label htmlFor="join-plan" className="field-label">
            How they pay
          </label>
          <select id="join-plan" className="select w-full" style={{ minHeight: 52 }} value={planId} onChange={(e) => setPlanId(e.target.value)}>
            <option value="">With class credits</option>
            {plans.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </div>
        <BackdatableDate
          id="join-start"
          label="From"
          value={startsOn}
          onChange={setStartsOn}
          max={addDays(today, 366)}
          enteredOn={today}
          min={addDays(today, -366)}
        />
      </div>
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <div className="flex flex-wrap gap-2">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Adding…" : "Add to class"}
        </button>
        <button type="button" className="btn btn-quiet" onClick={() => setOpen(false)}>
          Cancel
        </button>
      </div>
    </form>
  );
}

/** Sell a pack or a drop-in (front desk and up). */
export function SellPackForm({ studentId, packs }: { studentId: string; packs: { id: string; label: string }[] }) {
  const router = useRouter();
  const [planId, setPlanId] = useState("");
  const [paid, setPaid] = useState<PaymentMethod | "later">("later");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!planId) return setError("Choose a pack.");
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/students/${studentId}/credits/sell`, {
        plan_id: planId,
        paid: paid === "later" ? null : { method: paid },
      });
      setPlanId("");
      setPaid("later");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  if (packs.length === 0) {
    return <p className="muted m-0 text-sm">No packs or drop-ins are offered yet. Add them in Prices and packs.</p>;
  }
  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-2">
      <label htmlFor="sell-pack" className="field-label">
        Add a pack
      </label>
      <div className="flex flex-wrap gap-2">
        <select id="sell-pack" className="select min-w-0 flex-1" style={{ minHeight: 48 }} value={planId} onChange={(e) => setPlanId(e.target.value)}>
          <option value="">Choose…</option>
          {packs.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <label htmlFor="sell-paid" className="sr-only">
          Paying now or later
        </label>
        <select id="sell-paid" className="select" style={{ minHeight: 48 }} value={paid} onChange={(e) => setPaid(e.target.value as PaymentMethod | "later")}>
          <option value="later">Pay later</option>
          {PAYMENT_METHODS.map((m) => (
            <option key={m} value={m}>
              Paid by {m === "upi" ? "UPI" : PAYMENT_METHOD_LABEL[m].toLowerCase()}
            </option>
          ))}
        </select>
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Adding…" : "Add pack"}
        </button>
      </div>
      <p className="muted m-0 text-[13px]">
        {paid === "later" ? "The price goes on their fees as due today." : "The price is recorded as paid and gets a receipt."}
      </p>
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
    </form>
  );
}

/** Give free classes or take credits back (owners and managers), with a reason. */
export function AdjustCredits({ studentId, available }: { studentId: string; available: number }) {
  const router = useRouter();
  const [mode, setMode] = useState<"give" | "take">("give");
  const [classes, setClasses] = useState("1");
  const [note, setNote] = useState("");
  const [expiresOn, setExpiresOn] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setDone(null);
    const n = /^\d+$/.test(classes.trim()) ? Number(classes.trim()) : 0;
    if (n < 1) return setError("Type how many classes.");
    if (!note.trim()) return setError("Say why, for the record.");
    setBusy(true);
    try {
      if (mode === "give") {
        await apiSend("POST", `/students/${studentId}/credits/give`, { classes: n, note: note.trim(), expires_on: expiresOn || null });
        setDone(`${n} free ${n === 1 ? "class" : "classes"} added.`);
      } else {
        await apiSend("POST", `/students/${studentId}/credits/take-back`, { classes: n, note: note.trim() });
        setDone(`${n} ${n === 1 ? "class" : "classes"} taken back.`);
      }
      setNote("");
      setClasses("1");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="disclosure">
      <summary className="flex min-h-[44px] items-center text-sm font-semibold">Give free classes or take some back</summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3 pt-2">
        <div className="segmented self-start" role="group" aria-label="Give or take back">
          <button type="button" aria-pressed={mode === "give"} onClick={() => setMode("give")}>
            Give free classes
          </button>
          <button type="button" aria-pressed={mode === "take"} onClick={() => setMode("take")} disabled={available === 0}>
            Take back
          </button>
        </div>
        <div className={mode === "give" ? "grid grid-cols-[100px_1fr] gap-3" : "grid grid-cols-[100px] gap-3"}>
          <div>
            <label htmlFor="adjust-classes" className="field-label">
              Classes
            </label>
            <input id="adjust-classes" className="field" inputMode="numeric" maxLength={3} value={classes} onChange={(e) => setClasses(e.target.value)} />
          </div>
          {mode === "give" ? (
            <div>
              <label htmlFor="adjust-expiry" className="field-label">
                Use by <span className="muted font-normal">(optional)</span>
              </label>
              <input id="adjust-expiry" type="date" className="field" value={expiresOn} onChange={(e) => setExpiresOn(e.target.value)} />
            </div>
          ) : null}
        </div>
        <div>
          <label htmlFor="adjust-note" className="field-label">
            Why
          </label>
          <input
            id="adjust-note"
            className="field"
            maxLength={200}
            value={note}
            placeholder={mode === "give" ? "Make-up for the cancelled Diwali class" : "Pack sold by mistake"}
            onChange={(e) => setNote(e.target.value)}
          />
        </div>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <p role="status" className="accent-text m-0 font-semibold empty:hidden">
          {done}
        </p>
        <button type="submit" className={mode === "give" ? "btn btn-primary self-start" : "btn btn-dark self-start"} disabled={busy}>
          {busy ? "Saving…" : mode === "give" ? "Give classes" : "Take back"}
        </button>
      </form>
    </details>
  );
}
