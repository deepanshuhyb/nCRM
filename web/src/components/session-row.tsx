import { Meter } from "@/components/graphs";
import { TravelLink, Travels } from "@/components/travel";
import { formatDay } from "@/lib/format";
import type { SessionSummary } from "@/lib/types";

/** How marking stands for one class meeting, in words and a tone. */
export function sessionState(s: SessionSummary): { label: string; tag: string; state: "done" | "due" | "later" | "off" } {
  if (s.status === "cancelled") return { label: "Cancelled", tag: "tag tag-quiet", state: "off" };
  if (s.expected === 0 && s.came === 0) return { label: "No students yet", tag: "tag tag-quiet", state: "later" };
  if (s.unmarked === 0) return { label: "Marked", tag: "tag tag-good", state: "done" };
  if (s.ended) return { label: `${s.unmarked} not marked`, tag: "tag tag-attn", state: "due" };
  return { label: s.came + s.away > 0 ? `${s.unmarked} to mark` : "Not started", tag: "tag tag-quiet", state: "later" };
}

/** One class meeting as a tappable row: time, class, where, and how marking stands. */
export function SessionRow({ s, mine, showDate }: { s: SessionSummary; mine: boolean; showDate?: boolean }) {
  const st = sessionState(s);
  const marked = s.came + s.away;
  const total = Math.max(s.expected, marked);
  return (
    <TravelLink
      kind="session"
      id={s.id}
      label={s.batch_name}
      href={`/attendance/${s.id}`}
      className="row flex flex-col gap-1.5 px-4 py-3 no-underline"
      style={{ boxShadow: mine ? "inset 3px 0 0 var(--accent)" : undefined }}
    >
      <span className="flex flex-wrap items-center justify-between gap-2">
        <span className="caption font-bold tabular-nums" style={{ color: "var(--soft)" }}>
          {showDate ? `${formatDay(s.on_date)}, ` : ""}
          {s.starts_at}–{s.ends_at}
        </span>
        <span className={st.tag}>{st.label}</span>
      </span>
      <Travels kind="session" id={s.id}>
        <span className="h-card self-start">{s.batch_name}</span>
      </Travels>
      <span className="caption">
        {[s.branch_name, s.room_name, mine ? "You teach" : s.instructor_name].filter(Boolean).join(" · ")}
        {s.teacher_present === true ? " · teacher in" : s.teacher_present === false ? " · teacher absent" : ""}
      </span>
      {s.status === "scheduled" && total > 0 ? (
        <span className="mt-1 flex items-center gap-3 text-[13px] font-semibold">
          <span className="flex-1">
            <Meter value={marked / total} />
          </span>
          <span className="tabular-nums">{marked === 0 ? `${s.expected} expected` : `${s.came} came${s.away > 0 ? `, ${s.away} away` : ""}`}</span>
        </span>
      ) : null}
    </TravelLink>
  );
}
