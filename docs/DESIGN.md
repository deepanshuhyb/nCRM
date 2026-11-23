# ncrm design language

The dashboard should feel like a recital: warm stage light glowing
through frosted glass. Calm enough to use all day, with one bold
thing per screen.

Mockup: the "Studio CRM — owner home dashboard" design canvas (desktop and
phone artboards).

## Colour

There are three themes. **Recital** is the default; the studio (or each
user) can switch. No blue, and no yellow or lime, in any of them.

Every theme has the same roles:

| Role | Recital | Daylight | Emerald |
|---|---|---|---|
| Background | `#1F0E1C` wine | `#FBF5F2` blush white | `#0B1A16` deep green |
| Text | `#FFF3F0` | `#2A1726` | `#F1FAF6` |
| Secondary text | `#D4B8C4` | `#6B5363` | `#A9C4BA` |
| Good / primary action | `#7FE0B4` mint, dark text | `#0E7C5B` emerald, white text | `#8BEBC1` mint, dark text |
| Needs attention | `#FF8A7A` coral (text `#FFA89B`) | `#E5574C` coral (text `#B4232F`) | `#FF9EB8` rose (text `#FFB3C8`) |
| Background lights | coral `#FF7A6B`, magenta `#E85AAE`, peach `#FFB48A` | peach `#FFB199`, pink `#FF8FC0`, mint `#7FE0B4` | emerald `#2ED39A`, pink `#FF8FB1`, peach `#FFB48A` |
| Solid fallback for glass | `#3A1A33` | `#FFFFFF` | `#17322B` |

Colour always carries meaning: the action colour means fine or act here,
the attention colour means needs attention. Nothing else is coloured.

Attendance scale (heatmaps) runs dark to bright in the dark themes and
light to dark in Daylight, so it reads without hue. Cells under 65% also
get a 2px ring so they stand out for colour-blind users.

## Type

- **Unbounded** (600) for headlines and big numbers. Wide and rounded.
- **Figtree** (400–700) for everything else.
- Numbers use `font-variant-numeric: tabular-nums`.
- Rupees in Indian grouping: `Intl.NumberFormat('en-IN')` (₹1,24,500).
  Short form uses lakh: ₹1.24L.
- Sentence case everywhere. No all-caps labels.

## Glass

Three strengths, so depth shows hierarchy:

| Level | Fill | Border | Radius |
|---|---|---|---|
| Hero (one per screen) | white 18%→5% gradient over the theme's dark tint at 40%, blur 30px (Daylight: white 88%→58%) | white 26% | 36px |
| Panel / tile | white 10%→3% over the dark tint at 55%, blur 28px (Daylight: white 78%→52%) | white 13% | 28–30px |
| Row inside a panel | dark tint at 50%, no blur (Daylight: white 72%) | white 9% | 18–22px |

Every glass surface has an inner top highlight
(`inset 0 1px 0 rgba(255,255,255,.14–.30)`) and `saturate(140–150%)`.

Rules:
- Glass needs light behind it: 3–4 large blurred colour fields from the
  theme's background lights, at 20–55% opacity, behind the content.
- Text never sits on raw background light; it always sits on smoked glass.
- Under `prefers-reduced-transparency: reduce`, glass becomes the theme's
  solid fallback with no blur.
- Do not nest blurred surfaces. Rows inside a panel use a tint, not blur.

## Layout

- Bento grid where block size means importance. The hero block holds
  what needs action today.
- Every chart title is the finding in plain words ("Hip-hop Beginners is
  sliding."), with the chart's description underneath.
- Every insight has the action next to it (Send reminders, Message).
- Touch targets at least 44px. Mobile-first: every screen works at 390px.
- Phone navigation is a floating glass pill at the bottom.

## Copy

Plain, short, from the owner's point of view. Buttons name what happens
("Send reminders", not "Submit"), and the same action keeps the same name
everywhere.
