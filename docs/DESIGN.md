# ncrm design language

The dashboard should feel like a dance studio at night: soft stage light
glowing through frosted glass. Calm enough to use all day, with one bold
thing per screen.

Mockup: the "Studio CRM — owner home dashboard" design canvas (desktop and
phone artboards).

## Colour

| Token | Hex | Use |
|---|---|---|
| `stage` | `#15122B` | Page background. Never pure black. |
| `curtain` | `#241E45` | Solid fallback for glass (reduced transparency). |
| `spotlight` | `#F6F3FF` | Main text, primary pill on dark. |
| `haze` | `#B9B3D6` | Secondary text. Passes 4.5:1 on smoked glass. |
| `teal` | `#8EE3D8` | Good, done, primary action. Dark text on it. |
| `rose` | `#FF9EB8` | Needs attention: overdue, absent, at risk. Dark text on it. |
| `amber` | `#F5C98A` | Background light only. Never on text or controls. |
| `lilac` | `#A99BFF` | Background light only. |

Colour always carries meaning: teal means fine or act here, rose means
needs attention. Nothing else is coloured. No yellow or lime in the UI.

Attendance scale (heatmaps), dark to bright so it reads without hue:
`#FF9EB8` under 65%, `#3A3566` 65–74, `#2F6B70` 75–84, `#5FB8B0` 85–92,
`#8EE3D8` 93 and up.

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
| Hero (one per screen) | white 18%→5% gradient over `rgba(36,30,69,.40)`, blur 30px | white 24% | 36px |
| Panel / tile | white 10%→3% over `rgba(28,23,58,.50)`, blur 28px | white 12% | 28–30px |
| Row inside a panel | `rgba(15,12,32,.48)`, no blur | white 8% | 18–22px |

Every glass surface has an inner top highlight
(`inset 0 1px 0 rgba(255,255,255,.14–.30)`) and `saturate(140–150%)`.

Rules:
- Glass needs light behind it: 3–4 large blurred colour fields
  (rose, teal, amber, lilac) at 22–40% opacity, fixed behind the content.
- Text never sits on raw background light; it always sits on smoked glass.
- Under `prefers-reduced-transparency: reduce`, glass becomes solid
  `curtain` with no blur.
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
