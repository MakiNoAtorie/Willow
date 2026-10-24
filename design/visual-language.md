# Willow visual language

The reference every phase builds against. **Paper** is the default theme and the one Phases 2–7 build; **Desk** and **Poster** arrive in Phase 8 (see the spec's Themes section).

Status: approved by the owner on 2026-10-24, closing Phase 1. Desk and Poster below list only what differs from Paper; anything not mentioned works as in Paper.

## Where things are

- `mockups/themes.js` holds the exact token values. This document explains them; when the two disagree, `themes.js` wins.
- `mockups/mockups.html`: every static state. `?screen=<id>&theme=paper&os=mac|win`. Open it directly in a browser; it needs a network connection for the Google Fonts.
- `mockups/motion.html`: every animation, looping. `?theme=` as above.
- `mockups/png/<theme>/<screen>.png`: a 1280×820 image of each state in each theme (`paper`, `desk`, `poster`), for comparison without opening the HTML. Regenerate them with `mockups/render.sh [theme] [os]` (needs Google Chrome).
- Mockups are reference only. Nothing in `design/` ships.

| Screen | Shows |
|---|---|
| `mac-window` | macOS window, transparent title bar, menu bar with Edit open, "Edited" title |
| `mac-view-menu` | View menu with the Theme submenu |
| `mac-text-align` | View > Text Alignment, with Left chosen and applied |
| `align-centered` | the canvas with Centered text |
| `win-window` | Windows custom title bar, unsaved dot, menu button open with Edit cascade |
| `canvas` | trunk and branches, selected shape, shape being edited (caret, bold, italic), midpoint circles on hover, hovered connector, resize cursor |
| `menu-shape`, `menu-multi`, `menu-connector`, `menu-empty`, `menu-zone` | the five context menus |
| `marquee` | Shift+drag marquee mid-drag, with shapes inside already selected |
| `picker` | directional picker (Tab) on the selected shape |
| `zones` | two top-level zones and one nested zone, with titles |
| `zoom-pill` | zoom indicator |
| `low-zoom-fading`, `low-zoom` | text fading out at 62%; colored blocks at 27% |
| `code-panel` | code panel open, one invalid line underlined, caret in the panel |

## Paper

### Typography

| Use | Font | Size / line height | Notes |
|---|---|---|---|
| Shape text | Literata (OFL), bundled | 15 px / 1.5, weight 400 | Bold 700, italic. Justified by default (Left or Centered via View > Text Alignment), no hyphenation |
| Zone titles | Literata | 13 px, weight 600 | Never drawn smaller than 12 px on screen (see Zoom) |
| Code panel | JetBrains Mono (OFL), bundled | 12.5 px / 1.65 | |
| Menus, title bar, zoom pill | system UI font (SF on macOS, Segoe UI Variable on Windows) | 13 px (menus), 13 / 12.5 px (title) | Native feel for chrome |

### Colors

Names are what the file format writes and never change. Pastel goes on shapes and zones; picker is the saturated circle in menus.

| Name | Pastel | Picker |
|---|---|---|
| neutral | `#e9e5dd` | `#a39b8f` |
| rose | `#f3d8d3` | `#d9776c` |
| peach | `#f6dfc8` | `#e3955a` |
| butter | `#f2e8c0` | `#d2ad3a` |
| sage | `#d9e5cf` | `#7ea468` |
| sky | `#d3e0ea` | `#5d8fbf` |
| lilac | `#e2d8ea` | `#9479b8` |

| Role | Value |
|---|---|
| Canvas | `#f6f3ec` with a dot grid (1 px dots, `rgba(70,50,20,.10)`, every 24 canvas units) |
| Ink (text) | `#3b342c` |
| Accent (selection, hover, midpoints) | `#8b7760` |
| Text selection | `rgba(139,119,96,.22)` |

### Shapes

- Default width 210 canvas units (shared by every theme). Padding 13 top/bottom, 16 left/right. Corner radius 14.
- No border. Shadow: `0 1px 2px rgba(70,50,20,.07), 0 4px 14px rgba(70,50,20,.08)`.
- **Selected** (and being edited): a 2 px canvas-colored gap, then a 2 px accent ring, with a slightly deeper shadow (`0 6px 18px rgba(70,50,20,.12)`). Editing adds the caret: 1.5 px wide, ink color, blinking.
- **Hover**: four midpoint circles at the edge midpoints, 9 px, white fill, 1.5 px accent stroke.
- **Resize**: the system east-west resize cursor near the side edges.

### Connectors

- Straight, no arrowheads, from the facing side's midpoint to the other shape's. Drawn under shapes.
- 2 px `#cbbfae`, round caps. Hovered: 3 px accent `#8b7760`.
- Hit tolerance: 8 px either side of the line, on screen.
- Never thinner than 1 screen pixel at low zoom.

### Zones

- Corner radius 22. Fill: the zone's pastel mixed 45% into the canvas (nested zones 70%, so they read as deeper). Edge: 1 px inset, pastel mixed 35% toward its picker color.
- Title at the top-left, 16 px in and 12 px down (constant on screen), in Literata 13/600.

### Selection, marquee and picker

- Multi-selection uses the same ring as single selection, on every selected shape.
- Marquee: 1 px `rgba(139,119,96,.65)` border, `rgba(139,119,96,.08)` fill, radius 3.
- Directional picker: four 28 px keycaps (↑ → ↓ ←), 30 px out from each edge midpoint, joined to the shape by a short dashed stub. Keycap: `#fffdf8`, radius 8, a hairline outline and a small drop shadow.

### Menus

- Background `#fffdf8`, radius 10, item radius 6, padding 6. Hovered item `#f1ebe0`. Shadow: `0 0 0 1px rgba(70,50,20,.08), 0 10px 30px rgba(70,50,20,.18)`.
- System UI font 13 px, with shortcuts at 45% opacity. A destructive item (zone Delete) is `#b5463b`.
- Color rows: seven 18 px circles in picker colors, 7 px apart. The current color has a 1.5 px ring outside a 2 px gap.
- macOS uses the native menu bar and menus. Windows draws these same app menus from the menu button.

### Window chrome

- **macOS**: traffic lights over a transparent title bar, document name centered at 55% ink ("— Edited" when unsaved). The code-panel toggle (a panel icon) sits at the right of the title bar.
- **Windows**: a 34 px title bar in the canvas color. From the left: the menu button (≡) and the document name, with a 6 px dot when unsaved. Then on the right: the code-panel toggle, minimize, maximize and close (46 px each).

### Code panel

- 420 px wide, on the right, background `#fbf9f4`, with a 1 px hairline on its left. Starts below the title bar.
- Syntax colors: keywords `#8b7760`, IDs ink at weight 600, strings `#5f7d4c`, direction words `#8a6fae`, `@layout` lines `#a39b8f`, line numbers `#c9bfb0`.
- Invalid line: a 1 px wavy underline in `#d9776c` plus a faint `rgba(217,119,108,.06)` line tint. No popups.

### Zoom

| | Value |
|---|---|
| Minimum / maximum | 10% / 400% |
| Text fully visible | ≥ 75% |
| Text gone | ≤ 50% (15 px × 0.5 = 7.5 px, above the 7 px floor of ADR 0003) |
| Between | text opacity falls linearly; shape fills deepen up to 30% toward their picker color, so the low-zoom silhouette reads as colored blocks |
| Below 50% | shadows and the dot grid are dropped |
| Zoom pill | bottom center, `rgba(59,52,44,.84)` background, `#fffdf8` text in system 12/600. Shows on each zoom step, holds 700 ms, fades over 400 ms |

### Motion

| Animation | Duration and easing |
|---|---|
| Create a linked shape (connector draws, shape rises 8 px and scales from 96%) | 160 ms `cubic-bezier(.2,.8,.2,1)` |
| Insert-and-push (the pushed subtree slides) | 220 ms `cubic-bezier(.2,.8,.2,1)` |
| Color change (whole chain; ripples out from the changed shape at 40 ms per step) | 200 ms ease-out |
| Select / deselect ring | 120 ms ease-out |
| Wheel zoom step (retargets mid-animation) | 120 ms `cubic-bezier(.2,.8,.2,1)` |

Every animation ends its redraw loop when it finishes.

### Additions beyond the spec (approved with Phase 1)

- Zone titles stay readable when zoomed out: they never drop below 12 px on screen, and unlike shape text they don't fade. This keeps acts and episodes findable in the low-zoom view.
- Fills deepen toward the picker color as text fades (above), so the willow silhouette holds at low zoom.
- The color change ripples along the chain instead of switching all at once.
- The code-panel toggle sits in the title bar on both platforms (the spec says "a small toggle on the title bar"; this fixes its icon and place).

## Desk

Typewritten index cards on a linen desk.

| | Value |
|---|---|
| Shape text | Courier Prime (OFL), bundled, 14 px / 1.5. Also used for menus, zone titles and the code panel |
| Canvas | `#e6dfd1` with a fine linen weave, drawn at a fixed screen size |
| Ink / accent | `#2b2621` / `#2b2621` |
| Shapes | Radius 4, padding 12 top (under the tab), 13 bottom, 15 sides. The card body is the pastel, and a 6 px tab along the top edge is the picker color. Shadow: `0 1px 1px rgba(70,50,20,.18), 0 4px 10px rgba(70,50,20,.14)` |
| Selected | The card lifts 4 px, with a 1.5 px ink outline and a deep shadow (`0 16px 30px rgba(70,50,20,.28)`) |
| Midpoints | 10 px brass `#c79a3e` with a cream stroke |
| Connectors | 1.5 px `#9b8c74`; hovered 2.5 px ink |
| Zones | Radius 6. Fill: the picker color mixed 16% into the canvas (nested 30%). No border, a soft inset shadow. Title in uppercase, bold, letter-spaced |
| Marquee | 1 px dashed ink at 60% |
| Picker keycaps | Radius 3, paper-card shadow |
| Menus | `#fffdf7`, radius 4, hover `#f3ead6`. Color swatches are rounded squares (radius 3) |
| Zoom | Text gone at 52% (14 px × 0.52 = 7.3 px). Blocks deepen 60% toward the picker color, because the near-white cards vanish into the linen otherwise |
| Motion | As Paper, except select is 160 ms `cubic-bezier(.2,.8,.2,1)` so the lift reads |

| Name | Pastel (card) | Picker (tab, swatch) |
|---|---|---|
| neutral | `#fbfaf6` | `#b9b1a3` |
| rose | `#fff3f1` | `#d8574c` |
| peach | `#fff6ec` | `#e8873a` |
| butter | `#fffbea` | `#e2b52c` |
| sage | `#f5fbf1` | `#5c9d4d` |
| sky | `#f2f7fc` | `#4a84c4` |
| lilac | `#f8f4fc` | `#8a66c0` |

Accepted: a monospaced font plus justified text leaves wide gaps between words in narrow cards (see `png/desk/canvas.png`). View > Text Alignment > Left is the remedy (see `png/desk/mac-text-align.png`).

## Poster

Neo-brutalist: thick outlines, hard offset shadows, punchy pastels.

| | Value |
|---|---|
| Shape text | Space Grotesk (OFL), bundled, 14.5 px / 1.45, weight 500 (bold 700). Also used for menus and zone titles. The code panel keeps JetBrains Mono |
| Canvas | `#fbf6ea`, no texture |
| Ink / accent | `#111111`; a hovered connector turns `#ff4d1a` |
| Shapes | Radius 10, padding 12 / 14, a 2.5 px ink border, hard shadow `4px 4px 0 #111` |
| Selected | Moves up-left 3 px and the hard shadow grows to `8px 8px 0 #111` |
| Midpoints | 11 px white with a 2.5 px ink stroke |
| Connectors | 2.5 px ink, square caps; hovered 4.5 px `#ff4d1a` |
| Caret / text selection | 2 px ink / `#ffe066` |
| Zones | Radius 16, 2.5 px ink border, the pastel mixed 50% into the canvas (nested 85%). The title is a black pill with white uppercase text |
| Marquee | 2 px dashed ink, yellow fill `rgba(255,224,102,.25)` |
| Picker keycaps | White, radius 6, 2.5 px ink border, `3px 3px 0` hard shadow |
| Menus | White, radius 8, ink outline with a hard `5px 5px 0` shadow, hover `#ffe57a`. Swatches are 22 px rounded squares with an ink ring |
| Code panel | `#fffaf0`, a 2.5 px ink rule on its left |
| Zoom pill | Ink background, `#ffe57a` text |
| Zoom | Text gone at 50% (14.5 px × 0.5 = 7.25 px). Borders stay at low zoom, so blocks read as outlined tiles |
| Motion | Create and select overshoot slightly (`cubic-bezier(.3,1.4,.5,1)`, `cubic-bezier(.3,1.6,.5,1)`, 100 ms for select). Color change snaps in two steps (`steps(2, end)`, 160 ms). The zoom pill fades over 300 ms |

| Name | Pastel | Picker |
|---|---|---|
| neutral | `#e9e7e1` | `#8f8b82` |
| rose | `#ffb8b0` | `#ff4f3f` |
| peach | `#ffc994` | `#ff8a1f` |
| butter | `#ffe57a` | `#ffc800` |
| sage | `#b6eda9` | `#2fbf4a` |
| sky | `#a9d6ff` | `#1f8bff` |
| lilac | `#d2bbff` | `#8b4dff` |
