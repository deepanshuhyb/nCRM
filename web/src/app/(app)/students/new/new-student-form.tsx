"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ContactFields, emptyContact, type ContactDraft } from "@/components/contact-fields";
import { BackdatableDate } from "@/components/backdatable-date";
import { ApiError, apiSend } from "@/lib/api-client";
import type { StudentDetail } from "@/lib/types";

const MAX_CONTACTS = 4;

export function NewStudentForm({ branches, today }: { branches: { id: string; name: string }[]; today: string }) {
  const router = useRouter();
  const [name, setName] = useState("");
  const [dob, setDob] = useState("");
  const [branchId, setBranchId] = useState(branches.length === 1 ? branches[0].id : "");
  const [notes, setNotes] = useState("");
  const [joinedOn, setJoinedOn] = useState(today);
  const [contacts, setContacts] = useState<ContactDraft[]>([emptyContact()]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);


  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!name.trim()) return setError("Add the student's name.");
    const filled = contacts.filter((c) => c.name.trim() || c.phone.trim());
    if (filled.some((c) => !c.name.trim() || !c.phone.trim())) {
      return setError("Each contact needs a name and a number.");
    }

    setBusy(true);
    try {
      const created = await apiSend<StudentDetail>("POST", "/students", {
        name: name.trim(),
        date_of_birth: dob || null,
        branch_id: branchId || null,
        notes: notes.trim() || null,
        joined_on: joinedOn || today,
        contacts: filled.map((c, i) => ({ ...c, is_primary: i === 0 })),
      });
      router.push(`/students/${created.id}`);
      router.refresh();
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-[18px]">
      <section className="panel flex flex-col gap-4 p-6" aria-labelledby="student-section">
        <h2 id="student-section" className="h-section">
          Student
        </h2>
        <div>
          <label htmlFor="student-name" className="field-label">
            Full name
          </label>
          <input
            id="student-name"
            className="field"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={120}
            autoComplete="off"
            required
          />
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <div>
            <label htmlFor="student-dob" className="field-label">
              Date of birth <span className="muted font-normal">(optional)</span>
            </label>
            <input id="student-dob" type="date" className="field" value={dob} max={today} min="1900-01-01" onChange={(e) => setDob(e.target.value)} />
          </div>
          <BackdatableDate
            id="student-joined"
            label="Joined on"
            value={joinedOn}
            onChange={setJoinedOn}
            max={today}
            enteredOn={today}
          />
          <div>
            <label htmlFor="student-branch" className="field-label">
              Branch
            </label>
            <select id="student-branch" className="select w-full" style={{ minHeight: 52 }} value={branchId} onChange={(e) => setBranchId(e.target.value)}>
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
          <label htmlFor="student-notes" className="field-label">
            Notes <span className="muted font-normal">(optional, staff only)</span>
          </label>
          <textarea
            id="student-notes"
            className="field py-3"
            style={{ minHeight: 96 }}
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            maxLength={2000}
          />
        </div>
      </section>

      <section className="panel flex flex-col gap-4 p-6" aria-labelledby="contacts-section">
        <div>
          <h2 id="contacts-section" className="h-section">
            Who we message
          </h2>
          <p className="muted mt-1 mb-0 text-sm">The first one is the main contact for reminders and fees.</p>
        </div>
        {contacts.map((c, i) => (
          <div key={i} className="row flex flex-col gap-3 p-4">
            <ContactFields
              idPrefix={`contact-${i}`}
              value={c}
              onChange={(next) => setContacts(contacts.map((old, j) => (j === i ? next : old)))}
            />
            {contacts.length > 1 ? (
              <button type="button" className="btn btn-quiet self-start" onClick={() => setContacts(contacts.filter((_, j) => j !== i))}>
                Remove this contact
              </button>
            ) : null}
          </div>
        ))}
        {contacts.length < MAX_CONTACTS ? (
          <button type="button" className="btn btn-ghost self-start" onClick={() => setContacts([...contacts, { ...emptyContact(), relation: "father" }])}>
            Add another contact
          </button>
        ) : null}
      </section>

      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <div className="flex flex-wrap gap-3">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Adding…" : "Add student"}
        </button>
        <button type="button" className="btn btn-ghost" onClick={() => router.back()}>
          Cancel
        </button>
      </div>
    </form>
  );
}
