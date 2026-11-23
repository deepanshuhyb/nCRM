"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useRef, useState, type DragEvent } from "react";

import { BackdatedPill } from "@/components/backdated-pill";
import { ApiError, apiUpload } from "@/lib/api-client";
import { formatDate, formatPhone } from "@/lib/format";
import { RELATION_LABEL, type ImportOutcome, type ImportPreview, type ImportResult } from "@/lib/types";

const OUTCOME: Record<ImportOutcome, { label: string; tag: string }> = {
  ready: { label: "Will be added", tag: "tag tag-good" },
  already_here: { label: "Already here", tag: "tag tag-quiet" },
  repeated: { label: "Repeated in file", tag: "tag tag-quiet" },
  cannot_import: { label: "Can't be added", tag: "tag tag-attn" },
};

const MAX_BYTES = 2 * 1024 * 1024;

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

export function ImportFlow({ branches, today }: { branches: { id: string; name: string }[]; today: string }) {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);
  const [branchId, setBranchId] = useState(branches.length === 1 ? branches[0].id : "");
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"reading" | "adding" | null>(null);
  const [dragging, setDragging] = useState(false);

  const query = (b: string) => (b ? `?branch_id=${encodeURIComponent(b)}` : "");

  async function read(next: File, branch: string) {
    setError(null);
    setPreview(null);
    setResult(null);
    if (next.size > MAX_BYTES) {
      setFile(null);
      return setError("That file is larger than 2 MB. Split it into smaller files.");
    }
    setFile(next);
    setBusy("reading");
    try {
      setPreview(await apiUpload<ImportPreview>(`/students/import/preview${query(branch)}`, next));
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(null);
    }
  }

  async function add() {
    if (!file) return;
    setError(null);
    setBusy("adding");
    try {
      setResult(await apiUpload<ImportResult>(`/students/import${query(branchId)}`, file));
      setPreview(null);
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(null);
    }
  }

  function reset() {
    setFile(null);
    setPreview(null);
    setResult(null);
    setError(null);
    if (input.current) input.current.value = "";
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragging(false);
    const dropped = e.dataTransfer.files[0];
    if (dropped) void read(dropped, branchId);
  }

  if (result) {
    return (
      <section className="panel flex flex-col gap-3 p-6" aria-live="polite">
        <h2 className="h-section">
          {result.added} {result.added === 1 ? "student" : "students"} added
        </h2>
        <p className="soft m-0">
          {result.skipped === 0 ? "Every row was added." : `${result.skipped} ${result.skipped === 1 ? "row was" : "rows were"} skipped, as the preview showed.`}
        </p>
        <div className="flex flex-wrap gap-2">
          <Link href="/students" className="btn btn-primary">
            See students
          </Link>
          <button type="button" className="btn btn-ghost" onClick={reset}>
            Import another file
          </button>
        </div>
      </section>
    );
  }

  // Skipped rows matter one by one; notes on added rows are often the same
  // note many times ("no branch called Delhi"), so they're grouped.
  const order: ImportOutcome[] = ["cannot_import", "repeated", "already_here"];
  const skipped = (preview?.rows.filter((r) => r.outcome !== "ready") ?? []).sort(
    (a, b) => order.indexOf(a.outcome) - order.indexOf(b.outcome) || a.row - b.row,
  );
  const noteGroups = new Map<string, number[]>();
  for (const r of preview?.rows ?? []) {
    if (r.outcome !== "ready") continue;
    for (const n of r.notes) noteGroups.set(n, [...(noteGroups.get(n) ?? []), r.row]);
  }
  const groups = [...noteGroups.entries()].sort((a, b) => b[1].length - a[1].length);
  const sample = preview?.rows.filter((r) => r.outcome === "ready").slice(0, 8) ?? [];

  return (
    <div className="flex flex-col gap-[18px]">
      <section className="panel flex flex-col gap-4 p-6" aria-labelledby="file-title">
        <h2 id="file-title" className="h-section">
          Your file
        </h2>
        <div>
          <label htmlFor="import-branch" className="field-label">
            Students without a branch go to
          </label>
          <select
            id="import-branch"
            className="select w-full"
            style={{ minHeight: 52 }}
            value={branchId}
            onChange={(e) => {
              setBranchId(e.target.value);
              if (file) void read(file, e.target.value);
            }}
          >
            <option value="">No branch</option>
            {branches.map((b) => (
              <option key={b.id} value={b.id}>
                {b.name}
              </option>
            ))}
          </select>
        </div>
        <label
          htmlFor="import-file"
          className="row flex min-h-[120px] cursor-pointer flex-col items-center justify-center gap-1 p-6 text-center"
          style={{ borderStyle: "dashed", borderWidth: 2, borderColor: dragging ? "var(--accent)" : undefined }}
          onDragOver={(e) => {
            e.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={onDrop}
        >
          <span className="font-semibold">{file ? file.name : "Choose a .xlsx or .csv file"}</span>
          <span className="muted text-[13px]">{file ? "Choose another to replace it" : "or drop it here. Up to 2 MB, 2,000 students."}</span>
          <input
            ref={input}
            id="import-file"
            type="file"
            className="sr-only"
            accept=".xlsx,.csv,text/csv,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            onChange={(e) => {
              const chosen = e.target.files?.[0];
              if (chosen) void read(chosen, branchId);
            }}
          />
        </label>
        <p role="status" className="soft m-0 min-h-[1.2em] font-semibold" aria-live="polite">
          {busy === "reading" ? "Reading your file…" : busy === "adding" ? "Adding students…" : ""}
        </p>
        {error ? (
          <p role="alert" className="field-error m-0">
            {error}
          </p>
        ) : null}
      </section>

      {preview ? (
        <>
          <section className="panel flex flex-col gap-4 p-6" aria-labelledby="preview-title">
            <div>
              <h2 id="preview-title" className="h-section">
                {preview.ready === 0
                  ? "Nothing new to add"
                  : `${preview.ready} of ${preview.total} ${preview.total === 1 ? "row" : "rows"} ready to add`}
              </h2>
              <p className="soft m-0 mt-1 text-sm">
                {preview.sheet_name ? `From the sheet "${preview.sheet_name}". ` : ""}
                Read as:{" "}
                {preview.columns.map((c, i) => (
                  <span key={c.title}>
                    {i > 0 ? ", " : ""}
                    {c.title === c.used_as ? c.title : `${c.title} → ${c.used_as}`}
                  </span>
                ))}
                .{preview.ignored_columns.length > 0 ? ` Not used: ${preview.ignored_columns.join(", ")}.` : ""}
              </p>
            </div>
            <ul className="m-0 flex list-none flex-wrap gap-2 p-0" aria-label="What happens to the rows">
              <li className="tag tag-good">{preview.ready} will be added</li>
              {preview.already_here > 0 ? <li className="tag tag-quiet">{preview.already_here} already here</li> : null}
              {preview.repeated > 0 ? <li className="tag tag-quiet">{preview.repeated} repeated</li> : null}
              {preview.cannot_import > 0 ? <li className="tag tag-attn">{preview.cannot_import} can&apos;t be added</li> : null}
              {preview.ready_with_notes > 0 ? <li className="tag tag-quiet">{preview.ready_with_notes} added with notes</li> : null}
            </ul>
            <div className="flex flex-wrap gap-2">
              <button type="button" className="btn btn-primary" disabled={preview.ready === 0 || busy !== null} onClick={add}>
                {busy === "adding" ? "Adding…" : `Add ${preview.ready} ${preview.ready === 1 ? "student" : "students"}`}
              </button>
              <button type="button" className="btn btn-ghost" onClick={reset} disabled={busy !== null}>
                Choose another file
              </button>
            </div>
          </section>

          {skipped.length > 0 ? (
            <section className="panel flex flex-col gap-2 p-6" aria-labelledby="skipped-title">
              <h2 id="skipped-title" className="h-section mb-1">
                Won&apos;t be added
              </h2>
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {skipped.slice(0, 60).map((r) => (
                  <li key={r.row} className="row flex flex-col gap-1 p-3.5">
                    <span className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-bold">
                        <span className="muted mr-2 font-semibold tabular-nums">Row {r.row}</span>
                        {r.name || "No name"}
                      </span>
                      <span className={OUTCOME[r.outcome].tag}>{OUTCOME[r.outcome].label}</span>
                    </span>
                    <span className="soft text-[13px]">{r.notes.join(" ")}</span>
                  </li>
                ))}
              </ul>
              {skipped.length > 60 ? <p className="muted m-0 text-sm">And {skipped.length - 60} more.</p> : null}
            </section>
          ) : null}

          {groups.length > 0 ? (
            <section className="panel flex flex-col gap-2 p-6" aria-labelledby="notes-title">
              <h2 id="notes-title" className="h-section mb-1">
                Added, with a note
              </h2>
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {groups.slice(0, 40).map(([note, rows]) => (
                  <li key={note} className="row flex flex-col gap-1 p-3.5">
                    <span className="font-semibold">{note}</span>
                    <span className="muted text-[13px] tabular-nums">
                      {rows.length === 1 ? "Row " : `${rows.length} rows: `}
                      {rows.slice(0, 12).join(", ")}
                      {rows.length > 12 ? ` and ${rows.length - 12} more` : ""}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          {sample.length > 0 ? (
            <section className="panel flex flex-col gap-2 p-6" aria-labelledby="sample-title">
              <h2 id="sample-title" className="h-section mb-1">
                {preview.ready > sample.length ? `The first ${sample.length} to be added` : "To be added"}
              </h2>
              <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
                {sample.map((r) => (
                  <li key={r.row} className="row flex flex-wrap items-center justify-between gap-x-4 gap-y-1 p-3.5">
                    <span className="min-w-0">
                      <span className="block font-bold">{r.name}</span>
                      <span className="muted text-[13px]">
                        {r.contacts.length === 0
                          ? "No contact"
                          : r.contacts.map((c) => `${c.name} (${RELATION_LABEL[c.relation]}), ${formatPhone(c.phone)}`).join("; ")}
                      </span>
                    </span>
                    <span className="flex flex-wrap items-center gap-2 text-[13px]">
                      <span className="soft">{r.branch_name ?? "No branch"}</span>
                      <span className="soft">Joined {formatDate(r.joined_on)}</span>
                      <BackdatedPill date={r.joined_on} enteredOn={today} />
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
