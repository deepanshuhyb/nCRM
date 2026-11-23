import type { Capability } from "@/lib/types";

export type NavItem = { href: string; label: string };

/** How many items the phone's bottom bar shows before "More". */
export const BOTTOM_NAV_SLOTS = 4;

/**
 * The pages a role can open, in menu order. Hiding a page here is a
 * convenience; the API refuses what the role can't do anyway.
 */
export function navFor(capabilities: Capability[]): NavItem[] {
  const can = new Set(capabilities);
  return [
    { href: "/", label: "Home" },
    { href: "/attendance", label: "Today" },
    { href: "/classes", label: "Classes" },
    ...(can.has("manage_students") ? [{ href: "/students", label: "Students" }] : []),
    ...(can.has("manage_staff") ? [{ href: "/team", label: "Team" }] : []),
    { href: "/branches", label: "Branches" },
  ];
}
