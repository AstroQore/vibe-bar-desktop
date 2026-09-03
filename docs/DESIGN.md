# DESIGN.md — the porcelain language, as Desktop uses it

Desktop draws the native app's visual language — one flat language, three
surfaces, density not material — at Workbench scale. The native spec is
`vibe-bar/docs/DESIGN.md`; this file is what the Desktop pages share and
where a page must not invent its own. The tokens live in
`apps/desktop/src/workbench/porcelain.css` (`--wb-*`) and
`apps/desktop/src/tokens.ts` (provider accents, generated from the
contract); the shared components are the `.wb-*` classes in that same
stylesheet. A page composes them; it does not restyle them.

## Surfaces

Fill plus a 0.5 px hairline, never a drop shadow, material or gradient
(charts excepted). Selection is a fill change. Anything that genuinely sits
over content — a menu panel, a toast — is opaque (`--wb-overlay`), and a
menu panel alone adds a soft 24 px shadow at 10 % because it floats.
Vibrant windows lower every fill (`html.vibrant`) and let the desktop
through.

| Token | Light | Dark |
| --- | --- | --- |
| `--wb-window` | 249 250 252 · 96 % | 23 24 30 · 96 % |
| `--wb-sidebar` | white · 58 % | white · 3.5 % |
| `--wb-toolbar` | white · 74 % | white · 4.6 % |
| `--wb-field` | black · 4.5 % | black · 27 % |
| `--wb-card` | 242 242 247 · 60 % | 44 44 46 · 60 % |
| `--wb-selected` | white · 94 % | white · 10 % |
| `--wb-hover` | black · 4.5 % | white · 6.5 % |
| `--wb-hairline` | black · 8.5 % | white · 9 % |
| `--wb-overlay` | `#fcfcfe` | `#24252c` |
| `--wb-accent` | `#4E5FE0` | `#4E5FE0` |

Provider hues come from one table (`PROVIDER_ACCENT`) and a provider is the
same hue everywhere: glyph, chip tint, selected row, chart series. A tint is
the hue at 10–16 % over the surface with a 36–45 % hairline.

## Type

SF Pro Text; numbers that change are tabular. 22/26 bold −0.35 page header ·
15/19 semibold session or card title · 14 semibold card title at regular
density · 13 semibold row title, 13 medium sidebar nav and body · 12.5/18
message body and field text · 12 semibold secondary section caption · 11.5
subtitle, pill value, button · 11/14 secondary row summary, 11 mono code ·
10.5 tertiary metadata, counts, table headers (uppercase, +0.3) · 10
semibold +0.5 uppercase eyebrow, role caption, pill key.

## Controls — one height, one radius

| Piece | Class | Size |
| --- | --- | --- |
| Pill (button, filter, menu anchor) | `.wb-pill`, `.on`, `.prominent`, `.danger` | 26 px, radius 13, 11.5 semibold |
| Filter pill key | `.wb-pill-key` | 10 px uppercase eyebrow before the value |
| Capsule (model, count, id, status word — never a button) | `.wb-capsule`, `.mono`, `.tint`, `.tall` | 16 px (18 tall), radius 8, 10.5 |
| Field | `.wb-field` | 30 px, radius 10; 1 px accent ring at 60 % when focused |
| Icon button | `.wb-iconbtn` | 28 px round on the toolbar fill |
| Segmented | 26 outer on the field fill, radius 9; 22 inner, radius 7, active solid accent |
| Switch | 30×18, accent when on |
| Code | `.wb-code`, `.quiet` | mono 11/16, radius 9, field fill |
| Tool name | `.wb-toolname` | accent capsule, 18 px |
| Metadata line | `.wb-meta`, `.wb-dot` | 10.5 secondary, 3 px dots between items |

There are no 24 px chips and no 8 px button radius: every page uses the same
pill. Icons are stroke on a 16 px grid at 1.5 px in the secondary colour;
provider glyphs are filled, 18 px in rows and 26 px in headers. No emoji.

## Patterns

- **Page header**: title and 11.5 subtitle on the left, status text and
  round icon buttons on the right, 14 px below.
- **Toolbar**: one 50 px row on the toolbar fill, radius 14 — a field, then
  pills, the count at the far right. It never wraps; what does not fit goes
  behind an icon-only pill's menu.
- **List row**: radius 13, padding 10 / 11, an 18 px provider glyph, then a
  13 semibold title, one 11 summary line, a metadata line. Hover is the
  hover fill; selected is the provider hue at 10 % with a 40 % hairline. A
  count only when it is known.
- **Card**: radius 16, 16 px padding, card fill with a 40 % separator
  hairline, 10 px between children; caption above is 12 semibold secondary.
- **Table**: 10.5 uppercase tertiary headers on a 0.7 px rule, 12 px rows on
  0.5 px rules, numbers right-aligned and tabular.
- **Transcript message**: radius 14, a 3 px role bar (accent for you, 18 %
  ink for the assistant, 10 % for tools), a 10 px uppercase role caption
  with the time. A tool call is its name in an accent capsule, its purpose,
  and its arguments as fields or a code block; a result is a code block
  that folds after eight lines.
- **Empty state**: a 26 px light glyph, a 14 semibold line, an 11.5
  secondary line under 320 px, centred in the space it replaces.

## Where it stands

Codified on the Sessions page first (this file's first commit). Settings,
Usage Stats, Resets and Skills still carry their own chip, button and
segmented rules (`st-*`, `us-chip`, `rs-*`, `sk-*`) and move over one page
at a time; a page that moves drops its own rules rather than keeping both.
