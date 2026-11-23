
import { TravelLink } from "@/components/travel";
import { WEEKDAY_LONG, minutesOf, weekdayShort } from "@/lib/format";
import type { BatchSummary, Slot } from "@/lib/types";

/** Pixels per minute: one hour is 84px tall, enough for a name, time and count. */
const PX_PER_MINUTE = 1.4;
/** Height of a stretch of the day with no classes, drawn collapsed. */
const BREAK_PX = 40;
/** Hours with nothing in them are only collapsed when there are this many minutes of them. */
const MIN_BREAK = 120;

type Band = { from: number; to: number };

/**
 * The parts of the day that have classes, in whole hours. A studio with a
 * 7am class and evening classes would otherwise show eight empty hours in
 * the middle; those are drawn as one short strip instead.
 */
function bandsOf(items: { slot: Slot }[]): Band[] {
  const spans = items
    .map((i) => ({ from: Math.floor(minutesOf(i.slot.starts_at) / 60) * 60, to: Math.ceil(minutesOf(i.slot.ends_at) / 60) * 60 }))
    .sort((a, b) => a.from - b.from);
  const bands: Band[] = [];
  for (const s of spans) {
    const last = bands[bands.length - 1];
    if (last && s.from - last.to < MIN_BREAK) last.to = Math.max(last.to, s.to);
    else bands.push({ ...s });
  }
  return bands;
}

/** Where a time of day sits, in pixels from the top, with breaks collapsed. */
function yOf(bands: Band[], minute: number): number {
  let y = 0;
  for (const b of bands) {
    if (minute <= b.to) return y + (Math.max(minute, b.from) - b.from) * PX_PER_MINUTE;
    y += (b.to - b.from) * PX_PER_MINUTE + BREAK_PX;
  }
  return y - BREAK_PX;
}

const clock = (minute: number) => `${String(Math.floor(minute / 60)).padStart(2, "0")}:00`;

type Placed = { batch: BatchSummary; slot: Slot; start: number; end: number; lane: number; lanes: number };

/**
 * Lay out one day's classes. Overlapping classes (different rooms at the
 * same time) sit side by side: each takes the first free lane, and every
 * block in a cluster of overlaps shares that cluster's lane count.
 */
function placeDay(items: { batch: BatchSummary; slot: Slot }[]): Placed[] {
  const sorted = items
    .map((i) => ({ ...i, start: minutesOf(i.slot.starts_at), end: minutesOf(i.slot.ends_at) }))
    .sort((a, b) => a.start - b.start || b.end - a.end);
  const placed: Placed[] = [];
  let cluster: Placed[] = [];
  let clusterEnd = -1;
  let laneEnds: number[] = [];

  const closeCluster = () => {
    const lanes = Math.max(1, laneEnds.length);
    for (const p of cluster) p.lanes = lanes;
    placed.push(...cluster);
    cluster = [];
    laneEnds = [];
  };

  for (const item of sorted) {
    if (item.start >= clusterEnd && cluster.length > 0) closeCluster();
    let lane = laneEnds.findIndex((end) => end <= item.start);
    if (lane === -1) {
      lane = laneEnds.length;
      laneEnds.push(item.end);
    } else {
      laneEnds[lane] = item.end;
    }
    cluster.push({ ...item, lane, lanes: 1 });
    clusterEnd = Math.max(clusterEnd, item.end);
  }
  if (cluster.length > 0) closeCluster();
  return placed;
}

function fullness(b: BatchSummary) {
  if (b.capacity === null) return { full: false, ratio: 0, label: `${b.enrolled} in class` };
  const ratio = Math.min(1, b.enrolled / b.capacity);
  return { full: b.enrolled >= b.capacity, ratio, label: `${b.enrolled} of ${b.capacity}` };
}

