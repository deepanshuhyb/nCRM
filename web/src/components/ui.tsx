import Link from "next/link";
import { ViewTransition, type ReactNode } from "react";


/**
 * The shared parts every screen is built from, so headings, spacing and
 * motion can't drift between pages. See docs/DESIGN.md.
 */

/** A page's content. Arrives with a short fade and rise; leaves fast. */
export function Page({ children, narrow }: { children: ReactNode; narrow?: boolean }) {
  return (
    <ViewTransition enter="arrive" exit="leave" default="none">
      <div className="page" style={narrow ? { maxWidth: 860, marginInline: "auto", width: "100%" } : undefined}>
        {children}
      </div>
    </ViewTransition>
  );
}

/** What a route shows while its data loads. Same motion, mirrored. */
export function LoadingPage({ children, narrow }: { children: ReactNode; narrow?: boolean }) {
  return (
    <ViewTransition enter="swap" exit="leave" default="none">
      <div className="page" aria-busy="true" style={narrow ? { maxWidth: 860, marginInline: "auto", width: "100%" } : undefined}>
        <span className="sr-only" role="status">
          Loading
        </span>
        {children}
      </div>
    </ViewTransition>
  );
}

/** The top of a page: where you came from, the title, one line, actions. */
export function PageHeader({
  title,
  lead,
  back,
  actions,
  children,
}: {
  /** A string, or a node when the title travels (see travel.tsx). */
  title: ReactNode;
  lead?: ReactNode;
  back?: { href: string; label: string };
  actions?: ReactNode;
  /** Tags or status shown beside the title. */
  children?: ReactNode;
}) {
  return (
    <header>
      {back ? (
        <Link href={back.href} className="back-link">
          {back.label}
        </Link>
      ) : null}
      <div className="page-head">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
            {typeof title === "string" ? <h1 className="h-page">{title}</h1> : title}
            {children}
          </div>
          {lead ? <p className="lead">{lead}</p> : null}
        </div>
        {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
      </div>
    </header>
  );
}

/** A section's title with its count or note on the same line. */
export function SectionHeader({
  id,
  title,
  aside,
  action,
}: {
  id?: string;
  title: ReactNode;
  aside?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="section-head">
      <h2 id={id} className="h-section">
        {title}
      </h2>
      {aside || action ? (
        <div className="flex flex-wrap items-center gap-3">
          {aside ? <span className="aside">{aside}</span> : null}
          {action}
        </div>
      ) : null}
    </div>
  );
}

/** One figure: what it is, the number, and what it means. */
export function Stat({
  label,
  value,
  note,
  tone,
  href,
  children,
}: {
  label: string;
  value: ReactNode;
  note?: ReactNode;
  tone?: "good" | "attn";
  href?: string;
  /** A small graph under the number. */
  children?: ReactNode;
}) {
  const body = (
    <>
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {children}
      {note ? (
        <span className="stat-note" data-tone={tone}>
          {note}
        </span>
      ) : null}
    </>
  );
  return href ? (
    <Link href={href} className="panel stat">
      {body}
    </Link>
  ) : (
    <div className="panel stat">{body}</div>
  );
}

/** An empty list is an invitation: say what goes here and how to start. */
export function EmptyState({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <p className="h-card">{title}</p>
      {children ? <p className="soft m-0 max-w-[60ch]">{children}</p> : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}

/** Grey blocks in the shape of a list, for loading states. */
export function SkeletonRows({ rows = 6, height = 58 }: { rows?: number; height?: number }) {
  return (
    <div className="flex flex-col gap-1.5">
      {Array.from({ length: rows }, (_, i) => (
        <span key={i} className="skel" style={{ height, borderRadius: 14 }} />
      ))}
    </div>
  );
}

/** The four giant counts behind sign-in and Home. */
export function Counts() {
  return (
    <div className="counts" aria-hidden="true">
      <span className="c5">5</span>
      <span className="c6">6</span>
      <span className="c7">7</span>
      <span className="c8">8</span>
    </div>
  );
}
