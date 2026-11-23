# UI audit and improvement plan

Written 23 Nov 2026, after the owner tested the dashboard and reported "a
lot of issues" with readability, responsiveness and clutter. This is what
was tested, every problem found, what was done about each, and what is
still open. `docs/DESIGN.md` describes the design as it now stands; this
file is the record of the audit.

## How it was tested

The built app (`pnpm build && pnpm start`, the same thing
`./scripts/dev.sh --fast` runs) against the demo studio, in a real browser
engine (Chromium, driven by a script):

- **Every screen**: Home, Today, marking a class, Students, a student's
  four tabs, Add a student, Import, Fees, a receipt, Classes, a class, Add
  a class, Prices and packs, Team, Branches, More, Sign in.
- **Three widths**: phone (390px), tablet (820px), desktop (1400px).
- **Both themes**: Daylight and Emerald.
- **Each role**: owner (desktop), manager (phone), front desk (tablet),
  instructor (phone and desktop).
- **Measured on every screen**: sideways scrolling, anything wider than
  the window, text under 12px, text cut off, contrast under 4.5:1 (3:1 for
  large text), tap targets under 40px on touch widths, bars overlapping,
  browser console errors.
- **Looked at**: every screenshot, for clutter and hierarchy, which no
  script can measure.
- **Walked through on a phone**: add a student (including submitting the
  empty form), put them in a class, take a payment and open the receipt,
  switch profile tabs and open a tab from a link, mark a class and the
  teacher, add a class, search, add a room, open a team member's editor,
  switch theme, sign out.

## What was wrong, and what was done

### Broken

| # | Problem | Where | Fix |
|---|---------|-------|-----|
| 1 | **The page scrolled sideways.** Home was 562px wide in a 390px window (1464px in a 1400px one). | Home, Fees, a class, a student's Attendance tab; phone and desktop | Every graph carries a hidden table of its numbers for screen readers. The "visually hidden" style doesn't work on a `<table>` (tables ignore the width and clipping it sets), so the table was laid out at full size off to the side. It now sits inside a hidden wrapper. |
| 2 | **After saving anything on a student's Fees or Details tab, the page jumped back to the Classes tab**, so a payment's receipt appeared on a tab you were no longer looking at. | Student profile | The tab is remembered in the address (`#fees`). The code changed the address in a way the framework ignores, so the next refresh restored the old one. See NOTES, "Tabs forgot themselves after a save". |
| 3 | **Names in the "who owes" list broke into one word per line** and collided with the amount and a button. | Fees, phone | Each student is now one tappable row: name, "Overdue since 5 Nov · Hauz Khas", amount. Tapping opens their Fees tab. The separate "Take payment" button per row is gone. |
| 4 | **The last tab was cut off** ("Details" showed as "D") with no hint that the bar scrolls. | Student profile, phone | The four tabs share the width evenly on a phone; the count badge on Classes is hidden there. |
| 5 | **Timetable text was cut off**, even times ("18:00–19:15"). | Classes, desktop | See "The timetable" below. |

### Hard to read or tap

| # | Problem | Where | Fix |
|---|---------|-------|-----|
| 6 | Bottom bar labels were 11px. | Phone, tablet | 12px, the smallest size anywhere now. |
| 7 | Back links were 36px tall; names inside rows were 23px-tall links. | Everywhere on touch widths | Back links are 44px. A row whose name is a link is now tappable anywhere on the row (`.stretch`), and the row's own buttons stay on top. |
| 8 | "Done" blocks in the day strip were 4.4:1 against their text. | Today, Emerald | The green is darker (`--hero-good: #0b6b4f`). |
| 9 | A missing name on a long form was only reported next to the button, a screen away from the field. | Add a student, Add a class | The field that needs fixing is focused, which scrolls it into view. The message stays by the button. |
| 10 | The empty "Needs you" told instructors "no fee is overdue", though they can't see fees. | Home | The sentence only mentions fees to people who can see them. |

### Cluttered