function accessibleName(b: BatchSummary, s: Slot, showBranch: boolean, mine: boolean) {
  const where = [showBranch ? b.branch_name : null, b.room_name].filter(Boolean).join(", ");
  const f = fullness(b);
  const places = b.capacity === null ? `${b.enrolled} students` : `${b.enrolled} of ${b.capacity} places taken${f.full ? ", full" : ""}`;
  return [
    `${b.name}, ${WEEKDAY_LONG[s.weekday - 1]} ${s.starts_at} to ${s.ends_at}`,
    where || null,
    b.instructor_name ? `taught by ${mine ? "you" : b.instructor_name}` : null,
    places,
  ]
    .filter(Boolean)
    .join(", ");
}

export function Timetable({
  batches,
  todayWeekday,
  myStaffId,
  showBranch,
}: {
  batches: BatchSummary[];
  todayWeekday: number | null;
  myStaffId: string;
  showBranch: boolean;
}) {
  const items = batches.flatMap((batch) => batch.slots.map((slot) => ({ batch, slot })));
  const bands = bandsOf(items);
  const height = yOf(bands, bands[bands.length - 1].to);
  const hours = bands.flatMap((band) => Array.from({ length: (band.to - band.from) / 60 + 1 }, (_, i) => band.from + i * 60));
  const days = [1, 2, 3, 4, 5, 6, 7].map((weekday) => ({
    weekday,
    placed: placeDay(items.filter((i) => i.slot.weekday === weekday)),
  }));

  return (
    <>
      <div className="timetable">
        <span aria-hidden="true" />
        {days.map((d) => (
          <div key={d.weekday} className="tt-head" data-today={d.weekday === todayWeekday ? "" : undefined} aria-hidden="true">
            {weekdayShort(d.weekday)}
          </div>
        ))}

        <div className="tt-axis" style={{ height }} aria-hidden="true">
          {hours.map((m) => (
            <span key={m} style={{ top: yOf(bands, m) }}>
              {clock(m)}
            </span>
          ))}
        </div>

        {/* Hour lines and collapsed breaks, drawn once behind all seven days. */}
        <div className="tt-lines" style={{ height }} aria-hidden="true">
          {hours.map((m) => (
            <i key={m} style={{ top: yOf(bands, m) }} />
          ))}
          {bands.slice(0, -1).map((band, i) => (
            <span key={band.to} style={{ top: yOf(bands, band.to) + 4, height: BREAK_PX - 8 }}>
              No classes {clock(band.to)} to {clock(bands[i + 1].from)}
            </span>
          ))}
        </div>

        {days.map((d) => (
          <ul
            key={d.weekday}
            className="tt-col"
            style={{ height, gridColumn: d.weekday + 1 }}
            aria-label={`${WEEKDAY_LONG[d.weekday - 1]}${d.weekday === todayWeekday ? ", today" : ""}`}
            data-today={d.weekday === todayWeekday ? "" : undefined}
          >
            {d.placed.map((p) => {
              const f = fullness(p.batch);
              const mine = p.batch.instructor_id === myStaffId;
              // One short word for where: the branch when all branches are showing, the room otherwise.
              const place = showBranch ? p.batch.branch_name : p.batch.room_name;
              return (
                <li key={`${p.batch.id}-${p.slot.weekday}-${p.slot.starts_at}`}>
                  <TravelLink
                    kind="class"
                    id={p.batch.id}
                    label={p.batch.name}
                    href={`/classes/${p.batch.id}`}
                    className="tt-block"
                    data-full={f.full ? "" : undefined}
                    data-mine={mine ? "" : undefined}
                    aria-label={accessibleName(p.batch, p.slot, showBranch, mine)}
                    title={accessibleName(p.batch, p.slot, showBranch, mine)}
                    data-lanes={p.lanes > 2 ? "many" : p.lanes > 1 ? "two" : undefined}
                    style={{
                      top: yOf(bands, p.start) + 1,
                      height: (p.end - p.start) * PX_PER_MINUTE - 3,
                      left: `calc(${(p.lane / p.lanes) * 100}% + 2px)`,
                      width: `calc(${100 / p.lanes}% - 4px)`,
                    }}
                  >
                    {p.batch.capacity !== null ? (
                      <span className="tt-fill" style={{ height: `${f.ratio * 100}%` }} aria-hidden="true" />
                    ) : null}
                    {/* Most important first: a line that doesn't fit the
                        block's height is left out whole, never cut through. */}
                    <span className="tt-meta tt-time">
                      {p.slot.starts_at}
                      {p.lanes > 1 ? null : <span className="tt-end">–{p.slot.ends_at}</span>}
                    </span>
                    <span className="tt-name">{p.batch.name}</span>
                    {p.lanes > 2 ? null : (
                      <span className="tt-meta">
                        {f.full ? "Full" : p.lanes > 1 && p.batch.capacity !== null ? `${p.batch.enrolled}/${p.batch.capacity}` : f.label}
                      </span>
                    )}
                    {p.lanes > 1 || !(mine || place) ? null : <span className="tt-meta tt-where">{mine ? "You teach" : place}</span>}
                  </TravelLink>
                </li>
              );
            })}
          </ul>
        ))}
      </div>

      <Agenda days={days} todayWeekday={todayWeekday} myStaffId={myStaffId} showBranch={showBranch} />
    </>
  );
}

