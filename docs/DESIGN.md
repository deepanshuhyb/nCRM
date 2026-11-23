# ncrm design language

A calm page with one live thing on it. Every screen answers one question
at the top (how is attendance, what is left today, how much has come in),
in one inverted block that nothing else on the page competes with.
Everything below it is quiet: solid panels, a real type scale, the same
two meaning colours. Movement only answers something the person did.

The studio is still in it: the teacher's count ("5, 6, 7, 8") sits behind
sign-in and Home, and the display face is used the way a studio uses a
poster, for titles and numbers only.

This replaces the first design (glass panels over the count everywhere).
`docs/UI_PLAN.md` records why: every panel blurred the page behind it, so
scrolling stuttered, and with one surface for everything nothing stood
out. `docs/NOTES.md` has the measurements.

## Themes

Two themes on `<html data-theme>`, chosen with the sun/moon button and
remembered in the `ncrm_theme` cookie (read on the server, so there is no
flash). All colours are CSS variables at the top of `globals.css`.

| | Daylight | Emerald |
|---|---|---|
| Page | warm off-white `#f8f2ee` | deep green `#0a1713` |
| Panel | white | `#10241d` |
| Ink | plum `#2a1726` | `#f1faf6` |
| The inverted block | plum, white text | pale mint `#d6f3e5`, dark text |
| Fine / act here | emerald `#0e7c5b` | mint `#8bebc1` |
| Needs attention | coral `#e5574c` | `#ff9a8a` |

Two colours carry meaning and nothing else does: green is "fine, or act
here", coral is "needs attention". Coral always comes with a word ("Full",
"Overdue", "3 not marked") because green and coral are hard to tell apart
for colour-blind readers; the palette check in `docs/NOTES.md` shows the
numbers.

## Type

Two families, self-hosted. Unbounded (wide, heavy) for page titles and
big numbers; Figtree for everything else. The steps are far enough apart
to see without thinking:

| Class | Size | Use |
|---|---|---|
| `.h-page` | 40px (30 on phones), Unbounded | The page's title. One per page |
| `.num` | 30 to 72px, Unbounded, tabular figures | The number a block is about |
| `.h-section` | 22px, Figtree 800 | A section inside a page |
| `.h-card` | 17px, Figtree 700 | A panel's title, a row's name |
| body | 15px | |
| `.caption` | 13px, muted | Supporting detail |

Section titles are deliberately not in the display face: at 20px it read
as a label, not a heading, which is what "headlines don't look like
headers" was about. A graph's title is a sentence that states the finding
("2 classes are full"), with what is plotted in a caption under it.

## Surfaces

Three kinds, and only the last one blurs.

- **Page**: the flat background.
- **Panel** (`.panel`): solid, hairline border, soft shadow, 22px radius.
  Rows inside a panel (`.row`) are a slightly tinted fill with a 14px
  radius: radius shrinks with depth. `.panel-attn` tints a panel coral
  when its content needs attention.
- **The inverted block** (`.hero`): the theme turned inside out (plum on
  the warm page, pale mint on the dark one). One per screen, at the top,
  holding the screen's answer: a big number, a sentence, a graph.
- **Float** (`.float`): translucent with a 14px blur. Only for things that
  sit over moving content: the phone's bottom bar and the sign-in card.
  Under `prefers-reduced-transparency`, or where blur is unsupported, it
  becomes solid.

The count behind the page (`<Counts />`) appears on sign-in only. It was
on Home too, and showed through the gaps between panels and behind the
sidebar; a dashboard has enough on it. It is a fixed layer with paint
containment, so it is drawn once.

**Rows.** A row has at most three lines: the name, then one or two
captions. When the name is a link, give it the class `stretch` and the
whole row becomes the tap target; buttons in the row keep working (see
NOTES, "Rows that are tappable anywhere"). A status tag appears only when
it tells you something the list's filter doesn't.

## Layout

- Desktop (901px and up): a sidebar with icon and label per page, the
  studio's name, studio switcher, sign out and theme. The page is at most
  1240px wide.
- Phones: a slim top bar, and a floating bottom bar with four pages and
  "More". Every screen works at 390px with no sideways scrolling.
  Nothing is smaller than 12px, and anything you tap is at least 44px
  tall. Filters (`.segmented`) stay on one line and scroll sideways
  instead of wrapping. Figures (`Stat`) sit three across in compact tiles.
  `docs/UI_AUDIT.md` has the checks and how to run them.
- A page is `<Page>` (from `components/ui.tsx`) with a `PageHeader`
  (where you came from, title, one line, actions), then the inverted
  block, then figures (`Stat`), then lists. The most important thing is
  top-left.
- Detail pages with a lot to them use tabs (`components/tabs.tsx`): the
  student profile is Classes, Fees, Attendance, Details. All panels are
  rendered on the server; switching is instant and the address's
  `#fragment` remembers the tab.
- Icons are drawn in `components/icons.tsx` on one 24px grid: no icon
  package, no third-party origin.

## Motion

A system, not effects. Three durations and one curve, as CSS variables:
`--fast` 120ms (press, hover), `--base` 200ms (open, swap), `--slow`
320ms (page change, graphs drawing in), `--ease`
`cubic-bezier(0.2, 0, 0, 1)`. Nothing is slower than 320ms. Only
`transform` and `opacity` are animated, so nothing causes layout.

