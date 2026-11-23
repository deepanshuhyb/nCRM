"use client";

import { RELATION_LABEL, type Relation } from "@/lib/types";

export type ContactDraft = { name: string; phone: string; relation: Relation };

export const emptyContact = (): ContactDraft => ({ name: "", phone: "", relation: "mother" });

/** Name, phone and relation for one contact. */
export function ContactFields({
  idPrefix,
  value,
  onChange,
}: {
  idPrefix: string;
  value: ContactDraft;
  onChange: (next: ContactDraft) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1.2fr_1fr_0.9fr]">
      <div>
        <label htmlFor={`${idPrefix}-name`} className="field-label">
          Contact name
        </label>
        <input
          id={`${idPrefix}-name`}
          className="field"
          value={value.name}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
          maxLength={120}
          autoComplete="off"
        />
      </div>
      <div>
        <label htmlFor={`${idPrefix}-phone`} className="field-label">
          WhatsApp number
        </label>
        <input
          id={`${idPrefix}-phone`}
          className="field"
          type="tel"
          inputMode="tel"
          value={value.phone}
          onChange={(e) => onChange({ ...value, phone: e.target.value })}
          placeholder="98765 43210"
          maxLength={20}
          autoComplete="off"
        />
      </div>
      <div>
        <label htmlFor={`${idPrefix}-relation`} className="field-label">
          Who they are
        </label>
        <select
          id={`${idPrefix}-relation`}
          className="select w-full"
          style={{ minHeight: 52 }}
          value={value.relation}
          onChange={(e) => onChange({ ...value, relation: e.target.value as Relation })}
        >
          {(Object.keys(RELATION_LABEL) as Relation[]).map((r) => (
            <option key={r} value={r}>
              {RELATION_LABEL[r]}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
