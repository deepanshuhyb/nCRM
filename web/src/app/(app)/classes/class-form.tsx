"use client";

import { useRouter } from "next/navigation";
import { useState, type FormEvent } from "react";

import { ApiError, apiSend } from "@/lib/api-client";
import { WEEKDAY_LONG, weekdayShort } from "@/lib/format";
import type { BatchDetail, Slot } from "@/lib/types";

type Option = { id: string; name: string };
type RoomOption = Option & { branch_id: string; capacity: number | null };

/** Days that share a start and end time. Most classes need just one. */
type TimeGroup = { days: number[]; starts_at: string; ends_at: string };

export type ClassInitial = {
  id: string;
  name: string;
  style: string | null;
  branch_id: string;
  room_id: string | null;
  instructor_id: string | null;
  capacity: number | null;
  starts_on: string;
  ends_on: string | null;
  active: boolean;
  slots: Slot[];
  enrolled: number;
};

function groupsFrom(slots: Slot[]): TimeGroup[] {
  const groups: TimeGroup[] = [];
  for (const s of slots) {
    const g = groups.find((x) => x.starts_at === s.starts_at && x.ends_at === s.ends_at);
    if (g) g.days.push(s.weekday);
    else groups.push({ days: [s.weekday], starts_at: s.starts_at, ends_at: s.ends_at });
  }
  return groups.length > 0 ? groups : [{ days: [], starts_at: "18:00", ends_at: "19:00" }];
}

