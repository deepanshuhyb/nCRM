"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState, type FormEvent } from "react";

import { BackdatableDate } from "@/components/backdatable-date";
import { ApiError, apiSend } from "@/lib/api-client";
import { addDays } from "@/lib/format";
import type { Page, StudentListItem } from "@/lib/types";

/** Find a student by name or phone digits and put them in this class. */
export function AddToClass({
  batchId,
  inClass,
  plans,
  today,
}: {
  batchId: string;
  inClass: string[];
  plans: { id: string; label: string }[];
  today: string;
}) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<StudentListItem[]>([]);
  const [picked, setPicked] = useState<StudentListItem | null>(null);
  const [planId, setPlanId] = useState(plans.length === 1 ? plans[0].id : "");
  const [startsOn, setStartsOn] = useState(today);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Search as they type, after a short pause, and ignore stale answers.
  useEffect(() => {
    const q = query.trim();
    if (!open || picked || q.length < 2) return;
    let stale = false;
    const timer = setTimeout(async () => {
      try {
        const params = new URLSearchParams({ q: q.slice(0, 80), status: "active", limit: "8" });
        const page = await apiSend<Page<StudentListItem>>("GET", `/students?${params.toString()}`);
        if (!stale) setResults(page.items);
      } catch {
        if (!stale) setResults([]);
      }
    }, 250);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [query, open, picked]);

  if (!open) {
    return (
      <button type="button" className="btn btn-ghost self-start" onClick={() => setOpen(true)}>
        Add a student
      </button>
    );
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!picked) return setError("Choose a student first.");
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/students/${picked.id}/enrollments`, {
        batch_id: batchId,
        plan_id: planId || null,
        starts_on: startsOn || null,
      });
      setPicked(null);
      setQuery("");
      setResults([]);
      setOpen(false);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  const shown = query.trim().length >= 2 ? results : [];

  return (
    <form onSubmit={submit} noValidate className="row flex flex-col gap-3 p-4">
      {picked ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span>
            <span className="muted block text-[13px]">Adding</span>
            <span className="font-bold">{picked.name}</span>
          </span>
          <button type="button" className="btn btn-quiet" onClick={() => setPicked(null)}>
            Choose someone else
          </button>
        </div>
      ) : (
        <div>
          <label htmlFor="roster-search" className="field-label">
            Find a student
          </label>
          <input
            id="roster-search"
            className="field"
            type="search"
            value={query}
            maxLength={80}
            placeholder="Name or phone number"
            autoComplete="off"
            onChange={(e) => setQuery(e.target.value)}
            aria-describedby="roster-search-help"
          />
          <p id="roster-search-help" className="muted m-0 mt-1 text-[13px]">
            Active students only.
          </p>
          {shown.length > 0 ? (
            <ul className="m-0 mt-2 flex list-none flex-col gap-1 p-0" aria-label="Matching students">
              {shown.map((s) => {
                const already = inClass.includes(s.id);
                return (
                  <li key={s.id}>
                    <button
                      type="button"
                      className="btn btn-ghost w-full justify-between"
                      disabled={already}
                      onClick={() => setPicked(s)}
                    >
                      <span>{s.name}</span>
                      <span className="muted text-[13px] font-medium">{already ? "Already in" : s.branch_name ?? ""}</span>
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : query.trim().length >= 2 ? (
            <p className="muted m-0 mt-2 text-sm">No active student matches yet.</p>
          ) : null}
        </div>
      )}

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1.4fr_1fr]">
        <div>
          <label htmlFor="roster-plan" className="field-label">
            How they pay
          </label>
          <select id="roster-plan" className="select w-full" style={{ minHeight: 52 }} value={planId} onChange={(e) => setPlanId(e.target.value)}>
            <option value="">With class credits</option>
            {plans.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </div>
        <BackdatableDate
          id="roster-start"
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
        <button type="submit" className="btn btn-primary" disabled={busy || !picked}>
          {busy ? "Adding…" : "Add to class"}
        </button>
        <button type="button" className="btn btn-quiet" onClick={() => setOpen(false)}>
          Cancel
        </button>
      </div>
    </form>
  );
}
