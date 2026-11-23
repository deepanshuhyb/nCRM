# Dashboard overhaul plan

Written 2026-11-23 after the owner's review: movement looks choppy,
headings don't read as headings, every screen looks the same. He wants it
animated without jitter, distinctive, and built around graphs with the
right amount of information: not sparse, not crowded.

This is the plan. `docs/DESIGN.md` now describes what was built.

## Status (2026-11-23): built

All five steps are done. Where the result differs from the plan below:

- The block at the top of each screen is solid and inverted (plum, or
  pale mint in the dark theme), not glass. Blur is left only on the
  phone's bottom bar and the sign-in card.
- A name travels forward (list to page) but going back is instant, with
  no animation: the browser restores the old page outside a transition.
- Class names don't travel from the timetable, because a class appears
  there once per weekly slot and a travelling name must be unique.
- Rows fade in when added; removed rows simply go. Numbers don't tick
  over when they change.
- Team, Branches, prices and the forms were moved onto the new parts
  (headers, panels, type) but not redesigned beyond that.

Measured scrolling before and after is in `docs/NOTES.md`.

## What is wrong today, and why

| Symptom | Cause in our code |
|---|---|
| Pages freeze, then pop in whole | Every page waits for the API on the server and there is no `loading.tsx` anywhere, so the old page sits there until the new one is complete. Nothing connects the two. |
| Scrolling stutters | Four to seven `.glass` panels per page each blur what is behind them, live, over four giant fixed numerals. Each scroll frame re-blurs every panel. Blur over scrolling content is a well-known cause of jank. |
| Slow first visit to each page | `scripts/dev.sh` runs `next dev`, which compiles each page the first time it's opened. The built app doesn't. |
| Headings don't look like headings | Page title 32px, section title 20px, row title 16px bold: three sizes within a few pixels, one weight, and the wide display face at 20px reads as a label. No fixed space above a heading. |
| Everything looks the same | One surface (glass), one radius, one padding, one row style for every piece of content. Nothing says "this is the main thing". |
| No graphs | Counts only. The data for trends exists (attendance, payments, class fill) and isn't drawn. |

## Direction: calm page, one live thing per screen

Keep what is ours (warm daylight / deep emerald themes, the two meaning
colours, the display face for numbers). Change the structure.

1. **Three levels of surface instead of one.**
   - *Page*: flat background.
   - *Panel*: solid, soft border, no blur. Most content lives here.
   - *Glass*: only for things that float over content (top bar, phone
     bottom bar, save bar) and at most one hero per screen.
   That takes live blurs from 4–7 per screen to 2–3 small ones, which is
   the scrolling fix.
2. **The giant numerals** stay on sign-in and Home only, drawn once on a
   layer that never repaints.
3. **A type scale with real steps.** Page title 40/44 (30 on phones) in
   the display face. Section title 22, text face, bold, with 32px above
   it and its count or action on the same line. Card title 17. Body 15.
   Caption 13. Big numbers 44–56 in the display face with tabular
   figures. One `PageHeader` and one `SectionHeader` component used
   everywhere so it can't drift.
4. **Every screen has one answer at the top.** A headline sentence or
   number, up to three supporting figures, one graph, then the list.
   Most important thing top-left.
5. **A left sidebar on desktop** (icons with labels) instead of the pill
   row: there are seven pages now and WhatsApp and Reports are coming.
   The phone keeps its bottom bar.
6. **Icons**: one small set, inline SVG in the repo (no third-party
   origin, so no CSP change).

## Motion: a system, not effects

- Tokens: `--fast: 120ms` (press, hover), `--base: 200ms` (open, close,
  swap), `--slow: 320ms` (page change, graph draw-in). One curve,
  ease-out `cubic-bezier(0.2, 0, 0, 1)`. Nothing over 320ms.
- Only `transform` and `opacity` move. No animated height, shadow, blur
  or layout.
- **Page changes** use React's `<ViewTransition>` (built into React 19.3
  and the Next.js 16 App Router, no library): a short crossfade for
  content, and shared elements that travel: a class's name from its
  timetable block to its page title, a student's name from the list row
  to the profile, a stat tile into the page it opens.
