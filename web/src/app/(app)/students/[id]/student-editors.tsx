"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ContactFields, emptyContact, type ContactDraft } from "@/components/contact-fields";
import { BackdatableDate } from "@/components/backdatable-date";
import { ApiError, apiSend } from "@/lib/api-client";
import { STATUS_LABEL, type StudentStatus } from "@/lib/types";

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

export function RemoveContactButton({ studentId, contactId, name }: { studentId: string; contactId: string; name: string }) {
  const router = useRouter();
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  if (!confirming) {
    return (
      <button type="button" className="btn btn-quiet" onClick={() => setConfirming(true)} aria-label={`Remove ${name}`}>
        Remove
      </button>
    );
  }
  return (
    <span className="flex gap-2">
      <button
        type="button"
        className="btn btn-dark min-h-[44px]"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try {
            await apiSend("DELETE", `/students/${studentId}/contacts/${contactId}`);
            router.refresh();
          } finally {
            setBusy(false);
            setConfirming(false);
          }
        }}
      >
        {busy ? "Removing…" : `Remove ${name}`}
      </button>
      <button type="button" className="btn btn-quiet" onClick={() => setConfirming(false)}>
        Keep
      </button>
    </span>
  );
}

export function AddContactForm({ studentId }: { studentId: string }) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState<ContactDraft>(emptyContact());
  const [primary, setPrimary] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!open) {
    return (
      <button type="button" className="btn btn-ghost self-start" onClick={() => setOpen(true)}>
        Add a contact
      </button>
    );
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!draft.name.trim() || !draft.phone.trim()) return setError("Add a name and a number.");
    setBusy(true);
    try {
      await apiSend("POST", `/students/${studentId}/contacts`, { ...draft, is_primary: primary });
      setDraft(emptyContact());
      setPrimary(false);
      setOpen(false);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError && err.code === "conflict" ? "That number is already a contact for this student." : message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="row flex flex-col gap-3 p-4">
      <ContactFields idPrefix="new-contact" value={draft} onChange={setDraft} />
      <label className="flex min-h-[44px] items-center gap-2 font-semibold">
        <input type="checkbox" checked={primary} onChange={(e) => setPrimary(e.target.checked)} className="size-5" />
        Make this the main contact
      </label>
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <div className="flex flex-wrap gap-2">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Adding…" : "Add contact"}
        </button>
        <button type="button" className="btn btn-quiet" onClick={() => setOpen(false)}>
          Cancel
        </button>
      </div>
    </form>
  );
}

type EditableStudent = {
  id: string;
  name: string;
  date_of_birth: string | null;
  branch_id: string | null;
  status: StudentStatus;
  notes: string | null;
  joined_on: string;
  entered_on: string;
};

export function EditStudentForm({
  student,
  branches,
  today,
}: {
  student: EditableStudent;
  branches: { id: string; name: string }[];
  /** The studio's date, not the browser's: they differ around midnight. */
  today: string;
}) {
  const router = useRouter();
  const [form, setForm] = useState({
    name: student.name,
    date_of_birth: student.date_of_birth ?? "",
    branch_id: student.branch_id ?? "",
    status: student.status,
    notes: student.notes ?? "",
    joined_on: student.joined_on,
  });
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);
  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(false);
    if (!form.name.trim()) return setError("The name can't be empty.");
    if (!form.joined_on) return setError("Add the date they joined.");
    setBusy(true);
    try {
      await apiSend("PUT", `/students/${student.id}`, {
        name: form.name.trim(),
        date_of_birth: form.date_of_birth || null,
        branch_id: form.branch_id || null,
        status: form.status,
        notes: form.notes.trim() || null,
        joined_on: form.joined_on,
      });
      setSaved(true);
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="glass disclosure">
      <summary className="flex min-h-[60px] items-center px-6 font-semibold">Edit details or status</summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-4 px-6 pb-6">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div>
            <label htmlFor="edit-name" className="field-label">
              Full name
            </label>
            <input id="edit-name" className="field" value={form.name} maxLength={120} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          </div>
          <div>
            <label htmlFor="edit-status" className="field-label">
              Status
            </label>
            <select id="edit-status" className="select w-full" style={{ minHeight: 52 }} value={form.status} onChange={(e) => setForm({ ...form, status: e.target.value as StudentStatus })}>
              {(Object.keys(STATUS_LABEL) as StudentStatus[]).map((s) => (
                <option key={s} value={s}>
                  {STATUS_LABEL[s]}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="edit-dob" className="field-label">
              Date of birth
            </label>
            <input id="edit-dob" type="date" className="field" value={form.date_of_birth} max={today} min="1900-01-01" onChange={(e) => setForm({ ...form, date_of_birth: e.target.value })} />
          </div>
          <BackdatableDate
            id="edit-joined"
            label="Joined on"
            value={form.joined_on}
            onChange={(joined_on) => setForm({ ...form, joined_on })}
            max={today}
            enteredOn={student.entered_on}
          />
          <div>
            <label htmlFor="edit-branch" className="field-label">
              Branch
            </label>
            <select id="edit-branch" className="select w-full" style={{ minHeight: 52 }} value={form.branch_id} onChange={(e) => setForm({ ...form, branch_id: e.target.value })}>
              <option value="">Not set</option>
              {branches.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div>
          <label htmlFor="edit-notes" className="field-label">
            Notes <span className="muted font-normal">(staff only)</span>
          </label>
          <textarea id="edit-notes" className="field py-3" style={{ minHeight: 96 }} maxLength={2000} value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} />
        </div>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <p role="status" className="accent-text m-0 font-semibold empty:hidden">
          {saved ? "Changes saved." : ""}
        </p>
        <button type="submit" className="btn btn-primary self-start" disabled={busy}>
          {busy ? "Saving…" : "Save changes"}
        </button>
      </form>
    </details>
  );
}
