"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

function monthLabel(iso: string): string {
  return `${MONTHS[Number(iso.slice(5, 7)) - 1]} ${iso.slice(0, 4)}`;
}

/** The first day of the month `n` months after the one `today` is in. */
function monthStart(today: string, n: number): string {
  const year = Number(today.slice(0, 4));
  const month = Number(today.slice(5, 7)) - 1 + n;
  const d = new Date(Date.UTC(year, month, 1));
  return d.toISOString().slice(0, 10);
}

function ordinal(n: number): string {
  const tens = n % 100;
  if (tens >= 11 && tens <= 13) return `${n}th`;
  return `${n}${["th", "st", "nd", "rd"][n % 10] ?? "th"}`;
}

/**
 * Switch monthly fees on (from a month), change the due day, or stop.
 * Until this is on, nothing is charged automatically.
 */
export function MonthlyFeesSetup({ feesFrom, dueDay, today }: { feesFrom: string | null; dueDay: number; today: string }) {
  const router = useRouter();
  const thisMonth = monthStart(today, 0);
  const choices = [0, 1, 2].map((n) => monthStart(today, n));
  // A start month already in the past stays selectable as itself.
  const options = feesFrom && !choices.includes(feesFrom) ? [feesFrom, ...choices] : choices;
  const [from, setFrom] = useState(feesFrom ?? thisMonth);
  const [day, setDay] = useState(String(dueDay));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save(next: { fees_from: string | null }) {
    setError(null);
    setBusy(true);
    try {
      await apiSend("PUT", "/fees/settings", { fees_from: next.fees_from, fee_due_day: Number(day) });
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    void save({ fees_from: from });
  }

  const fields = (
    <div className="flex flex-wrap items-end gap-3">
      <div>
        <label htmlFor="fees-from" className="field-label">
          Starting with
        </label>
        <select id="fees-from" className="select" value={from} onChange={(e) => setFrom(e.target.value)}>
          {options.map((m) => (
            <option key={m} value={m}>
              {monthLabel(m)}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label htmlFor="fees-day" className="field-label">
          Due on day
        </label>
        <select id="fees-day" className="select" value={day} onChange={(e) => setDay(e.target.value)}>
          {Array.from({ length: 28 }, (_, i) => i + 1).map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
      </div>
      <button type="submit" className="btn btn-primary" disabled={busy}>
        {busy ? "Saving…" : feesFrom ? "Save" : "Start monthly fees"}
      </button>
    </div>
  );

  if (feesFrom === null) {
    return (
      <section aria-labelledby="monthly-title" className="panel flex flex-col gap-3 p-6">
        <h2 id="monthly-title" className="h-section">
          Monthly fees aren&apos;t being created yet
        </h2>
        <p className="soft m-0 max-w-[68ch]">
          Once you start, every student in a class on a monthly plan gets that month&apos;s fee on the 1st, and new joiners get theirs
          the day they join. Students on a break aren&apos;t charged. Packs and charges you add by hand work either way.
        </p>
        <form onSubmit={submit} noValidate className="flex flex-col gap-2">
          {fields}
          <p role="alert" className="field-error m-0 min-h-[1.2em]">
            {error}
          </p>
        </form>
      </section>
    );
  }

  return (
    <details className="panel disclosure">
      <summary className="flex min-h-[56px] items-center px-6 font-semibold">
        Monthly fees are created on the 1st and due on the {ordinal(dueDay)}
        {feesFrom > thisMonth ? `, starting ${monthLabel(feesFrom)}` : ""}
      </summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3 px-6 pb-6">
        {fields}
        <p className="soft m-0 text-sm">A new due day applies to fees created from now on.</p>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <button type="button" className="btn btn-quiet self-start px-0" disabled={busy} onClick={() => save({ fees_from: null })}>
          Stop creating monthly fees
        </button>
      </form>
    </details>
  );
}

/**
 * WhatsApp fee reminders: on or off for the whole studio. The schedule
 * itself is fixed (3 days before, on the day, 3 and 7 days after) so
 * that every family is treated the same and nobody has to tune it.
 */
export function FeeReminders({ on, feesFrom, dueDay, canChange }: { on: boolean; feesFrom: string | null; dueDay: number; canChange: boolean }) {
  const router = useRouter();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function set(next: boolean) {
    setError(null);
    setBusy(true);
    try {
      // The other two settings are sent back as they are: this endpoint
      // saves all three together.
      await apiSend("PUT", "/fees/settings", { fees_from: feesFrom, fee_due_day: dueDay, fee_reminders: next });
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="panel disclosure">
      <summary className="flex min-h-[56px] items-center gap-2 px-6 font-semibold">
        WhatsApp reminders
        <span className={on ? "tag tag-good" : "tag tag-quiet"}>{on ? "On" : "Off"}</span>
      </summary>
      <div className="flex flex-col gap-3 px-6 pb-6">
        <p className="soft m-0 max-w-[68ch] text-sm">
          {on ? "The" : "When on, the"} student&apos;s main contact gets a WhatsApp message 3 days before a fee is due, on the day, and 3 and 7 days after. One
          message covers everything a family owes. It stops when the fee is paid, and nobody who has left is reminded.
        </p>
        {canChange ? (
          <button type="button" className={`btn self-start ${on ? "btn-ghost" : "btn-primary"}`} disabled={busy} onClick={() => set(!on)}>
            {busy ? "Saving…" : on ? "Turn reminders off" : "Turn reminders on"}
          </button>
        ) : (
          <p className="caption m-0">The owner or a manager can change this.</p>
        )}
        <p role="alert" className="field-error m-0 empty:hidden">
          {error}
        </p>
      </div>
    </details>
  );
}
