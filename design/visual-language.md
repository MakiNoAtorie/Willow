# Willow visual language

The reference every phase builds against. **Paper** is the default theme and the one Phases 2–7 build; **Desk** and **Poster** arrive in Phase 8 (see the spec's Themes section).

Status: Paper drafted, awaiting owner approval. Desk and Poster values still to fill in (their round-2 look is in `prototype-visual-language/candidates/`).

## Where things are

- `mockups/themes.js` holds the exact token values. This document explains them; when the two disagree, `themes.js` wins.
- `mockups/mockups.html`: every static state. `?screen=<id>&theme=paper&os=mac|win`. Open it directly in a browser; it needs a network connection for the Google Fonts.
- `mockups/motion.html`: every animation, looping.
- `mockups/png/<theme>/<screen>.png`: a 1280×820 image of each state, for comparison without opening the HTML. Regenerate them with `mockups/render.sh [theme] [os]` (needs Google Chrome).
- Mockups are reference only. Nothing in `design/` ships.

| Screen | Shows |
|---|---|
| `mac-window` | macOS window, transparent title bar, menu bar with Edit open, "Edited" title |
| `mac-view-menu` | View menu with the Theme submenu |
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
| Shape text | Literata (OFL), bundled | 15 px / 1.5, weight 400 | Bold 700, italic. Justified, no hyphenation |
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
- Title at the top-left, 18 px in and 13 px down, in Literata 13/600.

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

### Proposals beyond the spec (owner to confirm)

- Zone titles stay readable when zoomed out: they never drop below 12 px on screen, and unlike shape text they don't fade. This keeps acts and episodes findable in the low-zoom view.
- Fills deepen toward the picker color as text fades (above), so the willow silhouette holds at low zoom.
- The color change ripples along the chain instead of switching all at once.
- The code-panel toggle sits in the title bar on both platforms (the spec says "a small toggle on the title bar"; this fixes its icon and place).

## Desk

To fill: same headings as Paper.

## Poster

To fill: same headings as Paper.
