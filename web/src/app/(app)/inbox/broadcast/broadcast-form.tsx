"use client";

import { useRouter } from "next/navigation";
import { useEffect, useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import type { Audience, Broadcast, BroadcastPreview, Page, StudentListItem } from "@/lib/types";

const MAX_TEXT = 600;

type Kind = Audience["kind"];
const KINDS: { value: Kind; label: string }[] = [
  // Short enough that all five fit across a phone; the line under them
  // says what each means.
  { value: "everyone", label: "All" },
  { value: "branch", label: "Branch" },
  { value: "batch", label: "Class" },
  { value: "owing", label: "Owing" },
  { value: "students", label: "Chosen" },
];

type Option = { id: string; name: string; where?: string };

/**
 * Write a message, choose who it is for, see exactly how many people
 * that is, then send. Two steps on purpose: a group message costs money
 * and can't be taken back.
 */
export function BroadcastForm({ branches, classes, live, connected }: { branches: Option[]; classes: Option[]; live: boolean; connected: boolean }) {
  const router = useRouter();
  const [kind, setKind] = useState<Kind>("everyone");
  const [branchId, setBranchId] = useState("");
  const [batchId, setBatchId] = useState("");
  const [picked, setPicked] = useState<{ id: string; name: string }[]>([]);
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<BroadcastPreview | null>(null);
  const [done, setDone] = useState<Broadcast | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // What was checked is only true for what was checked: any change to
  // the group or the words means checking again.
  function changed() {
    setPreview(null);
    setDone(null);
    setError(null);
  }

  function audience(): Audience | string {
    if (kind === "branch") return branchId ? { kind, branch_id: branchId } : "Choose the branch.";
    if (kind === "batch") return batchId ? { kind, batch_id: batchId } : "Choose the class.";
    if (kind === "students") return picked.length > 0 ? { kind, student_ids: picked.map((p) => p.id) } : "Choose at least one student.";
    return { kind };
  }

  async function check(e: FormEvent) {
    e.preventDefault();
    setDone(null);
    const who = audience();
    if (typeof who === "string") return setError(who);
    if (!text.trim()) {
      document.getElementById("broadcast-text")?.focus();
      return setError("Write the message first.");
    }
    setError(null);
    setBusy(true);
    try {
      setPreview(await apiSend<BroadcastPreview>("POST", "/broadcasts/preview", who));
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Couldn't check that. Try again.");
    } finally {
      setBusy(false);
    }
  }

  async function send() {
    const who = audience();
    if (typeof who === "string") return setError(who);
    setError(null);
    setBusy(true);
    try {
      const sent = await apiSend<Broadcast>("POST", "/broadcasts", { audience: who, text });
      setDone(sent);
      setPreview(null);
      setText("");
      setPicked([]);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Couldn't send that. Try again.");
    } finally {
      setBusy(false);
    }
  }

  const left = MAX_TEXT - text.length;

  return (
    <form onSubmit={check} noValidate className="panel flex flex-col gap-4 p-4 sm:p-6">
      <div className="flex flex-col gap-2">
        <span id="audience-label" className="field-label">
          Who is it for
        </span>
        <div className="segmented self-start" role="group" aria-labelledby="audience-label">
          {KINDS.map((k) => (
            <button
              key={k.value}
              type="button"
              aria-pressed={kind === k.value}
              onClick={() => {
                setKind(k.value);
                changed();
              }}
            >
              {k.label}
            </button>
          ))}
        </div>
        {kind === "branch" ? (
          <select
            aria-label="Branch"
            className="select w-full sm:w-auto sm:self-start"
            value={branchId}
            onChange={(e) => {
              setBranchId(e.target.value);
              changed();
            }}
          >
            <option value="">Choose a branch…</option>
            {branches.map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        ) : null}
        {kind === "batch" ? (
          <select
            aria-label="Class"
            className="select w-full sm:w-auto sm:self-start"
            value={batchId}
            onChange={(e) => {
              setBatchId(e.target.value);
              changed();
            }}
          >
            <option value="">Choose a class…</option>
            {classes.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
                {c.where ? `, ${c.where}` : ""}
              </option>
            ))}
          </select>
        ) : null}
        {kind === "owing" ? <p className="caption m-0">Students with anything left to pay, whether or not it is late.</p> : null}
        {kind === "everyone" ? <p className="caption m-0">Every student who hasn&apos;t left, across all branches.</p> : null}
        {kind === "students" ? (
          <StudentPicker
            picked={picked}
            onChange={(next) => {
              setPicked(next);
              changed();
            }}
          />
        ) : null}
      </div>

      <div>
        <label htmlFor="broadcast-text" className="field-label">
          Message
        </label>
        <textarea
          id="broadcast-text"
          className="field min-h-[104px]"
          maxLength={MAX_TEXT}
          value={text}
          placeholder="No class tomorrow, the hall is being painted. We're back on Thursday."
          onChange={(e) => {
            setText(e.target.value);
            changed();
          }}
        />
        <p className="caption m-0 mt-1 flex flex-wrap justify-between gap-x-3">
          <span>One paragraph: WhatsApp doesn&apos;t allow line breaks in a group message. It arrives as &quot;Hi (their name), a message from (your studio): …&quot;</span>
          <span className={`tabular-nums ${left < 40 ? "attn-text font-semibold" : ""}`}>{left} left</span>
        </p>
      </div>

      <p role="alert" className="field-error m-0 empty:hidden">
        {error}
      </p>

      {done ? (
        <p role="status" className="panel m-0 p-4 text-sm font-semibold">
          Sent to {done.recipients} {done.recipients === 1 ? "person" : "people"}. How many arrive shows under &quot;Sent before&quot;.
        </p>
      ) : null}

      {preview ? (
        <div className="row flex flex-col gap-3 p-4" role="status">
          {preview.recipients === 0 ? (
            <p className="m-0 font-semibold">Nobody in that group has a contact to message.</p>
          ) : (
            <>
              <p className="m-0 font-semibold">
                This goes to {preview.recipients} {preview.recipients === 1 ? "person" : "people"}
                {preview.sample.length > 0 ? `: ${preview.sample.join(", ")}${preview.recipients > preview.sample.length ? " and others" : ""}.` : "."}
              </p>
              {preview.without_contact > 0 ? (
                <p className="attn-text m-0 text-sm font-semibold">
                  {preview.without_contact} {preview.without_contact === 1 ? "student has" : "students have"} no main contact and will be missed.
                </p>
              ) : null}
              <p className="soft m-0 text-sm">
                {!live
                  ? "Test mode: WhatsApp isn't connected on this server, so it will be saved and shown but reach nobody."
                  : !connected
                    ? "This studio has no WhatsApp number yet, so it can't go out."
                    : "WhatsApp charges for each message, and a message can't be taken back once sent."}
              </p>
              <div className="flex flex-wrap items-center gap-2">
                <button type="button" className="btn btn-primary" disabled={busy} onClick={send}>
                  {busy ? "Sending…" : `Send to ${preview.recipients} ${preview.recipients === 1 ? "person" : "people"}`}
                </button>
                <button type="button" className="btn btn-quiet" disabled={busy} onClick={() => setPreview(null)}>
                  Not yet
                </button>
              </div>
            </>
          )}
        </div>
      ) : (
        <button type="submit" className="btn btn-dark self-start" disabled={busy}>
          {busy ? "Checking…" : "Check who gets it"}
        </button>
      )}
    </form>
  );
}

