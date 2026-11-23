"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";

export function NewBranchForm() {
  const router = useRouter();
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setSaved(null);
    if (!name.trim()) {
      setError("Give the branch a name.");
      return;
    }
    setBusy(true);
    try {
      await apiSend("POST", "/branches", {
        name: name.trim(),
        address: address.trim() || null,
      });
      setSaved(`${name.trim()} added.`);
      setName("");
      setAddress("");
      router.refresh();
    } catch (err) {
      if (err instanceof ApiError && err.code === "conflict") {
        setError("You already have a branch with that name.");
      } else {
        setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} noValidate className="panel flex flex-col gap-4 p-6" aria-labelledby="new-branch-title">
      <h2 id="new-branch-title" className="h-section">
        Add a branch
      </h2>
      <div>
        <label htmlFor="branch-name" className="field-label">
          Name
        </label>
        <input
          id="branch-name"
          className="field"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="North"
          maxLength={80}
          autoComplete="off"
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? "branch-error" : undefined}
          required
        />
      </div>
      <div>
        <label htmlFor="branch-address" className="field-label">
          Address <span className="muted font-normal">(optional)</span>
        </label>
        <input
          id="branch-address"
          className="field"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          placeholder="Sector 12, Dwarka"
          maxLength={200}
          autoComplete="off"
        />
      </div>
      <p id="branch-error" role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <p role="status" className="accent-text m-0 font-semibold empty:hidden">
        {saved}
      </p>
      <button type="submit" className="btn btn-primary" disabled={busy}>
        {busy ? "Adding…" : "Add branch"}
      </button>
    </form>
  );
}