- **Opening a page**: the route's `loading.tsx` skeleton appears at once
  (the page's shape in grey), then the content fades in and rises 8px.
  Old content leaves in 120ms. This is React's `<ViewTransition>` driving
  the browser's View Transitions API; no animation library.
- **A name that travels**: tap a student in a list and their name moves
  to the profile's title position immediately, before the server has
  answered. `components/travel.tsx` explains how.
- **Graphs** draw in once per page load: bars grow from the baseline.
  That is the one thing on a screen that moves by itself.
- **Rows that appear later** (a drop-in, a payment) fade in; lists opt in
  with `data-live`.
- **Tabs and disclosures** fade their content in.
- `prefers-reduced-motion: reduce` turns all of it off.

Judge motion on the built app (`./scripts/dev.sh --fast`), not in
development mode, where each page is compiled on first visit.

## Graphs

Built from plain elements on the server (`components/graphs.tsx`), like
the timetable: no chart library, no script, and they print.

- Each graph answers one question someone already asks, and its title is
  the answer. A figure appears only if someone acts on it.
- Marks: bars at most 24px wide with a 4px rounded top and a square foot
  on the baseline, 2px between neighbours, a hairline baseline, no grid.
  Labels are text-coloured, never the mark's colour.
- Hover, tap or focus on a mark shows what it stands for (`data-tip`, in
  CSS). Each graph also carries a spoken summary and a screen-reader
  table of its numbers.
- Colour is the same everywhere: green fine, coral needs attention (with
  a word), grey neutral. Earlier periods are a lighter step of the same
  green, never a second hue.
- Where two things are compared over time (joined and left), one goes up
  from the line and the other down, so position tells them apart, not
  colour.

| Screen | Graph | Question |
|---|---|---|
| Home | Share who came, each day for four weeks | Are people coming? |
| Home, Fees | Money received each day this month, against what fell due | Is the money coming in? |
| Home | Classes by places taken | Which classes need attention? |
| Home | Joined and left per month | Are we growing? |
| Today | The day as a strip of classes, filled once marked | What's left today? |
| Fees | What's owed by how late it is | How old is the debt? |
| Class page | Came and away at the last eight meetings | Is this class slipping? |
| Student page | A mark per class attended or missed | Is this student slipping? |

## The timetable

The classes page's one bold element. The week runs across, time runs
down, one hour is 84px. Two or more hours with no class anywhere in the
week are drawn as one short strip ("No classes 12:00 to 16:00"), so the
hours that have classes get the room. Each class is a row-tinted block inside one panel
whose fill rises from the bottom as places are
taken, in the "fine" tint; a full class takes the attention tint and
border. The fill has no top edge: a line there reads as a strikethrough
under the name. Classes at the same time sit side by side in lanes. An
instructor's own classes carry an accent edge on the left and say "You
teach". Today's column has a faint tint and a dark day pill. Below 900px
the grid becomes a list by day, starting today, with a small meter per
class. The whole thing is server-rendered: no JavaScript.

A block's lines are in order of importance: time, name, how full, where.
A line that doesn't fit is left out whole, never cut through (see NOTES,
"The timetable drops whole lines"). Classes sharing an hour show the start
time, the name and a short count, and open to the full width of the day on
hover or keyboard focus. A day that has them gets a wider column (1.6
shares of the width for two side by side, 2.2 for three).

Every block is a link whose accessible name says the class, day, time,
place, teacher and how full it is, and the same text is the hover title,
since narrow blocks leave things out.

## Marking attendance

Marking happens standing up, on a phone, with a class waiting. So each
student gets four buttons side by side (Present, Late, Absent, Excused),
each at least 44px tall, and the chosen one fills in: the "fine" colour
for Present and Late, the attention colour for Absent, a neutral dark for
Excused. Colour is never the only signal: the chosen button also carries
`aria-pressed` and the count in the heading changes ("14 of 15 came").

"Everyone's here" marks whoever is still blank as present, because in most
classes most people come; staff then tap the two or three exceptions.
Nothing is sent until Save. The save bar sticks above the bottom
navigation and says what Save will do ("3 to save", "2 not marked yet",
"Saved."). It is solid: it floats over names and they must not show
through.

Notes under a name are short and only appear when they matter: "Pays per
class, 3 credits left", "No credits left: this class will be owed" (in
the attention colour, before saving, so the front desk can sell a pack on
the spot), "Missed the last 2".

On the Today page each class is a row with its time, a state tag (Marked,
"3 not marked" once the class is over, Not started, Cancelled) and a
meter of marks. An instructor's own classes carry the same accent edge as
on the timetable.

## Money

Amounts are in the display face with tabular figures, formatted the
Indian way (₹1,85,000). A due amount is neutral; it takes the attention
colour only once it is overdue. "Paid" is the quiet good tag. Cancelled
entries stay visible, struck through, with the reason beside them:
nothing about money disappears from the screen any more than from the
database.

The payment form says what will happen before Save ("₹1,500 will still
be due", "the extra is kept as advance") and the button repeats the
amount ("Record ₹1,000"). A receipt is drawn as paper: a solid card with
no blur, so the print stylesheet only has to hide the shell.

## Backdated dates

A join date or class start set earlier than the day it was entered carries
a quiet "Backdated" tag next to the date, with "Entered on …" as its hover
title and screen-reader text. It's information, not a warning, so it uses
the quiet tag, never the attention colour. Forms show the tag live as soon
as a past date is picked, so staff see what they're doing.

## Styles and Tailwind

Our classes (`.panel`, `.row`, `.btn`, `.field`, `.tag`, …) live in
`@layer components` in `globals.css`, so a Tailwind utility on the same
element (`px-0`, `py-3`, `m-0`) wins. Put new component classes there too.
See the cascade-layer note in `docs/NOTES.md`.

## Copy

Plain, short, from the owner's point of view. Buttons name what happens
("Send reminders", not "Submit"), and the same action keeps the same name
everywhere.
