# ncrm design language

The dashboard borrows from the dance studio itself: the teacher's count
("5, 6, 7, 8") behind everything, and reeded glass like a studio door.
Calm enough to use all day, with one bold thing per screen.

Mockup: the "Studio CRM — owner home dashboard" design canvas (desktop,
phone, and light and dark side by side).

## Themes

**Daylight** is the light theme and the default. **Emerald** is the dark
theme. No blue, and no yellow or lime, in either.

| Role | Daylight | Emerald |
|---|---|---|
| Background | `#FBF5F2` blush white | `#0B1A16` deep green |
| Text | `#2A1726` | `#F1FAF6` |
| Secondary text | `#6B5363` | `#A9C4BA` |
| Good / primary action | `#0E7C5B` emerald, white text | `#8BEBC1` mint, dark text |
| Needs attention | `#E5574C` coral (text `#B4232F`) | `#FF9A8A` coral (text `#FFB0A3`) |
| Count numerals behind the glass | coral `#FFB3A7`, mint `#9FE5C6`, pink `#FFC2D8`, peach `#FFCBB0` at 85% | coral `#FF9A8A`, green `#2ED39A`, pink `#FF8FB1`, `#1FA67A` at 42% |
| Solid fallback for glass | `#FFFFFF` | `#14302A` |

Colour always carries meaning: the action colour means fine or act here,
coral means needs attention. Nothing else is coloured.

Attendance scale (heatmaps) runs light to dark in Daylight and dark to
bright in Emerald, so it reads without hue. Cells under 65% also get a 2px
ring so they stand out for colour-blind users.

## Type

- **Unbounded** (600) for headlines, big numbers and the background count.
- **Figtree** (400–700) for everything else.
- Numbers use `font-variant-numeric: tabular-nums`.
- Rupees in Indian grouping: `Intl.NumberFormat('en-IN')` (₹1,24,500).
  Short form uses lakh: ₹1.24L.
- Sentence case everywhere. No all-caps labels.

## Behind the glass

Not colour blobs. The page background carries the count, "5 6 7 8", in
Unbounded at 700–780px (380–420px on a phone), one numeral per region,
each in a theme colour. Glass panels sit over them, so the numerals show
as soft shapes through the glass and as crisp edges in the gaps.

## Glass

| Surface | Fill | Blur | Radius |
|---|---|---|---|
| Hero (one per screen) | Reeded: vertical ribs, `repeating-linear-gradient(90deg, …)` 12–14px period, plus fine grain | 7px | 26–32px |
| Panel / tile | Flat translucent fill, no gradient (Daylight white 62%, Emerald `rgba(12,30,25,.64)`) | 16px | 24–28px |
| Row inside a panel | Near-solid (Daylight white 86%, Emerald `rgba(5,14,11,.62)`) | none | 12–18px |

What makes it read as real glass rather than a template:
- **Edge light:** 1px border, bright on top and left, darker on bottom and
  right, as if light hits the glass edge from the top-left.
- **Contact shadows:** a 1–2px shadow plus a tight negative-spread drop
  (`0 14px 28px -16px`), not a large soft glow.
- **Grain on the hero only:** an inline SVG `feTurbulence` overlay at low
  opacity (multiply in Daylight, screen in Emerald).
- **Less blur than usual** (7–16px), so what is behind keeps its shape.
- Do not nest blurred surfaces. Rows inside a panel use a fill, not blur.
- Under `prefers-reduced-transparency: reduce`, glass becomes the theme's
  solid fallback with no blur.

## Shapes

- Buttons and navigation items are rounded rectangles (10–14px), not pills.
- Small status tags stay fully round.
- Radius shrinks with depth: hero 32, panels 24–28, rows 12–18, chips 3–6.

## Layout

- Bento grid where block size means importance. The hero block holds
  what needs action today.
- Every chart title is the finding in plain words ("Hip-hop Beginners is
  sliding."), with the chart's description underneath.
- Every insight has the action next to it (Send reminders, Message).
- Touch targets at least 44px. Mobile-first: every screen works at 390px.
- Phone navigation is a glass bar at the bottom.

## Copy

Plain, short, from the owner's point of view. Buttons name what happens
("Send reminders", not "Submit"), and the same action keeps the same name
everywhere.
