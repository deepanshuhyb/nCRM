"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState, useTransition, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { addDays, formatRupees, parseRupees } from "@/lib/format";
import { PAYMENT_METHOD_LABEL, PAYMENT_METHODS, type PaymentMethod, type Receipt } from "@/lib/types";

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

/** Record money received. Says what will happen to it before saving. */
export function TakePaymentForm({ studentId, duePaise, today }: { studentId: string; duePaise: number; today: string }) {
  const router = useRouter();
  // null means "not typed yet": the field then shows whatever is due, and
  // keeps following it when the page refreshes (a charge added, a payment
  // saved). State holds only what the person typed; see docs/NOTES.md.
  const [typed, setTyped] = useState<string | null>(null);
  const amount = typed ?? (duePaise > 0 ? String(duePaise / 100) : "");
  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [reference, setReference] = useState("");
  const [receivedOn, setReceivedOn] = useState(today);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [refreshing, startRefresh] = useTransition();
  const busy = saving || refreshing;
  const [done, setDone] = useState<Receipt | null>(null);

  const paise = parseRupees(amount);
  const hint =
    paise === null || paise === 0
      ? null
      : duePaise === 0
        ? `Nothing is due, so ${formatRupees(paise)} will be kept as advance.`
        : paise > duePaise
          ? `${formatRupees(paise - duePaise)} more than is due. The extra is kept as advance.`
          : paise < duePaise
            ? `${formatRupees(duePaise - paise)} will still be due.`
            : "Clears everything due.";

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (paise === null || paise === 0) return setError("Enter the amount received, like 1500.");
    setError(null);
    setSaving(true);
    try {
      const receipt = await apiSend<Receipt>("POST", `/students/${studentId}/payments`, {
        amount_paise: paise,
        method,
        reference: reference.trim() || null,
        received_on: receivedOn,
      });
      // One paint: the new balance arrives and the form goes back to
      // "whatever is still due" together.
      startRefresh(() => {
        router.refresh();
        setDone(receipt);
        setTyped(null);
        setReference("");
        setReceivedOn(today);
      });
    } catch (err) {
      setError(message(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="row flex flex-col gap-3 p-4">
      <h3 className="m-0 text-base font-bold">Take a payment</h3>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div>
          <label htmlFor="pay-amount" className="field-label">
            Amount received (₹)
          </label>
          <input
            id="pay-amount"
            className="field"
            inputMode="decimal"
            autoComplete="off"
            value={amount}
            placeholder="1500"
            aria-describedby="pay-hint"
            onChange={(e) => {
              setDone(null);
              setTyped(e.target.value);
            }}
          />
        </div>
        <div>
          <label htmlFor="pay-method" className="field-label">
            How
          </label>
          <select id="pay-method" className="select w-full" value={method} onChange={(e) => setMethod(e.target.value as PaymentMethod)}>
            {PAYMENT_METHODS.map((m) => (
              <option key={m} value={m}>
                {PAYMENT_METHOD_LABEL[m]}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label htmlFor="pay-ref" className="field-label">
            Reference <span className="muted font-normal">(optional)</span>
          </label>
          <input
            id="pay-ref"
            className="field"
            maxLength={80}
            autoComplete="off"
            value={reference}
            placeholder={method === "cheque" ? "Cheque number" : "UPI or bank reference"}
            onChange={(e) => setReference(e.target.value)}
          />
        </div>
        <div>
          <label htmlFor="pay-date" className="field-label">
            Received on
          </label>
          <input
            id="pay-date"
            className="field"
            type="date"
            value={receivedOn}
            max={today}
            min={addDays(today, -60)}
            onChange={(e) => setReceivedOn(e.target.value || today)}
          />
        </div>
      </div>
      <p id="pay-hint" className="soft m-0 min-h-[1.2em] text-sm">
        {hint}
        {receivedOn < today ? " Dated in the past." : ""}
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Saving…" : paise ? `Record ${formatRupees(paise)}` : "Record payment"}
        </button>
        <span role="status" aria-live="polite" className="text-sm font-semibold">
          {done ? (
            <>
              Saved. <Link href={`/fees/receipts/${done.payment.id}`}>Receipt {done.payment.receipt_no}</Link>
            </>
          ) : null}
        </span>
      </div>
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
    </form>
  );
}

/** Add a charge by hand: registration, a costume, an old balance. */
export function AddChargeForm({ studentId, today }: { studentId: string; today: string }) {
  const router = useRouter();
  const [description, setDescription] = useState("");
  const [amount, setAmount] = useState("");
  const [dueOn, setDueOn] = useState(today);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    const paise = parseRupees(amount);
    if (!description.trim()) return setError("Say what it's for.");
    if (paise === null || paise === 0) return setError("Enter the amount, like 1500.");
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/students/${studentId}/charges`, { description: description.trim(), amount_paise: paise, due_on: dueOn });
      setDescription("");
      setAmount("");
      setDueOn(today);
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="disclosure">
      <summary className="flex min-h-[44px] items-center text-sm font-semibold">Add a charge</summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3 pt-2">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div>
            <label htmlFor="charge-what" className="field-label">
              What it&apos;s for
            </label>
            <input
              id="charge-what"
              className="field"
              maxLength={120}
              value={description}
              placeholder="Costume for the annual show"
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>
          <div>
            <label htmlFor="charge-amount" className="field-label">
              Amount (₹)
            </label>
            <input id="charge-amount" className="field" inputMode="decimal" autoComplete="off" value={amount} placeholder="1500" onChange={(e) => setAmount(e.target.value)} />
          </div>
          <div>
            <label htmlFor="charge-due" className="field-label">
              Due on
            </label>
            <input
              id="charge-due"
              className="field"
              type="date"
              value={dueOn}
              min={addDays(today, -365)}
              max={addDays(today, 365)}
              onChange={(e) => setDueOn(e.target.value || today)}
            />
          </div>
        </div>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <button type="submit" className="btn btn-dark self-start" disabled={busy}>
          {busy ? "Adding…" : "Add charge"}
        </button>
      </form>
    </details>
  );
}

/** Cancel a charge or a payment, with a reason (owners and managers). */
export function CancelEntry({ kind, id, label }: { kind: "charge" | "payment"; id: string; label: string }) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = `cancel-${kind}-${id}`;

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!reason.trim()) return setError("Say why, for the record.");
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/${kind === "charge" ? "charges" : "payments"}/${id}/void`, { reason: reason.trim() });
      setOpen(false);
      setReason("");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  if (!open) {
    return (
      <button type="button" className="btn btn-quiet" onClick={() => setOpen(true)} aria-label={`Cancel ${label}`}>
        Cancel
      </button>
    );
  }
  return (
    <form onSubmit={submit} noValidate className="flex w-full flex-col gap-2 pt-1">
      <label htmlFor={field} className="field-label">
        Why cancel {label}?
      </label>
      <input
        id={field}
        className="field"
        maxLength={200}
        value={reason}
        placeholder={kind === "charge" ? "Sibling discount" : "Entered twice"}
        onChange={(e) => setReason(e.target.value)}
      />
      <p className="muted m-0 text-[13px]">
        {kind === "charge"
          ? "Money already paid towards it moves to their other dues, or is kept as advance."
          : "It stays on the record as cancelled. What it had paid becomes due again."}
      </p>
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <div className="flex gap-2">
        <button type="submit" className="btn btn-dark" disabled={busy}>
          {busy ? "Cancelling…" : `Cancel this ${kind}`}
        </button>
        <button type="button" className="btn btn-quiet" onClick={() => setOpen(false)}>
          Keep it
        </button>
      </div>
    </form>
  );
}