/** Find students by name and add them one at a time. */
function StudentPicker({ picked, onChange }: { picked: { id: string; name: string }[]; onChange: (next: { id: string; name: string }[]) => void }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<StudentListItem[]>([]);

  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) return;
    let stale = false;
    const timer = setTimeout(async () => {
      try {
        const params = new URLSearchParams({ q: q.slice(0, 80), limit: "8" });
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
  }, [query]);

  const shown = query.trim().length >= 2 ? results.filter((s) => !picked.some((p) => p.id === s.id)) : [];
  return (
    <div className="flex flex-col gap-2">
      {picked.length > 0 ? (
        <ul className="m-0 flex list-none flex-wrap gap-1.5 p-0" aria-label="Chosen students">
          {picked.map((p) => (
            <li key={p.id}>
              <button type="button" className="btn btn-ghost min-h-[40px] gap-2 px-3 text-sm" onClick={() => onChange(picked.filter((x) => x.id !== p.id))} aria-label={`Remove ${p.name}`}>
                {p.name}
                <span aria-hidden="true">×</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      <label htmlFor="pick-student" className="sr-only">
        Find a student
      </label>
      <input id="pick-student" className="field" value={query} placeholder="Type a student's name to add them" onChange={(e) => setQuery(e.target.value)} autoComplete="off" />
      {shown.length > 0 ? (
        <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Matching students">
          {shown.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                className="btn btn-ghost w-full justify-between"
                onClick={() => {
                  onChange([...picked, { id: s.id, name: s.name }]);
                  setQuery("");
                  setResults([]);
                }}
              >
                <span>{s.name}</span>
                <span className="muted text-[13px] font-medium">{s.primary_contact_name ?? "No contact"}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : query.trim().length >= 2 ? (
        <p className="caption m-0">No student matches that.</p>
      ) : null}
    </div>
  );
}
