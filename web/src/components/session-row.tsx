import Link from "next/link";

import { formatDay } from "@/lib/format";
import type { SessionSummary } from "@/lib/types";

function state(s: SessionSummary): { label: string; tag: string } {
  if (s.status === "cancelled") return { label: "Cancelled", tag: "tag tag-quiet" };
  if (s.expected === 0 && s.came === 0) return { label: "No students yet", tag: "tag tag-quiet" };
  if (s.unmarked === 0) return { label: "Marked", tag: "tag tag-good" };
  if (s.ended) return { label: `${s.unmarked} not marked`, tag: "tag tag-attn" };
  return { label: s.came + s.away > 0 ? `${s.unmarked} to mark` : "Not started", tag: "tag tag-quiet" };
}

/** One class meeting as a tappable row: time, class, where, and how marking stands. */
export function SessionRow({ s, mine, showDate }: { s: SessionSummary; mine: boolean; showDate?: boolean }) {
  const st = state(s);
  const marked = s.came + s.away;
  const total = Math.max(s.expected, marked);
  return (
    <Link
      href={`/attendance/${s.id}`}
      className="row flex flex-col gap-1.5 px-4 py-3 no-underline"
      style={{ color: "inherit", boxShadow: mine ? "inset 3px 0 0 var(--accent)" : undefined }}
    >
      <span className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-sm font-bold tabular-nums">
          {showDate ? `${formatDay(s.on_date)}, ` : ""}
          {s.starts_at}–{s.ends_at}
        </span>
        <span className={st.tag}>{st.label}</span>
      </span>
      <span className="list-row-title">{s.batch_name}</span>
      <span className="muted text-[13px]">
        {[s.branch_name, s.room_name, mine ? "You teach" : s.instructor_name].filter(Boolean).join(" · ")}
        {s.teacher_present === true ? " · teacher in" : s.teacher_present === false ? " · teacher absent" : ""}
      </span>
      {s.status === "scheduled" && total > 0 ? (
        <span className="flex items-center gap-2 text-[13px] font-semibold">
          <span className="meter flex-1" aria-hidden="true">
            <span style={{ width: `${(marked / total) * 100}%` }} />
          </span>
          <span className="tabular-nums">
            {marked === 0 ? `${s.expected} expected` : `${s.came} came${s.away > 0 ? `, ${s.away} away` : ""}`}
          </span>
        </span>
      ) : null}
    </Link>
  );
}
