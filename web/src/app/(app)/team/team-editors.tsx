"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { ROLE_LABEL, type Role } from "@/lib/types";

type BranchOption = { id: string; name: string };

function roleOptions(canGrantOwner: boolean): Role[] {
  return canGrantOwner ? ["owner", "manager", "front_desk", "instructor"] : ["manager", "front_desk", "instructor"];
}

function message(err: unknown) {
  if (err instanceof ApiError && err.code === "conflict") return "Someone on the team already has that number.";
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

function RoleAndBranch({
  idPrefix,
  role,
  branchId,
  onRole,
  onBranch,
  branches,
  canGrantOwner,
}: {
  idPrefix: string;
  role: Role;
  branchId: string;
  onRole: (r: Role) => void;
  onBranch: (b: string) => void;
  branches: BranchOption[];
  canGrantOwner: boolean;
}) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <div>
        <label htmlFor={`${idPrefix}-role`} className="field-label">
          Role
        </label>
        <select id={`${idPrefix}-role`} className="select w-full" style={{ minHeight: 52 }} value={role} onChange={(e) => onRole(e.target.value as Role)}>
          {roleOptions(canGrantOwner).map((r) => (
            <option key={r} value={r}>
              {ROLE_LABEL[r]}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label htmlFor={`${idPrefix}-branch`} className="field-label">
          Branch
        </label>
        <select id={`${idPrefix}-branch`} className="select w-full" style={{ minHeight: 52 }} value={branchId} onChange={(e) => onBranch(e.target.value)}>
          <option value="">All branches</option>
          {branches.map((b) => (
            <option key={b.id} value={b.id}>
              {b.name}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

export function HireForm({ branches, canGrantOwner }: { branches: BranchOption[]; canGrantOwner: boolean }) {
  const router = useRouter();
  const [name, setName] = useState("");
  const [phone, setPhone] = useState("");
  const [role, setRole] = useState<Role>("instructor");
  const [branchId, setBranchId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(null);
    if (!name.trim() || !phone.trim()) return setError("Add a name and a number.");
    setBusy(true);
    try {
      await apiSend("POST", "/staff", { name: name.trim(), phone, role, branch_id: branchId || null });
      setSaved(`${name.trim()} added. They can sign in with their number now.`);
      setName("");
      setPhone("");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="panel flex flex-col gap-4 p-6" aria-labelledby="hire-title">
      <h2 id="hire-title" className="h-section">
        Add someone
      </h2>
      <div>
        <label htmlFor="hire-name" className="field-label">
          Name
        </label>
        <input id="hire-name" className="field" value={name} maxLength={120} onChange={(e) => setName(e.target.value)} autoComplete="off" />
      </div>
      <div>
        <label htmlFor="hire-phone" className="field-label">
          Their WhatsApp number
        </label>
        <input id="hire-phone" className="field" type="tel" inputMode="tel" value={phone} maxLength={20} placeholder="98765 43210" onChange={(e) => setPhone(e.target.value)} autoComplete="off" />
      </div>
      <RoleAndBranch idPrefix="hire" role={role} branchId={branchId} onRole={setRole} onBranch={setBranchId} branches={branches} canGrantOwner={canGrantOwner} />
      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <p role="status" className="accent-text m-0 font-semibold empty:hidden">
        {saved}
      </p>
      <button type="submit" className="btn btn-primary" disabled={busy}>
        {busy ? "Adding…" : "Add to team"}
      </button>
    </form>
  );
}

type Member = { id: string; name: string; phone: string; role: Role; branch_id: string | null; active: boolean };

export function EditMember({ member, branches, canGrantOwner }: { member: Member; branches: BranchOption[]; canGrantOwner: boolean }) {
  const router = useRouter();
  const [form, setForm] = useState({ ...member, branch_id: member.branch_id ?? "" });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save(next: typeof form) {
    setError(null);
    setBusy(true);
    try {
      await apiSend("PUT", `/staff/${member.id}`, {
        name: next.name.trim(),
        phone: next.phone,
        role: next.role,
        branch_id: next.branch_id || null,
        active: next.active,
      });
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    void save(form);
  }

  return (
    <details className="disclosure row">
      <summary className="flex min-h-[48px] items-center px-4 text-sm font-semibold">Edit {member.name}</summary>
      <form onSubmit={submit} noValidate className="flex flex-col gap-3 px-4 pb-4">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div>
            <label htmlFor={`m-${member.id}-name`} className="field-label">
              Name
            </label>
            <input id={`m-${member.id}-name`} className="field" value={form.name} maxLength={120} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          </div>
          <div>
            <label htmlFor={`m-${member.id}-phone`} className="field-label">
              Number
            </label>
            <input id={`m-${member.id}-phone`} className="field" type="tel" value={form.phone} maxLength={20} onChange={(e) => setForm({ ...form, phone: e.target.value })} />
          </div>
        </div>
        <RoleAndBranch
          idPrefix={`m-${member.id}`}
          role={form.role}
          branchId={form.branch_id}
          onRole={(role) => setForm({ ...form, role })}
          onBranch={(branch_id) => setForm({ ...form, branch_id })}
          branches={branches}
          canGrantOwner={canGrantOwner}
        />
        <p className="muted m-0 text-[13px]">Changing the number signs them out; they sign in again with the new one.</p>
        <p role="alert" className="field-error m-0 min-h-[1.2em]">
          {error}
        </p>
        <div className="flex flex-wrap gap-2">
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "Saving…" : "Save"}
          </button>
          <button type="button" className={member.active ? "btn btn-ghost" : "btn btn-dark"} disabled={busy} onClick={() => save({ ...form, active: !member.active })}>
            {member.active ? "Deactivate" : "Reactivate"}
          </button>
        </div>
      </form>
    </details>
  );
}