/** The same week as a list, for phones. Starts from today. */
function Agenda({
  days,
  todayWeekday,
  myStaffId,
  showBranch,
}: {
  days: { weekday: number; placed: Placed[] }[];
  todayWeekday: number | null;
  myStaffId: string;
  showBranch: boolean;
}) {
  const start = todayWeekday ?? 1;
  const ordered = [...days.slice(start - 1), ...days.slice(0, start - 1)].filter((d) => d.placed.length > 0);
  return (
    <div className="tt-agenda">
      {ordered.map((d) => (
        <section key={d.weekday} aria-labelledby={`day-${d.weekday}`}>
          <h2 id={`day-${d.weekday}`} className="h-card mb-2">
            {WEEKDAY_LONG[d.weekday - 1]}
            {d.weekday === todayWeekday ? <span className="tag tag-good ml-2 align-middle">Today</span> : null}
          </h2>
          <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
            {[...d.placed]
              .sort((a, b) => a.start - b.start)
              .map((p) => {
                const f = fullness(p.batch);
                const mine = p.batch.instructor_id === myStaffId;
                return (
                  <li key={`${p.batch.id}-${p.slot.starts_at}`}>
                    <TravelLink
                      kind="class"
                      id={p.batch.id}
                      label={p.batch.name}
                      href={`/classes/${p.batch.id}`}
                      className="row flex flex-col gap-1 px-4 py-3 no-underline"
                      style={{ color: "inherit", boxShadow: mine ? "inset 3px 0 0 var(--accent)" : undefined }}
                      aria-label={accessibleName(p.batch, p.slot, showBranch, mine)}
                    >
                      <span className="flex items-center justify-between gap-3">
                        <span className="text-sm font-bold tabular-nums">
                          {p.slot.starts_at}–{p.slot.ends_at}
                        </span>
                        <span className="flex items-center gap-2 text-[13px] font-semibold">
                          <span className={f.full ? "attn-text" : undefined}>{f.full ? "Full" : f.label}</span>
                          {p.batch.capacity !== null ? (
                            <span className="meter w-[64px]" data-full={f.full ? "" : undefined} aria-hidden="true">
                              <span style={{ width: `${f.ratio * 100}%` }} />
                            </span>
                          ) : null}
                        </span>
                      </span>
                      <span className="list-row-title">{p.batch.name}</span>
                      <span className="muted text-[13px]">
                        {[showBranch ? p.batch.branch_name : null, p.batch.room_name, mine ? "You teach" : p.batch.instructor_name]
                          .filter(Boolean)
                          .join(" · ")}
                      </span>
                    </TravelLink>
                  </li>
                );
              })}
          </ul>
        </section>
      ))}
    </div>
  );
}
