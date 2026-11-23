import type { ReactNode } from "react";

/**
 * The dashboard's icons, drawn here so no third-party origin or package is
 * involved (see the CSP rule in CLAUDE.md). All share one 24px grid and one
 * stroke; they inherit the text colour.
 */
const PATHS = {
  home: <path d="M4 11.2 12 4l8 7.2V20h-5.2v-5.4H9.2V20H4z" />,
  today: (
    <>
      <rect x="4" y="5" width="16" height="15" rx="3" />
      <path d="M8 3v4M16 3v4M4 10h16M9 15l2 2 4-4" />
    </>
  ),
  classes: (
    <>
      <rect x="4" y="4" width="7" height="9" rx="2" />
      <rect x="13" y="4" width="7" height="5" rx="2" />
      <rect x="13" y="11" width="7" height="9" rx="2" />
      <rect x="4" y="15" width="7" height="5" rx="2" />
    </>
  ),
  students: (
    <>
      <circle cx="9" cy="8.2" r="3.2" />
      <path d="M3.5 19.5c.6-3.2 2.8-5 5.5-5s4.9 1.8 5.5 5" />
      <circle cx="17" cy="9.2" r="2.4" />
      <path d="M16.6 14.7c2.1.3 3.5 1.8 4 4.3" />
    </>
  ),
  fees: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M9 8h6M9 11h6M10 8c3.6 0 3.6 6 0 6H9l4.4 4" />
    </>
  ),
  inbox: (
    <>
      <path d="M5 5.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-7.5L7 21v-3.5H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2Z" />
      <path d="M8 10h8M8 13h5" />
    </>
  ),
  team: (
    <>
      <rect x="4" y="5" width="16" height="14" rx="3" />
      <circle cx="9.5" cy="10.8" r="2" />
      <path d="M6.8 15.9c.5-1.5 1.5-2.2 2.7-2.2s2.2.7 2.7 2.2M14.6 10h2.9M14.6 13.5h2.9" />
    </>
  ),
  branches: (
    <>
      <path d="M12 21s-6.5-5.6-6.5-10.5a6.5 6.5 0 0 1 13 0C18.5 15.4 12 21 12 21z" />
      <circle cx="12" cy="10.5" r="2.3" />
    </>
  ),
  more: (
    <>
      <circle cx="6" cy="12" r="1.3" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.3" fill="currentColor" stroke="none" />
      <circle cx="18" cy="12" r="1.3" fill="currentColor" stroke="none" />
    </>
  ),
  back: <path d="M14.5 6 8.5 12l6 6" />,
  out: <path d="M10 5H6.5A1.5 1.5 0 0 0 5 6.5v11A1.5 1.5 0 0 0 6.5 19H10M14 8l4 4-4 4M18 12H9.5" />,
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>
  ),
  moon: <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  open: <path d="M7 17 17 7M9 7h8v8" />,
  check: <path d="m5 12.5 4.5 4.5L19 7.5" />,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 20 }: { name: IconName; size?: number }) {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ flex: "none" }}
    >
      {PATHS[name]}
    </svg>
  );
}
