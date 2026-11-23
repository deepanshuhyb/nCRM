"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import type { Room } from "@/lib/types";

function message(err: unknown) {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

function readCapacity(text: string): number | null | "bad" {
  const t = text.trim();
  if (!t) return null;
  return /^\d+$/.test(t) ? Number(t) : "bad";
}

/** A branch's rooms: list, add, rename, resize, close. */
export function RoomsEditor({ branchId, branchName, rooms, canManage }: { branchId: string; branchName: string; rooms: Room[]; canManage: boolean }) {
  const router = useRouter();
  const [name, setName] = useState("");
  const [capacity, setCapacity] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function add(e: FormEvent) {
    e.preventDefault();
    setError(null);
    if (!name.trim()) return setError("Give the room a name.");
    const cap = readCapacity(capacity);
    if (cap === "bad") return setError("Capacity is a whole number.");
    setBusy(true);
    try {
      await apiSend("POST", `/branches/${branchId}/rooms`, { name: name.trim(), capacity: cap });
      setName("");
      setCapacity("");
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {rooms.length === 0 ? (
        <p className="muted m-0 text-sm">No rooms yet.</p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-1.5 p-0" aria-label={`Rooms at ${branchName}`}>
          {rooms.map((r) => (
            <li key={r.id} className="row flex flex-col gap-1 px-3.5 py-2.5">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-semibold">{r.name}</span>
                <span className="flex items-center gap-2 text-sm">
                  <span className="muted">{r.capacity ? `Holds ${r.capacity}` : "No size set"}</span>
                  {r.active ? null : <span className="tag tag-quiet">Closed</span>}
                </span>
              </div>
              {canManage ? <EditRoom room={r} /> : null}
            </li>
          ))}
        </ul>
      )}
      {canManage ? (
        <details className="disclosure row" open={rooms.length === 0 ? true : undefined}>
          <summary className="flex min-h-[48px] items-center px-3.5 text-sm font-semibold">Add a room</summary>
          <form onSubmit={add} noValidate className="flex flex-wrap items-end gap-2 px-3.5 pb-3.5">
          <div className="min-w-[140px] flex-[2_1_160px]">
            <label htmlFor={`room-name-${branchId}`} className="field-label">
              Room name
            </label>
            <input
              id={`room-name-${branchId}`}
              className="field"
              value={name}
              maxLength={60}
              placeholder="Studio A"
              autoComplete="off"
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="w-[110px]">
            <label htmlFor={`room-cap-${branchId}`} className="field-label">
              Holds
            </label>
            <input
              id={`room-cap-${branchId}`}
              className="field"
              inputMode="numeric"
              maxLength={4}
              value={capacity}
              placeholder="20"
              onChange={(e) => setCapacity(e.target.value)}
            />
          </div>
          <button type="submit" className="btn btn-ghost min-h-[52px]" disabled={busy}>
            {busy ? "Adding…" : "Add room"}
          </button>
          <p role="alert" className="field-error m-0 w-full empty:hidden">
            {error}
          </p>
          </form>
        </details>
      ) : null}
    </div>
  );
}

function EditRoom({ room }: { room: Room }) {
  const router = useRouter();
  const [name, setName] = useState(room.name);
  const [capacity, setCapacity] = useState(room.capacity ? String(room.capacity) : "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save(active: boolean) {
    setError(null);
    const cap = readCapacity(capacity);
    if (cap === "bad") return setError("Capacity is a whole number.");
    setBusy(true);
    try {
      await apiSend("PUT", `/rooms/${room.id}`, { name: name.trim(), capacity: cap, active });
      router.refresh();
    } catch (err) {
      setError(message(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <details className="disclosure">
      <summary className="flex min-h-[40px] items-center text-[13px] font-semibold">Edit {room.name}</summary>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void save(room.active);
        }}
        noValidate
        className="flex flex-wrap items-end gap-2 pt-1 pb-1"
      >
        <div className="min-w-[140px] flex-[2_1_160px]">
          <label htmlFor={`edit-room-${room.id}`} className="field-label">
            Name
          </label>
          <input id={`edit-room-${room.id}`} className="field" value={name} maxLength={60} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="w-[110px]">
          <label htmlFor={`edit-cap-${room.id}`} className="field-label">
            Holds
          </label>
          <input
            id={`edit-cap-${room.id}`}
            className="field"
            inputMode="numeric"
            maxLength={4}
            value={capacity}
            onChange={(e) => setCapacity(e.target.value)}
          />
        </div>
        <button type="submit" className="btn btn-primary min-h-[52px]" disabled={busy}>
          Save
        </button>
        <button type="button" className="btn btn-quiet min-h-[52px]" disabled={busy} onClick={() => save(!room.active)}>
          {room.active ? "Close room" : "Open again"}
        </button>
        <p role="alert" className="field-error m-0 w-full empty:hidden">
          {error}
        </p>
      </form>
    </details>
  );
}