export function ClassForm({
  branches,
  rooms,
  team,
  today,
  initial,
}: {
  branches: Option[];
  rooms: RoomOption[];
  team: Option[];
  today: string;
  initial?: ClassInitial;
}) {
  const router = useRouter();
  const editing = initial !== undefined;
  const [name, setName] = useState(initial?.name ?? "");
  const [style, setStyle] = useState(initial?.style ?? "");
  const [branchId, setBranchId] = useState(initial?.branch_id ?? (branches.length === 1 ? branches[0].id : ""));
  const [roomId, setRoomId] = useState(initial?.room_id ?? "");
  const [instructorId, setInstructorId] = useState(initial?.instructor_id ?? "");
  const [capacity, setCapacity] = useState(initial?.capacity != null ? String(initial.capacity) : "");
  const [startsOn, setStartsOn] = useState(initial?.starts_on ?? today);
  const [endsOn, setEndsOn] = useState(initial?.ends_on ?? "");
  const [groups, setGroups] = useState<TimeGroup[]>(groupsFrom(initial?.slots ?? []));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmArchive, setConfirmArchive] = useState(false);

  const branchRooms = rooms.filter((r) => r.branch_id === branchId);
  const room = branchRooms.find((r) => r.id === roomId);

  function updateGroup(i: number, next: Partial<TimeGroup>) {
    setGroups(groups.map((g, j) => (j === i ? { ...g, ...next } : g)));
  }

  function toggleDay(i: number, day: number) {
    const g = groups[i];
    updateGroup(i, { days: g.days.includes(day) ? g.days.filter((d) => d !== day) : [...g.days, day].sort() });
  }

  function slots(): Slot[] {
    return groups.flatMap((g) => g.days.map((weekday) => ({ weekday, starts_at: g.starts_at, ends_at: g.ends_at })));
  }

  function body(active: boolean) {
    return {
      name: name.trim(),
      style: style.trim() || null,
      branch_id: branchId,
      room_id: roomId || null,
      instructor_id: instructorId || null,
      capacity: capacity.trim() ? Number(capacity) : null,
      starts_on: startsOn || (editing ? initial.starts_on : null),
      ends_on: endsOn || null,
      active,
      slots: slots(),
    };
  }

  async function save(active: boolean) {
    setError(null);
    // Bring the field that needs fixing into view: the form is longer than a phone screen.
    const fix = (id: string, message: string) => {
      document.getElementById(id)?.focus();
      setError(message);
    };
    if (!name.trim()) return fix("class-name", "Give the class a name.");
    if (!branchId) return fix("class-branch", "Choose the branch.");
    if (capacity.trim() && !/^\d+$/.test(capacity.trim())) return fix("class-capacity", "Capacity is a whole number.");
    if (active && slots().length === 0) return setError("Pick at least one day.");
    setBusy(true);
    try {
      if (editing) {
        await apiSend<BatchDetail>("PUT", `/batches/${initial.id}`, body(active));
        setConfirmArchive(false);
        router.refresh();
      } else {
        const created = await apiSend<BatchDetail>("POST", "/batches", body(true));
        router.push(`/classes/${created.id}`);
        router.refresh();
      }
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Something went wrong. Try again.");
    } finally {
      setBusy(false);
    }
  }

  function submit(e: FormEvent) {
    e.preventDefault();
    void save(initial?.active ?? true);
  }

  return (
    <form onSubmit={submit} noValidate className="flex flex-col gap-[18px]">
      <section className="panel flex flex-col gap-4 p-6" aria-labelledby="class-basics">
        <h2 id="class-basics" className="h-section">
          The class
        </h2>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1.4fr_1fr]">
          <div>
            <label htmlFor="class-name" className="field-label">
              Name
            </label>
            <input
              id="class-name"
              className="field"
              value={name}
              maxLength={80}
              placeholder="Hip-hop Beginners"
              onChange={(e) => setName(e.target.value)}
              autoComplete="off"
            />
          </div>
          <div>
            <label htmlFor="class-style" className="field-label">
              Style <span className="muted font-normal">(optional)</span>
            </label>
            <input
              id="class-style"
              className="field"
              value={style}
              maxLength={40}
              placeholder="Hip-hop"
              onChange={(e) => setStyle(e.target.value)}
              autoComplete="off"
            />
          </div>
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <div>
            <label htmlFor="class-branch" className="field-label">
              Branch
            </label>
            <select
              id="class-branch"
              className="select w-full"
              style={{ minHeight: 52 }}
              value={branchId}
              disabled={editing && initial.enrolled > 0}
              onChange={(e) => {
                setBranchId(e.target.value);
                setRoomId("");
              }}
            >
              <option value="">Choose…</option>
              {branches.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="class-room" className="field-label">
              Room
            </label>
            <select
              id="class-room"
              className="select w-full"
              style={{ minHeight: 52 }}
              value={roomId}
              onChange={(e) => setRoomId(e.target.value)}
              disabled={!branchId}
            >
              <option value="">{!branchId ? "Choose a branch first" : branchRooms.length === 0 ? "No rooms at this branch" : "No room"}</option>
              {branchRooms.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.name}
                  {r.capacity ? ` (holds ${r.capacity})` : ""}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="class-teacher" className="field-label">
              Teacher
            </label>
            <select
              id="class-teacher"
              className="select w-full"
              style={{ minHeight: 52 }}
              value={instructorId}
              onChange={(e) => setInstructorId(e.target.value)}
            >
              <option value="">Not decided</option>
              {team.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          <div>
            <label htmlFor="class-capacity" className="field-label">
              Most students
            </label>
            <input
              id="class-capacity"
              className="field"
              inputMode="numeric"
              value={capacity}
              maxLength={3}
              placeholder={room?.capacity ? `Room holds ${room.capacity}` : "No limit"}
              onChange={(e) => setCapacity(e.target.value)}
              autoComplete="off"
            />
          </div>
          <div>
            <label htmlFor="class-starts" className="field-label">
              First class
            </label>
            <input id="class-starts" type="date" className="field" value={startsOn} onChange={(e) => setStartsOn(e.target.value)} />
          </div>
          <div>
            <label htmlFor="class-ends" className="field-label">
              Last class <span className="muted font-normal">(optional)</span>
            </label>
            <input id="class-ends" type="date" className="field" value={endsOn} min={startsOn} onChange={(e) => setEndsOn(e.target.value)} />
          </div>
        </div>
      </section>

      <section className="panel flex flex-col gap-4 p-6" aria-labelledby="class-when">
        <div>
          <h2 id="class-when" className="h-section">
            When it meets
          </h2>
          <p className="muted mt-1 mb-0 text-sm">Pick the days, then the time. Add another time if some days differ.</p>
        </div>
        {groups.map((g, i) => (
          <fieldset key={i} className="row m-0 flex flex-col gap-3 border-0 p-4">
            <legend className="sr-only">Time {i + 1}</legend>
            <div className="flex flex-wrap gap-2" role="group" aria-label="Days">
              {[1, 2, 3, 4, 5, 6, 7].map((day) => {
                const usedElsewhere = groups.some((other, j) => j !== i && other.days.includes(day) && overlaps(other, g));
                return (
                  <label key={day} className="day-toggle" title={WEEKDAY_LONG[day - 1]}>
                    <input
                      type="checkbox"
                      checked={g.days.includes(day)}
                      disabled={usedElsewhere && !g.days.includes(day)}
                      onChange={() => toggleDay(i, day)}
                      aria-label={WEEKDAY_LONG[day - 1]}
                    />
                    <span aria-hidden="true">{weekdayShort(day)}</span>
                  </label>
                );
              })}
            </div>
            <div className="flex flex-wrap items-end gap-3">
              <div>
                <label htmlFor={`g${i}-from`} className="field-label">
                  From
                </label>
                <input
                  id={`g${i}-from`}
                  type="time"
                  step={300}
                  className="field w-[150px]"
                  value={g.starts_at}
                  onChange={(e) => updateGroup(i, { starts_at: e.target.value })}
                />
              </div>
              <div>
                <label htmlFor={`g${i}-to`} className="field-label">
                  To
                </label>
                <input
                  id={`g${i}-to`}
                  type="time"
                  step={300}
                  className="field w-[150px]"
                  value={g.ends_at}
                  onChange={(e) => updateGroup(i, { ends_at: e.target.value })}
                />
              </div>
              {groups.length > 1 ? (
                <button type="button" className="btn btn-quiet" onClick={() => setGroups(groups.filter((_, j) => j !== i))}>
                  Remove this time
                </button>
              ) : null}
            </div>
          </fieldset>
        ))}
        {groups.length < 7 ? (
          <button
            type="button"
            className="btn btn-ghost self-start"
            onClick={() => setGroups([...groups, { days: [], starts_at: groups[groups.length - 1].starts_at, ends_at: groups[groups.length - 1].ends_at }])}
          >
            Add another time
          </button>
        ) : null}
      </section>

      <p role="alert" className="field-error m-0 min-h-[1.2em]">
        {error}
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Saving…" : editing ? "Save changes" : "Add class"}
        </button>
        {!editing ? (
          <button type="button" className="btn btn-ghost" onClick={() => router.back()}>
            Cancel
          </button>
        ) : initial.active ? (
          confirmArchive ? (
            <span className="flex flex-wrap items-center gap-2">
              <span className="text-sm font-semibold">
                {initial.enrolled > 0
                  ? `This takes ${initial.enrolled} ${initial.enrolled === 1 ? "student" : "students"} out of the class from today.`
                  : "It leaves the timetable."}
              </span>
              <button type="button" className="btn btn-dark" disabled={busy} onClick={() => save(false)}>
                Archive {initial.name}
              </button>
              <button type="button" className="btn btn-quiet" onClick={() => setConfirmArchive(false)}>
                Keep it
              </button>
            </span>
          ) : (
            <button type="button" className="btn btn-ghost" onClick={() => setConfirmArchive(true)}>
              Archive class
            </button>
          )
        ) : (
          <button type="button" className="btn btn-dark" disabled={busy} onClick={() => save(true)}>
            Bring back to the timetable
          </button>
        )}
      </div>
    </form>
  );
}

function overlaps(a: TimeGroup, b: TimeGroup) {
  return a.starts_at < b.ends_at && b.starts_at < a.ends_at;
}
