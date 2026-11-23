"use client";

import { BackdatedPill } from "@/components/backdated-pill";

/**
 * A join or start date that may be set in the past. Shows the Backdated
 * pill as soon as the date is earlier than `enteredOn` (today for a new
 * record, the day it was entered for an existing one).
 */
export function BackdatableDate({
  id,
  label,
  value,
  onChange,
  max,
  enteredOn,
  min = "2000-01-01",
  hint,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (next: string) => void;
  /** Latest date allowed: today for join dates, a year ahead for class starts. */
  max: string;
  enteredOn: string;
  min?: string;
  hint?: string;
}) {
  return (
    <div>
      <label htmlFor={id} className="field-label flex flex-wrap items-center gap-2">
        {label}
        {value ? <BackdatedPill date={value} enteredOn={enteredOn} /> : null}
      </label>
      <input
        id={id}
        type="date"
        className="field"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(e.target.value)}
        aria-describedby={hint ? `${id}-hint` : undefined}
      />
      {hint ? (
        <p id={`${id}-hint`} className="muted m-0 mt-1 text-[13px]">
          {hint}
        </p>
      ) : null}
    </div>
  );
}
