"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState, useTransition, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { ATTENDANCE_LABEL, type AttendanceStatus, type Page, type RosterMark, type StudentListItem } from "@/lib/types";

const STATUSES: { value: AttendanceStatus; tone?: "good" | "attn" }[] = [
  { value: "present", tone: "good" },
  { value: "late", tone: "good" },
  { value: "absent", tone: "attn" },
  { value: "excused" },
];

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

function creditNote(r: RosterMark, status: AttendanceStatus | null): { text: string; attention: boolean } | null {
  if (!r.pays_with_credits) return null;
  if (r.owes_credit) return { text: "Came without a credit: owes a class", attention: true };
  const coming = status === "present" || status === "late";
  if (r.credits_left === 0 && !(r.status === "present" || r.status === "late")) {
    return { text: coming ? "No credits left: this class will be owed" : "No credits left", attention: true };
  }
  return { text: `Pays per class, ${r.credits_left} ${r.credits_left === 1 ? "credit" : "credits"} left`, attention: false };
}

/** The roster with four big buttons per student and one Save. */
export function MarkSheet({
  sessionId,
  roster,
  canMark,
  canFindStudents,
}: {
  sessionId: string;
  roster: RosterMark[];
  canMark: boolean;
  canFindStudents: boolean;
}) {
  const router = useRouter();
  // Only what's been tapped and not saved yet. Everything else shows the
  // saved mark, so a refreshed roster (a new drop-in, someone else's save)
  // is never hidden by stale local state.
  const [edits, setEdits] = useState<Record<string, AttendanceStatus>>({});
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // The refresh runs as a transition so the taps are cleared in the same
  // paint as the saved marks arrive. Clearing them earlier would flash the
  // whole class back to "not marked" for a moment.
  const [refreshing, startRefresh] = useTransition();
  const busy = saving || refreshing;

  const statusOf = (r: RosterMark): AttendanceStatus | null => edits[r.student_id] ?? r.status;
  const changed = roster.filter((r) => edits[r.student_id] !== undefined && edits[r.student_id] !== r.status);
  const unmarked = roster.filter((r) => statusOf(r) === null).length;
  const came = roster.filter((r) => statusOf(r) === "present" || statusOf(r) === "late").length;

  function set(studentId: string, status: AttendanceStatus) {
    setDone(null);
    setEdits({ ...edits, [studentId]: status });
  }

  async function save() {
    setError(null);
    setSaving(true);
    try {
      await apiSend("PUT", `/sessions/${sessionId}/attendance`, {
        marks: changed.map((r) => ({ student_id: r.student_id, status: edits[r.student_id] })),
      });
      startRefresh(() => {
        router.refresh();
        setEdits({});
        setDone("Saved.");
      });
    } catch (err) {
      setError(message(err));
    } finally {
      setSaving(false);
    }
  }

  async function removeDropIn(studentId: string) {
    setError(null);
    try {
      await apiSend("DELETE", `/sessions/${sessionId}/drop-ins/${studentId}`);
      router.refresh();
    } catch (err) {
      setError(message(err));
    }
  }

  return (
    <section aria-labelledby="roster-title" className="panel flex flex-col gap-3 p-4 sm:p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="roster-title" className="h-section">
          {roster.length === 0 ? "Nobody in this class yet" : `${came} of ${roster.length} came`}
        </h2>
        {canMark && unmarked > 0 ? (
          <button
            type="button"
            className="btn btn-dark"
            onClick={() => {
              setDone(null);
              setEdits(Object.fromEntries(roster.map((r) => [r.student_id, statusOf(r) ?? "present"])));
            }}
          >
            {unmarked === roster.length ? "Everyone's here" : `Mark the other ${unmarked} present`}
          </button>
        ) : null}
      </div>

      <ul className="m-0 flex list-none flex-col gap-1.5 p-0" data-live>
        {roster.map((r) => {
          const status = statusOf(r);
          const note = creditNote(r, status);
          return (
            <li key={r.student_id} className="row flex flex-col gap-2 p-3 sm:flex-row sm:items-center sm:justify-between">
              <span className="min-w-0">
                <span className="block font-bold">
                  {r.student_name}
                  {r.drop_in ? <span className="tag tag-quiet ml-2">Drop-in</span> : null}
                </span>
                <span className="flex flex-wrap gap-x-3 text-[13px]">
                  {note ? <span className={note.attention ? "attn-text font-semibold" : "muted"}>{note.text}</span> : null}
                  {r.absences_in_a_row >= 2 ? (
                    <span className="attn-text font-semibold">Missed the last {r.absences_in_a_row}</span>
                  ) : null}
                </span>
              </span>
              <span className="flex items-center gap-2">
                <span className="marks min-w-0 flex-1 sm:w-[340px] sm:flex-none" role="group" aria-label={`Mark ${r.student_name}`}>
                  {STATUSES.map((s) => (
                    <button
                      key={s.value}
                      type="button"
                      data-tone={s.tone}
                      aria-pressed={status === s.value}
                      disabled={!canMark || busy}
                      onClick={() => set(r.student_id, s.value)}
                    >
                      {ATTENDANCE_LABEL[s.value]}
                    </button>
                  ))}
                </span>
                {r.drop_in && canMark ? (
                  <button type="button" className="btn btn-quiet" onClick={() => removeDropIn(r.student_id)} aria-label={`Take ${r.student_name} off this class`}>
                    Remove
                  </button>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>

      {canMark && canFindStudents ? <AddDropIn sessionId={sessionId} onList={roster.map((r) => r.student_id)} /> : null}

      {canMark ? (
        <div className="save-bar row flex flex-wrap items-center justify-between gap-3 p-3">
          <span className="min-w-0 text-sm font-semibold" role="status" aria-live="polite">
            {error ? <span className="attn-text">{error}</span> : changed.length > 0 ? `${changed.length} to save` : (done ?? (unmarked > 0 ? `${unmarked} not marked yet` : "All marked"))}
          </span>
          <button type="button" className="btn btn-primary" disabled={busy || changed.length === 0} onClick={save}>
            {busy ? "Saving…" : "Save attendance"}
          </button>
        </div>
      ) : null}
    </section>
  );
}

/** Add someone who isn't in the class, for this one meeting. */
function AddDropIn({ sessionId, onList }: { sessionId: string; onList: string[] }) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<StudentListItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const q = query.trim();
    if (!open || q.length < 2) return;
    let stale = false;
    const timer = setTimeout(async () => {
      try {
        const params = new URLSearchParams({ q: q.slice(0, 80), limit: "8" });
        const page = await apiSend<Page<StudentListItem>>("GET", `/students?${params.toString()}`);
        if (!stale) setResults(page.items.filter((s) => s.status !== "left"));
      } catch {
        if (!stale) setResults([]);
      }
    }, 250);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [query, open]);

  if (!open) {
    return (
      <button type="button" className="btn btn-ghost self-start" onClick={() => setOpen(true)}>
        Add a drop-in
      </button>
    );
  }

  async function add(studentId: string) {
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/sessions/${sessionId}/drop-ins`, { student_id: studentId });
      setQuery("");
      setResults([]);
      setOpen(false);
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  const shown = query.trim().length >= 2 ? results : [];
  return (
    <div className="row flex flex-col gap-2 p-4">
      <label htmlFor="drop-in-search" className="field-label">
        Who dropped in?
      </label>
      <input
        id="drop-in-search"
        className="field"
        type="search"
        value={query}
        maxLength={80}
        placeholder="Name or phone number"
        autoComplete="off"
        onChange={(e) => setQuery(e.target.value)}
      />
      <p className="muted m-0 text-[13px]">They&apos;re marked present and one class credit is used.</p>
      {shown.length > 0 ? (
        <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Matching students">
          {shown.map((s) => (
            <li key={s.id}>
              <button type="button" className="btn btn-ghost w-full justify-between" disabled={busy || onList.includes(s.id)} onClick={() => add(s.id)}>
                <span>{s.name}</span>
                <span className="muted text-[13px] font-medium">{onList.includes(s.id) ? "Already on the list" : (s.branch_name ?? "")}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : query.trim().length >= 2 ? (
        <p className="muted m-0 text-sm">No student matches yet. New people are added under Students first.</p>
      ) : null}
      {error ? (
        <p role="alert" className="field-error m-0">
          {error}
        </p>
      ) : null}
      <button type="button" className="btn btn-quiet self-start" onClick={() => setOpen(false)}>
        Cancel
      </button>
    </div>
  );
}

/** Was the teacher there, and who taught. */
export function TeacherCheckIn({
  sessionId,
  instructorId,
  instructorName,
  present,
  canMarkAny,
  team,
}: {
  sessionId: string;
  instructorId: string | null;
  instructorName: string | null;
  present: boolean | null;
  canMarkAny: boolean;
  team: { id: string; name: string }[];
}) {
  const router = useRouter();
  const [who, setWho] = useState(instructorId ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function mark(isPresent: boolean) {
    setError(null);
    setBusy(true);
    try {
      await apiSend("PUT", `/sessions/${sessionId}/teacher`, { present: isPresent, instructor_id: who || null });
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 p-1.5">
      <span className="min-w-0">
        <span className="muted block text-[13px]">Teacher</span>
        {canMarkAny && team.length > 0 ? (
          <>
            <label htmlFor="session-teacher" className="sr-only">
              Who taught
            </label>
            <select id="session-teacher" className="select" value={who} onChange={(e) => setWho(e.target.value)}>
              <option value="">Not decided</option>
              {team.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </>
        ) : (
          <span className="font-bold">{instructorName ?? "Not decided"}</span>
        )}
      </span>
      <span className="flex flex-wrap items-center gap-2">
        {present === null ? null : <span className={present ? "tag tag-good" : "tag tag-attn"}>{present ? "Was here" : "Was absent"}</span>}
        <button type="button" className={present === true ? "btn btn-ghost" : "btn btn-primary"} disabled={busy} onClick={() => mark(true)}>
          {canMarkAny ? "Teacher was here" : "I'm here"}
        </button>
        {canMarkAny ? (
          <button type="button" className="btn btn-ghost" disabled={busy} onClick={() => mark(false)}>
            Teacher was absent
          </button>
        ) : null}
      </span>
      {error ? (
        <p role="alert" className="field-error m-0 w-full">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/** Cancel this one meeting (with a reason), or bring it back. */
export function CancelClass({ sessionId, cancelled, name }: { sessionId: string; cancelled: boolean; name: string }) {
  const router = useRouter();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function send(path: "cancel" | "restore", body: unknown) {
    setError(null);
    setBusy(true);
    try {
      await apiSend("POST", `/sessions/${sessionId}/${path}`, body);
      setReason("");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  if (cancelled) {
    return (
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" className="btn btn-dark" disabled={busy} onClick={() => send("restore", {})}>
          Bring this class back
        </button>
        {error ? (
          <span role="alert" className="field-error m-0">
            {error}
          </span>
        ) : null}
      </div>
    );
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    if (!reason.trim()) return setError("Say why, for the record.");
    void send("cancel", { reason: reason.trim() });
  }

  return (
    <details className="panel disclosure">
      <summary className="flex min-h-[56px] items-center px-6 font-semibold">This class didn&apos;t happen</summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3 px-6 pb-6">
        <p className="soft m-0 text-sm">Cancelling clears today&apos;s marks for {name} and gives back any class credits used.</p>
        <div>
          <label htmlFor="cancel-reason" className="field-label">
            Why
          </label>
          <input id="cancel-reason" className="field" maxLength={200} value={reason} placeholder="Teacher unwell" onChange={(e) => setReason(e.target.value)} />
        </div>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <button type="submit" className="btn btn-dark self-start" disabled={busy}>
          {busy ? "Cancelling…" : "Cancel this class"}
        </button>
      </form>
    </details>
  );
}
