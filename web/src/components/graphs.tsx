import Link from "next/link";
import type { ReactNode } from "react";

/**
 * The dashboard's graphs. Plain elements rendered on the server (like the
 * timetable): no chart library, no script, and they print.
 *
 * Every graph has:
 *  - a spoken summary (`label`) and a table of its numbers for screen
 *    readers, so nothing is told by sight alone;
 *  - a hover/tap/focus description on each mark (`tip`), done in CSS;
 *  - colours with one meaning everywhere: green is fine, coral needs
 *    attention (and always has a word beside it), grey is neutral.
 * The mark rules are written at the top of the graphs section of
 * globals.css; docs/DESIGN.md explains the choices.
 */

type Tone = "soft" | "attn" | "neutral" | "none";

export type Bar = {
  key: string;
  /** 0 to 1 of the graph's height. null: nothing to show for this slot. */
  value: number | null;
  /** A second segment stacked on top (0 to 1), drawn in neutral grey. */
  above?: number;
  tone?: Tone;
  tip: string;
  /** Extra space before this bar (a new week). */
  gap?: boolean;
};

function edge(i: number, n: number): "start" | "end" | undefined {
  if (n < 8) return undefined;
  return i < 3 ? "start" : i >= n - 3 ? "end" : undefined;
}

/** A table only screen readers meet: the graph's numbers as text. */
function Numbers({ caption, rows }: { caption: string; rows: [string, string][] }) {
  return (
    <table className="sr-only">
      <caption>{caption}</caption>
      <tbody>
        {rows.map(([name, value]) => (
          <tr key={name}>
            <th scope="row">{name}</th>
            <td>{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Bars over time. Values are already scaled to 0..1 by the caller. */
export function Bars({
  bars,
  label,
  height = 120,
  from,
  to,
}: {
  bars: Bar[];
  /** What the graph shows, in a sentence, for people who can't see it. */
  label: string;
  height?: number;
  /** Captions under the two ends of the axis. */
  from?: string;
  to?: string;
}) {
  return (
    <figure className="m-0">
      <div className="g-bars" style={{ ["--h" as string]: `${height}px` }} role="img" aria-label={label}>
        {bars.map((b, i) => (
          <span key={b.key} className="g-col" data-tip={b.tip} data-edge={edge(i, bars.length)} data-gap={b.gap ? "" : undefined} tabIndex={-1}>
            {b.value === null ? (
              <i data-tone="none" style={{ ["--i" as string]: i }} />
            ) : (
              <>
                <i data-tone={b.tone} style={{ height: `${Math.max(0, Math.min(1, b.value)) * 100}%`, ["--i" as string]: i }} />
                {b.above ? <i data-tone="neutral" style={{ height: `${Math.min(1, b.above) * 100}%`, ["--i" as string]: i }} /> : null}
              </>
            )}
          </span>
        ))}
      </div>
      {from || to ? (
        <div className="g-axis" aria-hidden="true">
          <span>{from}</span>
          <span>{to}</span>
        </div>
      ) : null}
      <Numbers caption={label} rows={bars.map((b) => [b.key, b.tip])} />
    </figure>
  );
}

/** A thin meter: how much of a whole. */
export function Meter({ value, tone, index = 0 }: { value: number; tone?: "attn" | "neutral"; index?: number }) {
  return (
    <span className="g-meter" aria-hidden="true">
      <i data-tone={tone} style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%`, ["--i" as string]: index }} />
    </span>
  );
}

export type RankedRow = {
  key: string;
  name: ReactNode;
  /** Shown at the right, in text: the number itself. */
  value: ReactNode;
  /** 0 to 1. */
  share: number;
  tone?: "attn" | "neutral";
  href?: string;
};

/** Named rows, each with its number and a meter: for comparing a handful. */
export function Ranked({ rows, label }: { rows: RankedRow[]; label: string }) {
  return (
    <ul className="m-0 flex list-none flex-col gap-3 p-0" aria-label={label}>
      {rows.map((r, i) => (
        <li key={r.key} className="flex flex-col gap-1.5">
          <span className="flex items-baseline justify-between gap-3 text-sm">
            <span className="min-w-0 truncate font-semibold">{r.href ? <Link href={r.href}>{r.name}</Link> : r.name}</span>
            <span className="shrink-0 font-semibold tabular-nums">{r.value}</span>
          </span>
          <Meter value={r.share} tone={r.tone} index={i} />
        </li>
      ))}
    </ul>
  );
}

export type FlowMonth = { key: string; label: string; up: number; down: number };

/** Two things per month on one line: up for one, down for the other. */
export function Flow({
  months,
  upName,
  downName,
  label,
  height = 64,
}: {
  months: FlowMonth[];
  upName: string;
  downName: string;
  label: string;
  height?: number;
}) {
  const max = Math.max(1, ...months.map((m) => Math.max(m.up, m.down)));
  return (
    <figure className="m-0">
      <p className="caption m-0 mb-2 flex gap-4 font-semibold" aria-hidden="true">
        <span>
          <i className="g-swatch" />
          {upName}
        </span>
        <span>
          <i className="g-swatch" data-tone="neutral" />
          {downName}
        </span>
      </p>
      <div className="g-flow" style={{ ["--h" as string]: `${height}px` }} role="img" aria-label={label}>
        {months.map((m, i) => (
          <div key={m.key} className="g-flow-col" data-tip={`${m.label}: ${m.up} ${upName.toLowerCase()}, ${m.down} ${downName.toLowerCase()}`} tabIndex={-1}>
            <div className="g-flow-up">{m.up > 0 ? <i style={{ height: `${(m.up / max) * 100}%`, ["--i" as string]: i }} /> : null}</div>
            <div className="g-flow-down">{m.down > 0 ? <i style={{ height: `${(m.down / max) * 100}%`, ["--i" as string]: i }} /> : null}</div>
            <span className="caption font-semibold" aria-hidden="true">
              {m.label}
            </span>
          </div>
        ))}
      </div>
      <Numbers caption={label} rows={months.map((m) => [m.label, `${m.up} ${upName.toLowerCase()}, ${m.down} ${downName.toLowerCase()}`])} />
    </figure>
  );
}

export type Dot = { key: string; tone?: "late" | "attn" | "neutral"; tip: string };

/** One mark per event, oldest first: a pattern you can see at a glance. */
export function Dots({ dots, label }: { dots: Dot[]; label: string }) {
  return (
    <figure className="m-0">
      <div className="g-dots" role="img" aria-label={label}>
        {dots.map((d, i) => (
          <span key={d.key} className="g-dot" data-tone={d.tone} data-tip={d.tip} data-edge={i < 4 ? "start" : undefined} tabIndex={-1} />
        ))}
      </div>
      <Numbers caption={label} rows={dots.map((d) => [d.key, d.tip])} />
    </figure>
  );
}