- **Loading**: a `loading.tsx` skeleton per route, the same shape as the
  page, so a tap answers within a frame and the content fades into place
  with no jump.
- **In place**: rows added or removed fade and slide 8px; saving shows on
  the button; numbers that change tick over once.
- **One orchestrated moment per screen**: graphs draw in once on load
  (bars grow from the baseline, lines trace). Nothing else moves by
  itself.
- `prefers-reduced-motion`: everything becomes a plain fade or nothing.
- `scripts/dev.sh --fast` builds and runs the production app, for
  judging smoothness. Scroll and tap responsiveness are measured on the
  built app before and after (Chrome performance trace, INP).

## Graphs: few, small, each answering a question

Hand-drawn SVG, rendered on the server like the timetable: no chart
library, no extra JavaScript, themeable, printable. Every graph has a
title that states the finding, direct labels instead of a legend, a
text alternative, and opens the list behind it when tapped. Colour
means the same thing everywhere: accent = fine, coral = needs attention,
ink = neutral data.

| Screen | Graph | The question it answers |
|---|---|---|
| Home | Attendance, last 4 weeks: one bar per day, this week solid | Are people coming? |
| Home | Fees this month: collected against charged, with a small daily line | Is the money coming in? |
| Home | Classes by how full: sorted bars, full ones in coral | Which classes need attention? |
| Home | Students: joined and left per month, last 6 | Are we growing? |
| Today | A strip of the day: classes as blocks on a timeline, marked ones filled | What's left to do today? |
| Class page | Attendance per meeting, last 8 | Is this class slipping? |
| Student page | A dot per class for 8 weeks (came, late, missed) | Is this student slipping? |
| Fees | Collected per day this month, and dues by how late | When does money arrive, how old is the debt? |

Density rule: a figure appears only if someone acts on it. Each graph
replaces a number that was there, it isn't added on top.

This needs small aggregate endpoints (`/reports/attendance`,
`/reports/collection`, `/reports/classes`, `/reports/students`), read
through `TenantTx`, with the same role rules as the lists they
summarise. It is the start of the analytics phase in `docs/PLAN.md`.

## Screen by screen

- **Home**: the dashboard above. The setup checklist shrinks to a single
  line once everything is done.
- **Today**: day strip, then classes as cards that show state at a
  glance; marking keeps its big buttons, gains row animation and a
  visible "saved" state per row.
- **Classes**: the timetable stays the bold element; cleaner blocks,
  shared-element move into the class page.
- **Students**: denser list (name, class, fees state, last seen) with a
  status dot; profile reorganised as header, three stat tiles, then tabs
  (Classes, Fees, Attendance, Details) instead of one long page.
- **Fees**: collection graph, ageing of dues, then the list.
- **Team, Branches, sign-in**: restyled with the new parts.

## Order of work

1. **Foundation**: tokens (type, space, surface, motion), the blur diet,
   `PageHeader` / `SectionHeader` / `Panel` / `Stat` / `Row` /
   `EmptyState`, sidebar, icons, `loading.tsx` skeletons, view
   transitions, `dev.sh --fast`. Before/after performance numbers.
2. **Graph kit** (bar, line, strip, ring, as server components) and the
   four report endpoints with tests.
3. **Home**, shown to the owner before the rest.
4. **Today, Fees, Students, Classes**, then the small pages.
5. **Polish**: empty, loading and error states everywhere; keyboard;
   390px; both themes' contrast; reduced motion and transparency;
   `docs/DESIGN.md` rewritten; `docs/NOTES.md` entries for view
   transitions and the blur finding.

Each step ships with the usual checks (lint, typecheck, tests, build) and
screenshots at 1400px and 390px in both themes.

## Open choices for the owner

1. Left sidebar on desktop (recommended), or keep the top pill row?
2. Giant background numerals only on Home and sign-in (recommended), or
   everywhere, or gone?
3. Student profile as tabs (recommended) or one long page?
