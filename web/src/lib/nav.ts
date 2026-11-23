import type { IconName } from "@/components/icons";
import type { Capability } from "@/lib/types";

export type NavItem = { href: string; label: string; icon: IconName };

/** How many items the phone's bottom bar shows before "More". */
export const BOTTOM_NAV_SLOTS = 4;

/**
 * The pages a role can open, in menu order. Hiding a page here is a
 * convenience; the API refuses what the role can't do anyway.
 */
export function navFor(capabilities: Capability[]): NavItem[] {
  const can = new Set(capabilities);
  const items: (NavItem | false)[] = [
    { href: "/", label: "Home", icon: "home" },
    { href: "/attendance", label: "Today", icon: "today" },
    can.has("manage_students") && { href: "/students", label: "Students", icon: "students" },
    can.has("manage_fees") && { href: "/fees", label: "Fees", icon: "fees" },
    can.has("use_inbox") && { href: "/inbox", label: "Inbox", icon: "inbox" },
    { href: "/classes", label: "Classes", icon: "classes" },
    can.has("manage_staff") && { href: "/team", label: "Team", icon: "team" },
    { href: "/branches", label: "Branches", icon: "branches" },
  ];
  return items.filter((i): i is NavItem => i !== false);
}
