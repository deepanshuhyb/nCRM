"use client";

import Link from "next/link";
import { useParams } from "next/navigation";
import { ViewTransition, type ComponentProps, type ReactNode } from "react";

/**
 * A name that travels: tap a student in a list and their name moves to
 * where the profile's title will be, instead of the page being swapped
 * under you.
 *
 * How: the list row, the loading skeleton and the real page each wrap the
 * name in <ViewTransition name="student-<id>">. The browser animates an
 * element between two pages when both carry the same name.
 *
 * The skeleton is the hard part: it appears before the server has answered,
 * so it doesn't know the name. The link remembers the label it showed
 * (below), and the skeleton reads it back. On a direct page load there is
 * nothing remembered and the skeleton shows a grey bar.
 */
const remembered = { key: "", label: "" };

export function travelName(kind: string, id: string) {
  return `${kind}-${id}`;
}

/** A link whose label travels to the next page's title. */
export function TravelLink({
  kind,
  id,
  label,
  children,
  ...link
}: { kind: string; id: string; label: string; children: ReactNode } & Omit<ComponentProps<typeof Link>, "children" | "onClick">) {
  return (
    <Link
      {...link}
      onClick={() => {
        remembered.key = travelName(kind, id);
        remembered.label = label;
      }}
    >
      {children}
    </Link>
  );
}

/** Wraps the text that should travel (in a row, or a page title). */
export function Travels({ kind, id, children }: { kind: string; id: string; children: ReactNode }) {
  return (
    <ViewTransition name={travelName(kind, id)} share="travel" default="none">
      {children}
    </ViewTransition>
  );
}

/** The title slot of a loading skeleton for /…/[id] pages. */
export function TravelTitleSkeleton({ kind }: { kind: string }) {
  const params = useParams<{ id?: string }>();
  const id = typeof params.id === "string" ? params.id : "";
  const label = remembered.key === travelName(kind, id) ? remembered.label : null;
  return (
    <Travels kind={kind} id={id}>
      {label ? (
        <h1 className="h-page">{label}</h1>
      ) : (
        <span className="skel" style={{ height: 40, width: "min(420px, 70%)" }} />
      )}
    </Travels>
  );
}