| # | Problem | Where | Fix |
|---|---------|-------|-----|
| 11 | **Giant numerals showed through the gaps** between panels and behind the sidebar. | Home | Removed from Home. They stay on sign-in, where nothing sits on top of them. This reverses a choice made in `UI_PLAN.md`; putting them back is one line in `app/(app)/page.tsx`. |
| 12 | Three full-width figure tiles pushed a student's actual content off the first screen. | Student profile, phone | The three figures sit side by side in compact tiles. |
| 13 | Rows were three and four lines tall, with details wrapping around a button. | A student's classes, a class's students | Name and "Take out" share the first line; the details get the full width below. |
| 14 | Student list rows were tall, and every row said "Active" on a list already filtered to active. | Students | Two lines on a phone (name; contact and number), columns on a desktop. The status only shows when it says something (on a break, left, or the "Everyone" view). |
| 15 | The branch filter wrapped onto two lines. | Today, Classes, Fees; phone | It stays on one line and scrolls sideways if a studio has many branches. "All branches" reads "All" on a phone. |
| 16 | An "add a room" form was open under every branch. | Branches | One "Add a room" line per branch that opens the form. It starts open only when a branch has no rooms. |
| 17 | The teacher block took three stacked controls above the roster. | Marking a class | One line: who taught, then "Teacher was here" / "Was absent". The chosen one is filled in; the separate "Was here" tag is gone. |
| 18 | **Every student and class said "Backdated".** | Demo data only | The demo seeder moved joining dates into the past without moving the date they were entered. It now does both, for seeded people only. Run the dev script with your number again to tidy an existing demo studio. |
| 19 | Add a student and Add a class had no way back except the browser's. | Both forms | A back link above the title, like every other inner page. |

### The timetable (desktop)

The week grid drew every hour from the first class to the last. A studio
with a 7am class and evening classes got eight empty hours in the middle,
and the blocks that mattered were too small for their text.

- **Empty stretches collapse.** Two or more hours with no class anywhere
  in the week are drawn as one short strip: "No classes 12:00 to 16:00".
- **An hour is taller** (84px, was 72px).
- **Blocks say less, in order of importance:** time, name (up to two
  lines), how full, then where (the branch when all branches are showing,
  otherwise the room). A line that doesn't fit the block's height is left
  out whole; nothing is sliced through the middle.
- **Classes that share an hour** sit side by side and are narrow. They
  show the start time, the name and a short count ("15/16"). Pointing at
  one, or reaching it with the keyboard, opens it to the full width of the
  day.
- On a narrow desktop window the end time and the place are dropped
  before anything is cut.

## Result

Measured again on all 19 screens × 3 widths × 2 themes:

| Check | Before | After |
|-------|--------|-------|
| Screens that scroll sideways | 4 | 0 |
| Text under 12px | bottom bar on every screen | none |
| Contrast failures | 1 (Emerald day strip) | 0 |
| Tap targets under 40px (touch widths) | back links and row links on most screens | 0 |
| Text cut off with "…" | the timetable's times, counts and places | 0 |
| Console errors | 0 | 0 |

The phone walk-through passes end to end, including staying on the Fees
tab after a payment.

## Running the checks again

The scripts are in `scripts/ui-audit/` (Python with Playwright; a
developer's tool, not part of CI). Start the app with its output in a
file, because sign-in codes are printed there:

```sh
./scripts/dev.sh --fast 9599195718 "Deepanshu" 2>&1 | tee /tmp/ncrm-dev.log
```

Then, in another terminal:

```sh
pip install playwright && playwright install chromium   # once
python3 scripts/ui-audit/audit.py 9599195718 shots   # every screen, measured; screenshots in /tmp/ncrm-audit
python3 scripts/ui-audit/flows.py                    # the phone walk-through
```

`flows.py` adds a test student, class, room and payment to the demo
studio each time. Both scripts clear the dev database's sign-in code
limit (five an hour) before they start, and neither should ever be
pointed at a real studio.

## Still open

Not done here; listed so they aren't forgotten.

1. **Side-by-side timetable blocks can still shorten a long name** (it
   wraps to three lines, then stops). Hover or keyboard focus shows it in
   full; on a touch-only wide screen the only way to read it is to open
   the class. A studio with three or more rooms busy at once would be
   better served by a per-branch default than by narrower lanes.
2. **Marking is still one row per student with four buttons**, about 85px
   each. Fifteen students is two phone screens. It is easy to tap and hard
   to shrink without making the buttons smaller than a thumb; "Everyone's
   here" covers the common case.
3. **Dates in forms use the browser's own date picker**, which shows
   `mm/dd/yyyy` on a browser set to US English. On a phone or laptop set
   to India it shows `dd/mm/yyyy`. Everywhere else the app writes dates
   out ("23 Nov 2026").
4. **Removed rows don't animate out**, and back navigation doesn't
   animate (both noted in `UI_PLAN.md`).
5. **Tablets (up to 900px wide) get the phone layout** with a bottom bar,
   stretched. It works and nothing overflows, but a 820px screen has room
   for the sidebar.
6. **Not tested:** real phones (only an emulated 390px Chromium), Safari
   and Firefox, a screen reader, and 200% zoom.
