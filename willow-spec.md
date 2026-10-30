# Willow — Build Spec

A minimal, polished desktop app for planning stories as diagrams. It is deliberately "just rounded shapes and lines": no feature creep. The writer builds a **plot trunk** (a vertical chain of shapes describing the evolving situation) and places **branches** (separate, unlinked chains for what characters do) beside it. From far away the layout reads like a willow tree, but the app knows nothing about trunks or branches. It only knows shapes, links, chains and zones.

## Stack and platform

- Rust + **GPUI** (Zed's UI framework). GPUI is not a stable published crate: pin a git revision and expect to pay for upgrades.
- Windows and macOS from day one; both must build and feel equally good. Bundle each theme's font so text renders identically on both; Phase 1 picks them, and Phase 0 checks emoji fallback on Windows.
- Light themes only (see Themes below).
- Memory and CPU efficiency matter: idle should be near-zero CPU, and large canvases must stay smooth. Animations must stop their redraw loop when they finish.
- Tests attach to one seam: a core crate with no GPUI dependency holding the document model, every editing command, the undo history and the text format. Tests drive commands and assert on the model and on saved text; the GPUI layer only turns input into commands and draws the model (ADR 0001). Shape height comes from text layout, which needs GPUI's text system, so the core takes a text-measuring function from the UI layer instead of measuring text itself (ADR 0003).
- Tests that need large or many input files build them with file generators rather than hand-written fixtures. Parser edge cases (escaping, invalid files) may use small golden files. Any benchmark or tool output should be CSV.
- Performance targets, measured on a generated 10,000-shape document: p95 frame time ≤ 8 ms while panning and zooming, draw ≤ 8 ms per keystroke while typing in a shape with the next vsync never missed (keystroke-to-paint is bounded by the display refresh, not draw cost; ADR 0003), idle CPU near zero within 1 s of the last input or animation. Reference machines: a MacBook Air M4, which must meet the targets, and a Windows desktop (Ryzen 5 9600X, RTX 5060 Ti), checked for correctness and feel. Optionally repeat the Windows run once on the 9600X's integrated graphics as a stand-in for a weak laptop. Timing targets hold within about 5% run-to-run noise: 8.1 ms passes an 8 ms target.

## Look and feel (fixed decisions)

- Polish is a core requirement, not a final step: crisp text, soft shadows, consistent spacing, short eased animations, high frame rate.
- Shapes are rounded rectangles. Connectors are **straight lines, no arrowheads**. Avoid anything that looks like spaghetti.
- Six pastel colors plus one neutral light gray. The neutral is the default for new unlinked shapes and for zones. In the picker all seven appear as **more saturated** circles so they're easy to tell apart. On shapes and zones they're applied as the pastel version.
- The full visual language is defined in Phase 1 and every later phase follows it.

## Themes

- Three themes, all light: **Paper** (the default), **Desk** and **Poster**, fixed in Phase 1. Paper is the one every phase builds and is reviewed against; Desk and Poster are added in Phase 8.
- A theme sets the look only: font and typography, the pastel and picker value of each of the seven colors, canvas background, shape fill, borders, corner radii, padding, shadows, connector stroke, selection, hover and midpoint styles, and menus. It never changes the model or the file. Color names are shared, so a chain saved as sage is sage in every theme.
- View > Theme switches it. The choice is an app preference, the same in every open window and remembered across launches; documents don't store it.
- Switching reflows. Each theme keeps its own font, so shapes keep their position, width and extra height, and their text is re-measured: a shape can grow or shrink, and neighbors make room through the spacing pass (Phase 8; before it, shapes may overlap). Spacing never changes placed positions, so the document stays saved, and the switch is not undoable.
- New shapes use one default width in every theme (Paper's), so the model never depends on the theme.

## Text alignment

- Shape text is justified by default. View > Text Alignment switches every shape to Justified, Left or Centered. In justified text the last line of a paragraph is left-aligned.
- Like the theme, it is an app preference: the same in every open window, remembered across launches, never stored in documents, and not undoable.
- Alignment only moves words within a line: line breaks, and so shape heights, are the same in all three, so switching never reflows.
- Justified text in narrow shapes can leave wide gaps between words, most of all in Desk's monospaced font. That is accepted; Left is the remedy.

## Window

- The canvas fills the window. No toolbar: every action lives in right-click menus and shortcuts.
- One window per document. Ctrl/Cmd+N opens a new window; several documents can be open at once.
- **macOS**: traffic lights over a transparent title bar, plus the native menu bar (File, Edit, View, and Window from Phase 8). The Edit menu carries Undo, Redo, Cut, Copy, Paste so the system shortcuts route correctly.
- **Windows**: a slim custom title bar with the document name, a quiet saved/unsaved dot, and one small menu button holding the same menus. It must behave like a native title bar: drag region, double-click to maximize, Windows 11 snap layouts on the maximize button, correct on mixed-DPI monitors. The owner verifies mixed DPI by hand with a second monitor at a different scale.
- A small toggle on the title bar shows/hides the code panel (Phase 7).
- While zooming, the zoom level appears briefly and fades.

## Core model

- **Shape**: rounded rectangle with rich text (plain, bold, italic), a position, a width and an optional extra height. Positions and sizes are whole canvas units; mouse drags at any zoom round to the nearest unit.
- **Link**: connects two shapes. Which side of each shape the connector leaves from (top, bottom, left, right) is **derived from the shapes' relative positions at draw time**, never stored: compare the two shapes' centers; if |dy| ≥ |dx| the link is vertical (above/below), otherwise horizontal, so an exact diagonal is vertical. So a link's direction follows the shapes when they move, and "the shape below b" always means the one that is actually below. Drawing, the saved text and story order all use this one rule, on drawn positions from Phase 8.
- **Chain**: the set of shapes reachable through links. A chain is a tree: a shape may have any number of links, but a link that would close a cycle is refused. An unlinked shape is a chain of one. **Color belongs to the chain**, not the shape; every shape in a chain shows the chain's color. When a chain splits (a removed link or a deleted shape), every resulting chain keeps the original color.
- **Zone**: a titled, colored container holding shapes. Membership is **explicit** (stored in the file), not geometric: a shape is in a zone because it was put there, and it stays a member even if it is later moved, grown or resized outside the zone's rectangle. Zones can nest one level deep. A chain may span zones.
- One canvas per file.

---

## Phase 0 — GPUI viability check

**Status: closed.** GPUI is the UI stack (ADR 0003).

Goal: confirm GPUI can carry this app before building on it. This is a throwaway spike, not the start of the real codebase.

Build the smallest thing that answers these questions, on both Windows and macOS:

- Can a canvas of rounded shapes with straight connectors be custom-drawn, panned and zoomed smoothly, including with a very large generated document?
- Can text inside a shape be laid out **justified**, with bold and italic mixed in, and edited live while the shape grows to fit? GPUI's text system has no justified alignment, so this means custom line layout; it is the most likely no-go point.
- Does in-shape editing work properly: caret, mouse text selection, IME input (test CJK and dead keys) on both platforms?
- Do memory use and idle CPU stay low, including after an animation has finished?
- Does GPUI's API make the planned interactions (hover midpoints, context menus, a directional picker, a side text panel, a custom title bar on Windows with snap layouts and mixed DPI) reasonable to build?

Record measurements as CSV. End with a short go/no-go note. If GPUI falls short, the fallback is the Linebender stack (winit + Vello + Parley); say which gaps would justify switching.

## Phase 1 — Visual language

**Status: closed 2026-10-24.** Approved result: `design/visual-language.md` and `design/mockups/` (PNGs in `design/mockups/png/`).

Goal: nail exactly how Willow looks before building any real UI. No Rust in this phase: recreate the app purely visually as SVG or HTML mockups, iterate on them with the owner, and record the result.

- Mock up every state the app has, as it would actually appear, in each of the three themes:
  - The window on both platforms (macOS title bar and menu bar, Windows custom title bar and menu button).
  - A canvas with a trunk and a couple of branches beside it, some colored.
  - A selected shape, a shape being edited (with caret, bold and italic text), hover midpoint circles, a hovered connector, and the resize cursor on side edges.
  - The shape context menu with the seven color circles, the multi-selection menu, the zone menu, the connector menu, and the empty-space menu.
  - The keyboard directional picker.
  - Zones, including one nested zone, with titles.
  - Multi-selection and the marquee.
  - The fading zoom indicator.
  - The low-zoom view, where text fades and shapes become colored blocks.
  - The code panel open beside the canvas, including an underlined invalid line.
- Where motion matters (shape creation, insert-and-push, color change, selection), HTML mockups can animate to show timing and easing.
- Everything shown must be achievable in GPUI; mockups are reference only and never ship.
- Present options where there's a real choice, and refine based on the owner's feedback until approved.
- Record the approved result in the repo as the visual reference for all later phases: the mockup files themselves, a PNG screenshot of every mocked-up state (so later phases and agents can compare against an image without opening the mockups), plus a visual-language document with the concrete values for each theme (colors for both picker and pastel, the bundled font and typography, sizes, corner radii, border and selection styles, spacing, shadows, connector stroke and hit tolerance, animation timings and easing, minimum and maximum zoom, and the zoom level where text fades). Text is not drawn below about 7 px on screen, so text must have fully faded by the zoom where it would drop under that size (ADR 0003).

Done when: the owner has approved the mockups and the visual-language document exists.

## Phase 2 — Canvas and shapes

Goal: an infinite canvas where shapes can be created, moved and written in, inside the final window, already looking like the Phase 1 mockups.

- Window chrome as described above, on both platforms.
- Drawing takes every visual value from one theme definition (Paper for now), so adding Desk and Poster in Phase 8 is mostly new values. What only they have (linen, card tabs, lifted selection, hard offset shadows, title pills) is new drawing code then.
- Infinite canvas: pan and zoom.
  - Plain left-drag on empty space pans.
  - Middle-drag always pans.
  - Holding Space turns left-drag into a pan anywhere, over shapes too.
  - Mouse wheel and two-finger trackpad scroll pan. Shift+wheel pans horizontally.
  - Ctrl/Cmd+wheel and pinch zoom, within the Phase 1 limits. Ctrl/Cmd+0 resets zoom.
  - Each wheel zoom step eases to its new level with a short animation instead of jumping; a step that arrives mid-animation retargets it. Pinch follows the fingers directly.
  - Panning only happens when nothing is being edited. Clicking empty space while editing first exits editing.
- A new document starts with one empty shape.
- Mouse on shapes, resolved so moving and text selection never fight:
  - Mouse-down on a shape that is **not** selected: releasing without dragging selects it (clear active border); dragging moves the shape instead.
  - Mouse-down on the **selected** shape: releasing without dragging places the caret where you clicked and enters editing; dragging moves the shape instead.
  - Mouse-down on the shape that **is** being edited: dragging selects text. To move it, press Esc first (it stays selected) and then drag.
- F2, or typing any character except Space, starts editing the selected shape with the caret at the end of its text (typing adds to the existing text).
- Enter or Esc leaves editing; the shape stays selected.
- Text is justified (see Text alignment). The shape grows vertically to fit text; width stays fixed. A single word wider than the shape is broken across lines rather than widening the shape. Line breaking follows UAX #14, so CJK text breaks between characters and is justified too, with kinsoku (no line starts with closing punctuation such as `。`) (ADR 0003).
- Width: dragging either side edge resizes both sides symmetrically around the center. The cursor changes to a resize arrow near side edges.
- Height: dragging the bottom edge extends the shape downward. Only the bottom edge resizes vertically.
- A shape can never be smaller than its text.
- Alt/Option+click autofits the shape to its text.
- Bold and italic with the usual shortcuts (Ctrl/Cmd+B, Ctrl/Cmd+I).
- New shapes always use the default size; they don't inherit width from anything.
- At low zoom, text fades out and shapes become plain colored blocks, so the overall silhouette reads cleanly.
- Undo/redo is part of the foundation from this phase on, and every later feature plugs into it. One history per document, covering model changes only: pan, zoom and selection are not undoable. Typing inside a shape coalesces into one step per editing session.

Done when: a generated 10,000-shape document meets the performance targets on the MacBook Air, and pans, zooms and edits correctly on Windows.

## Phase 3 — File format and persistence

Goal: documents save and reload, using a human-readable text format that will also be edited directly in Phase 7. Every later phase works on real, saved documents.

The chain, link and zone decisions in the core model are settled before this phase starts; the format encodes them and should not need to change in Phase 4 or 6.

### Model and drawing

- The core model gains links, chain color and zones now, even though the UI that creates them arrives in Phases 4 and 6. Color stays on each shape, with the invariant that every shape in a chain has the same color: a split keeps colors for free, a join repaints the adopted chain. Commands enforce the invariant and tests check it.
- The canvas draws connectors and zones read-only, exactly as the Phase 1 visual language defines them, so a file holding links and zones shows them. Nothing new is editable yet.
- Drawing order ("later on top") is not saved; after a load, shapes stack in story order. It only matters where shapes overlap, which they may until Phase 8.

### Format

- The text format is the save file and covers the full core model (shapes, links, chains, colors, zones). It describes shapes with stable IDs, their text, links and colors, with positions kept in a separate layout section so the story content stays readable:

  ```
  willow 1

  zone d "Arc 1" color sky {
    a: "The city closes its gates" color sage
    b: "Food prices **spike**" below a
    c: "Protagonist smuggles grain" right of b
  }

  @layout a 0,0 w210
  @layout b 0,140 w210
  @layout c 290,140 w210 h60
  @layout d -40,-40 580x300
  ```

- Files are `.willow`, UTF-8 without BOM, LF line endings on both platforms. Loading accepts CRLF without marking the document unsaved.
- The first line is the format version, `willow 1`, so files can be migrated later.
- A shape line is `id: "text"`, then an optional link (`below a`, `above a`, `right of a`, `left of a`), then an optional `color name`, in that order. A zone opens with `zone id "Title" color name {` (`color` omitted when the zone is the neutral) and closes with `}` on its own line; an empty zone is those two lines. Zone titles are plain text with the same escapes as shape text.
- `below a` is a link. The direction word is the derived side at save time (core model); positions in `@layout` are what actually place shapes.
- Links are resolved after the whole file is parsed, so a link may point to a shape written later in the file (a chain split across zone blocks can put a child's block before its parent's).
- Chain color is written once per chain, on the chain's first shape in story order, and omitted when it is the neutral; the loader applies it to the whole chain. Colors are written by the fixed lowercase names from Phase 1 (`neutral`, `rose`, `peach`, `butter`, `sage`, `sky`, `lilac`); names never change even if a shade is retuned.
- The layout section holds position, width and extra height for shapes (`@layout a x,y wW [hH]`), and position and size for zones (`@layout d x,y WxH`).
- IDs are auto-generated when shapes and zones are created: shapes and zones share one namespace and one generator (a, b, …, z, aa, ab, …), taking the first ID not in use. A deleted shape's ID may be reused by a later one. IDs never change once assigned.
- Bold/italic is represented inline in the text (`**bold**`, `*italic*`, `***both***`). Text stays on one line: `"` `*` and `\` are always escaped with a backslash, and a line break inside a shape is written `\n`.

### Canonical text

Saving writes canonical text. Loading a canonical file and saving it reproduces it byte for byte.

- **Story order** (ADR 0002): zone blocks and loose chains are ordered by their topmost shape (ties by x, then ID). Each chain is walked depth-first from its topmost shape, visiting neighbors by side in the order below, right, left, above. A chain spanning zones is split across those zone blocks. A nested zone's block sits inside its parent's block. Within a block, the ordered units are nested zones and, per chain, that chain's members in this block, kept together in walk order; units are ordered by their topmost shape. A chain that dips into a nested zone and comes back out is still one unit in the parent block.
- Each link is written once, by the shape the walk reaches second, pointing back to the shape it was reached from. The first shape of a chain writes no link.
- Rich text: adjacent runs with the same style are merged, a bold-and-italic run is written `***x***`, and style markers close before each `\n` and reopen after it.
- One `@layout` line per shape or zone, in story order, all shapes before all zones. Width is always written; extra height `h` only when non-zero.
- Single spaces between tokens, no column alignment, 2-space indent inside zone blocks, one blank line between top-level blocks and before the layout section, and a trailing newline.

### Loading imperfect files

- Unknown lines are ignored on load and dropped on save.
- The loader repairs what it can and keeps going (from Phase 7, the code panel underlines most of these instead of repairing them; see Invalid text): a duplicate ID drops the later line; a link that would close a cycle is dropped; a reference to a missing ID drops the link but keeps the shape; a zone nested deeper than one level is flattened into its parent; conflicting colors within one chain resolve to the first in story order.
- Missing layout is filled in. A linked shape is placed one **creation gap** away in the stated direction; an unlinked shape is placed below the lowest shape on the canvas. A missing width is the default width. A zone without layout wraps its members with padding. The creation gap is a theme size, measured edge to edge in both directions; Paper's is 48 units, and Phase 4's midpoint creation uses the same value.
- Any repair, dropped line or filled-in layout marks the document unsaved, so the rewrite on save is a visible choice.
- A file without a `willow 1` header, from a newer version, or not valid UTF-8 does not open: a native alert names the reason ("not a Willow file", "made by a newer Willow").

### Files and windows

- Ctrl/Cmd+N new document, Ctrl/Cmd+O open, Ctrl/Cmd+S save, Ctrl/Cmd+Shift+S save as. These also appear in the File menu.
- Open always uses a new window, except that it reuses a blank, untouched untitled window. Opening a file that is already open focuses its window.
- Closing a window with unsaved changes asks Save / Don't save / Cancel; this is the only confirm dialog the app has. Quitting (Cmd+Q, or closing the last window on Windows) asks it for each unsaved document in turn; Cancel stops the quit.
- The first manual save of an untitled document asks where to put it.
- Saving writes a temporary file beside the target and renames it over the target, so a crash mid-save never leaves a half-written file. If a save fails (no permission, disk full, folder gone), a native alert names the reason; the document stays unsaved and its recovery file is kept.
- Saving overwrites the file on disk without warning, even if it changed there since it was opened.
- The title bar's saved/unsaved dot reflects manual saves only.

### Crash recovery

- Autosave is crash recovery, not saving: every document with unsaved changes, named or not, is written to a recovery file 5 s after the last change and every 30 s while changes keep coming.
- Recovery files live in the OS app data folder (`~/Library/Application Support/Willow/Recovery`, `%APPDATA%\Willow\Recovery`): `<random>.willow` plus a `<random>.path` sidecar holding the document's path (empty for an untitled one). A manual save or Don't save deletes them.
- On launch, each recovery file newer than its document opens with the recovered content, marked unsaved; an untitled one opens as untitled. Save keeps it; closing with Don't save discards the recovery and leaves the file on disk as it was. No extra dialog. A recovery file older than its document is deleted silently. One whose path no longer exists opens as an untitled document with that content.
- Launch reopens only recovered documents; there is no session restore. With nothing to recover, launch opens one new document.

### Changes on disk

- When a window gains focus, the app checks its file's modified time and content, ignoring the bytes it last wrote itself.
- If the file changed and the document has no unsaved changes, it reloads silently, and the reload is one undoable step so the writer's version can be brought back. If it does have unsaved changes, the app keeps its version and the dot shows unsaved.
- If the file was deleted or renamed, the document stays open and is marked unsaved; the next Save writes it again.

### Tests and performance

- A generator-based round-trip test: `generate(n, seed)`, extended to produce links, colors and zones, is saved, loaded and saved again across many seeds, and both saves are byte-identical. Golden files cover escaping and each repair above.
- On the MacBook Air with the generated 10,000-shape document: save (serialize and write) ≤ 50 ms, load (read, parse, repair, auto-place) ≤ 100 ms, both recorded in bench mode's CSV. Autosave runs off the UI thread, or is measured to stay well under one frame.

## Phase 4 — Chains and editing

Goal: build and reshape chains with the mouse.

### Midpoints and linked shapes

- Hovering a shape shows small circles at its four edge midpoints, at every zoom and at a constant size on screen. They never show on the shape being edited. Each has a 10 px hit radius on screen, which wins over the resize edges it sits on.
- Clicking a midpoint creates a new linked shape on that side, one creation gap away, in the chain's color, and enters editing in it.
- If a shape is already linked on that side, the new shape is **inserted**: the nearest such shape (by center) is relinked to the new shape instead of the source, and its subtree is pushed along just far enough to keep one creation gap on each side of the new shape (often not at all). Other shapes linked on that side keep their link to the source.
- If the new shape lands on an unrelated shape (another chain, or a shape not linked on that side), that shape's whole chain is pushed clear (see Pushing).
- One undo step covers the creation, any push and the first editing session in the new shape, so a single Undo takes it all back. A new shape left empty is kept.
- Link two existing shapes by dragging from a midpoint onto another shape. While dragging, a connector follows the cursor and a valid target shows the hover ring. Dropping on empty space cancels. Dropping on the source, on a shape already linked to it, or where the link would close a cycle does nothing; those targets never highlight. The new link's side is derived as usual, so it may differ from the midpoint the drag started from. When this joins two chains, the chain the drag started from keeps its color and the other chain adopts it.
- Remove a link by right-clicking the connector (Remove link). Connectors have a generous hit tolerance and highlight on hover. Connectors are never selected.
- Connectors follow shapes as they move.

### Pushing

Until Phase 8's spacing pass, a few changes push shapes out of the way. Pushes are real moves (ADR 0004).

- **What pushes**: a shape growing (typing, dragging its bottom edge, or a width change that reflows its text taller), a shape getting wider (dragging a side edge), midpoint creation and insert-and-push, and blocks that were themselves pushed, so pushes cascade. The writer's own drags, moves and pastes never push: a shape dropped onto another overlaps it.
- **Own chain**: the subtrees linked on the growing side (below for height; left and right for width) keep at least one **creation gap** from the grown shape and move as blocks.
- **Other chains**: a shape of another chain that the change brings closer than the **minimum gap** (a theme size, edge to edge; Paper's is 24 units) is pushed with its whole chain as one block, out to the minimum gap.
- A push only happens when a change brings a shape closer than it was. Shapes the writer left closer than the gap stay where they are until something grows into them. In a cascade, a block that was itself pushed keeps shapes it was already closer than the gap to at that same distance; only the change itself (the grown shape, its subtree, a new shape) pushes out to the full gap. This is what makes cascades end.
- Pushes go in the direction of the growth: down for height, outward from the center for width (left-side shapes left, right-side shapes right). Shrinking never pulls anything back.
- Every push folds into the undo step that caused it (the typing session, the resize, the creation). Pushes never change zone membership.

### Deleting

- Delete or Backspace removes the selected shape(s) when nothing is being edited. Deleting a shape in the middle of a chain reconnects its neighbors on opposite sides: the one above with the one below, the one left with the one right. With several neighbors on a side, the **hub** is the neighbor nearest the deleted shape (by center, ties by ID) on the side with fewer neighbors (a tie goes to above, or left), and every neighbor on the opposite side links to it; the other neighbors on the hub's side lose their link and become their own chains. Links on sides with no opposite neighbor are dropped. This can never close a cycle.
- Several selected shapes are deleted one at a time in story order, each reconnecting its own neighbors, so each chain is handled on its own and a run of deleted shapes in one chain closes into one link. The whole delete is one undo step.
- Nothing moves on delete: the gap stays and the connector gets longer.
- Cut behaves the same way, after copying.

### Selection

- Shift+drag starting on empty space draws a marquee; shapes it touches are added to the selection. Shift+click toggles a shape.
- A plain click on a shape in a multi-selection, without dragging, selects only that shape; clicking it again edits it (Phase 2). Clicking empty space clears the selection.
- Dragging any selected shape moves the whole selection, as one undo step.

### Menus

- Right-clicking a shape that isn't selected selects only that shape. Right-clicking inside a multi-selection keeps it.
- Right-click on a shape: Cut, Copy, Paste, Duplicate, Change color (seven circles).
- Right-click on a multi-selection: Cut, Copy, Duplicate, Delete.
- Phase 6 adds Create zone to both.
- Right-click on empty space: New shape, an unlinked neutral shape centered on the click, in editing, joining the innermost zone under the click (a new parallel branch).

### Copy, paste and color

- Paste creates copies at the cursor: the copied group's bounding box is centered on it, keeping the copies' layout; with the cursor outside the canvas, at the view center. Duplicate creates copies one creation gap to the right of the originals' bounding box. Either way the copies become the selection.
- Copies get fresh IDs and keep the links among themselves (remapped) and their chain color, but are not linked to the originals. Pasted copies join the innermost zone under the cursor; duplicates join the originals' zone. Zones themselves are not copied.
- The clipboard carries the text format, so paste works between documents; pasting plain text from another app creates one unlinked shape with that text. While a shape is being edited, paste inserts plain text: copied shapes give their texts joined by line breaks.
- Changing color paints the shape and its whole chain. A new shape created from a chain joins that chain's color. Unlinked shapes start with the neutral color.

## Phase 5 — Keyboard flow

Goal: everything from Phases 2–4 (except multi-select) is fast to do without the mouse. Canvas shortcuts never fire while editing text; Tab is the one exception. Canvas shortcuts use Ctrl/Cmd and Shift only, never Alt/Option: Ctrl+Alt is AltGr on some layouts, and Ctrl+Alt+Arrows rotates the screen on many Windows machines.

### Moving around

- Arrow keys move the selection. A shape is in direction d by the link-side rule (core model), comparing centers: below when |dy| ≥ |dx| and dy > 0, and so on. Follow a link on that side if one exists (several: nearest by center, ties by ID); otherwise jump to the nearest shape of any chain in that direction; with none, nothing happens. With a multi-selection, the selection first collapses to its first shape in story order. Picking the next shape is a core function, tested there.
- When the newly selected shape is off screen, the view pans just enough to show it with a margin, eased over 120 ms. It never zooms.
- With nothing selected, arrow keys pan the view (**keyboard pan**): a tap pans 80 screen px, eased over 120 ms; a held arrow pans continuously at about 900 screen px/s at any zoom, easing in on key-down and out on key-up (tracked by key-down and key-up, not the OS repeat rate); two held arrows pan diagonally. The redraw loop stops when the pan settles.
- Enter with nothing selected selects the shape nearest the view center (ties by ID), without editing. Enter on a selected shape starts editing with the caret at the end; Enter or Esc leaves editing. Shift+Enter inserts a new line while editing. Esc while not editing deselects.

### Moving and resizing

- Ctrl/Cmd+Arrows move the selection (multi-selection too) by 24 units. Like a mouse drag they never push and never change zone membership. With nothing selected they do nothing.
- Ctrl/Cmd+Shift+Left/Right narrow or widen the selected shape by 24 units in total (12 per side, symmetric). Ctrl/Cmd+Shift+Up/Down shrink or grow its extra height by 24. Same minimum-size rule as the mouse; growth pushes as in Phase 4. With a multi-selection they do nothing.
- Each press is one undo step; a held key repeats at the OS rate, each repeat a step. Keyboard moves and resizes are instant, not animated.

### Creating

- **Tab** shows the directional picker over the selected shape, also while editing it (editing ends first). Pressing an arrow immediately creates the linked shape there and enters editing; inserting and pushing work exactly as in Phase 4. Esc or Tab closes the picker; a click closes it and acts as usual; any other key closes it and is consumed. With nothing selected or a multi-selection, Tab does nothing. The keycaps are not clickable.
- Ctrl/Cmd+Shift+N creates an unlinked neutral shape in editing, one creation gap right of the selected shape's chain bounding box, top-aligned with the selected shape (a new branch beside it); with nothing selected, centered in the view; with a multi-selection, placed from its first shape in story order. It joins the innermost zone under its center and pushes what it lands on, like midpoint creation.

### Clipboard, color and chains

- Ctrl/Cmd+X/C/V cut, copy, paste. Ctrl/Cmd+D duplicates.
- Ctrl/Cmd+Shift+C shows the seven color circles as a popup over the selected shape, the focus ring on its current color. Left/Right move the ring (stopping at the ends, no wrap), Enter applies (one undo step, with the ripple), Esc cancels; no live preview. With a multi-selection it does nothing.
- **Select chain** (Ctrl/Cmd+L): selects every shape in the chains of the selected shapes. With nothing selected it does nothing. It is also an item in the shape menu and the multi-selection menu. Not an undo step, like any selection change.
- Every menu item with a shortcut shows it: New shape, Change color, Select chain and the clipboard items.

### Windows check

- A "Phase 5 check on Windows" issue covers the Ctrl+Shift+Arrows feel, key-down and key-up tracking for the held pan, and Tab focus.

## Phase 6 — Zones

Goal: group sections of the canvas (arcs, episodes) into containers. Zones nest one level deep (an episode inside an arc).

### Creating and titles

- Create a zone by right-clicking a shape or a multi-selection and choosing **Create zone**. The zone wraps the selected shapes with the theme's zone padding (the loader's, which clears the title). Only the selected shapes join: unselected shapes that happen to lie inside the rectangle stay out. The item is available only when all selected shapes have the same parent: none (a top-level zone) or the same zone (a nested zone). A selection spanning two zones, or inside an already nested zone, can't become a zone.
- A new zone is neutral, draws above any sibling zones it overlaps, and starts title editing with an empty title. One undo step covers the creation and that first title edit; an empty title is kept.
- Each zone has an editable title and its own color from the same palette (default: the neutral light gray). It never inherits color from its contents.
- A title is plain text on one line (no bold or italic), edited in place on the canvas. Enter or Esc commits; Esc does not cancel. While being edited the title shows even if it overflows its zone; after that, a title that doesn't fit is hidden as before.

### The title band

- A zone's **title band** is the strip along its top edge where the title sits, a constant height on screen at 100% zoom and below (zoomed in, it grows with the title it holds), there even when the title is empty or hidden. It is the only part of a zone the mouse grabs: drag it to move the zone, double-click it to rename, right-click it for the zone menu. A shape drawn over it wins the hit.
- The rest of a zone acts like empty canvas: dragging pans, a click deselects, Shift+drag draws a marquee, and right-click opens the empty-space menu (whose New shape joins the zone). Shift+drag starting on the band also draws a marquee; Shift+click on it does nothing.
- Zones are never selected, like connectors. A press on the band clears the shape selection, like a click on empty space.
- Hovering the band draws the zone's edge in the hovered-connector accent (Paper `#8b7760`), 2 px, following its corner radius. Cursors are those for moving a shape.

### Moving, resizing and membership

- Moving a zone moves its members by the list (including members now outside its rectangle) and its nested zones with their members. Shapes that only lie inside its rectangle stay put. It never pushes and is one undo step.
- Zones resize from all four edges and all four corners, with the same hit width as shape edges; the top and left edges move the zone's position. On the top edge, resizing wins over the title band, as a shape's resize edges win over moving it; a press on an edge clears the shape selection, like the band. The minimum size is the theme's shape minimum width in both directions. A zone may be smaller than its contents: members never move and stay members.
- Zones never grow by themselves: when members grow, are pushed or are created past an edge, the zone keeps its size and the writer resizes it.
- When a drag ends, each dropped shape (each shape of a multi-selection too) joins the innermost zone under its own center, or no zone. Where sibling zones overlap, the one drawn on top wins: the innermost zone under a point is always the one drawn on top there, and every rule that puts a shape in a zone uses it. So a member of a nested zone dropped inside the parent but outside the nested zone joins the parent.
- Only a mouse drop changes membership: keyboard moves, text growth, insert-and-push and zone resizing never do. The exceptions are new shapes: paste and duplicate place copies as in Phase 4, the empty-space New shape and Ctrl/Cmd+Shift+N join the innermost zone under them, and a new linked shape (midpoint creation, insert-and-push, Tab) joins its source shape's zone even when it lands outside the zone's rectangle.
- A dropped zone is judged by its center too: it nests in the top-level zone under its center (nested zones are never targets), or becomes top-level when there is none. A zone that holds nested zones never nests, since that would make two levels; it just moves.
- While shapes or a zone are being dragged, the zone the drop would join shows the hover ring, but only when the drop would change membership. For a multi-selection that is the pressed shape's target, or, when the pressed shape's zone would not change, the target of the first shape in story order whose zone would.
- A dropped zone draws above any sibling zones it overlaps, and a zone always draws above its parent. Draw order is not saved: after a load, zones stack in story order, like shapes.

### Zone menu

- Right-click on the title band, as in the `menu-zone` mockup: Rename, the seven color circles, Dissolve, Delete (in the destructive color).
- Changing a zone's color is one undo step and colors only the zone.
- **Dissolve** removes the container and keeps what it held, one level up: a nested zone's shapes join its parent; a top-level zone's nested zones become top-level and its shapes become loose. Nothing moves. One undo step.
- **Delete** removes the zone, its nested zones and every member shape (including members outside its rectangle). A chain that crosses the zone's edge reconnects by the Phase 4 delete rule, shape by shape in story order. One undo step, no confirm.
- A zone left with no members stays until it is dissolved or deleted.

### Keyboard, performance and Windows

- Zones are mouse-only for now: no shortcuts act on them, and keyboard navigation ignores them.
- Dragging the largest zone of the generated 10,000-shape document meets the frame target (p95 ≤ 8 ms on the MacBook Air), recorded in bench mode's CSV.
- A "Phase 6 check on Windows" issue covers IME in title editing, the resize cursors on zone edges and corners, and the feel of dragging the title band.

## Phase 7 — Code panel

Goal: edit the diagram as text, live, alongside the canvas. The file is the text (ADR 0005).

### Panel and focus

- The **code panel** is a side panel on the right showing the document's text format (Phase 3), as in the `code-panel` mockup. Ctrl/Cmd+E, the title-bar toggle or View > Code Panel opens it and focuses it; when it is open, the same key closes it. Ctrl/Cmd+E follows the Phase 5 rule (Ctrl/Cmd and Shift only); macOS's "use selection for find" doesn't clash, since Willow has no find.
- Whether the panel is open is per window: it starts closed, is not remembered across launches and is not stored in documents. The one exception: loading an invalid file opens it.
- It narrows the canvas instead of covering it; the canvas content stays where it is on screen.
- Resize it by dragging the hairline on its left (about 6 px to grab, column-resize cursor), from 280 px up to half the window (half wins in a window narrower than 560 px), starting at 420 px. The width is an app preference like the theme: the same in every window, remembered across launches, never in documents, not undoable. Widening the panel narrows the canvas without moving its content.
- Clicking the canvas focuses the canvas; Esc in the panel moves focus to the canvas and the panel stays open. Canvas shortcuts never fire while the panel has focus, as while editing a shape.
- The editor (ours to build, ADR 0003) has: caret, mouse and Shift+arrow selection, word and line moves, double-click selects a word, triple-click a line, select all, cut, copy and paste as plain text, soft wrap with line numbers counting logical lines, vertical scroll, IME, and the syntax colors from the visual language. Tab inserts 2 spaces; Enter keeps the current line's indent. No find, auto-closing quotes, autocomplete or folding.

### Text to canvas

- Canvas and text are two views of the same document. While the panel has focus the text stays exactly as typed, even when valid: nothing rewrites it, so the caret never jumps under the writer's hands.
- **Regeneration** writes the text as canonical text from the model. While the panel is open, it happens after each canvas undo step finishes (not on every frame of a drag, and typing in a shape shows when its editing session ends), and when focus leaves the panel with valid text. A closed panel never regenerates; opening it does. Regeneration reorders the text into story order and adds `@layout` lines for auto-placed shapes.
- Text edits reach the canvas 150 ms after the last keystroke; each keystroke restarts the wait, so at most one update is pending and typing in the panel never waits. Pending text applies at once when focus leaves the panel, on Undo or Redo, on Save and before any canvas edit.
- The parser is forgiving: an unclosed quote runs to the end of its line.
- Shapes added in text without a position are auto-placed as the Phase 3 loader does: one creation gap from the shape they link to, or below the lowest shape when unlinked. If the spot is taken, the shape overlaps until Phase 8.
- Changing only a link's direction word (`below a` to `right of a`), with that shape's `@layout` line untouched, re-places the shape one creation gap from `a` on the new side; the part of its chain reached through it moves with it as one block, as in insert-and-push. This keeps direction words meaningful in text, since layout is what places shapes.
- Text edits never push, and zone membership comes only from the `zone { }` blocks, never from the drop rule. Zones never grow from a text edit.
- IDs stay stable across edits so links never scramble. Changing an ID token is not a rename: it is a new shape, and the old references become invalid lines. IDs are lowercase letters, as the generator writes them.

### Invalid text

- An **invalid line** blocks: while any line is invalid, no text edit reaches the canvas, which quietly holds its last valid state. Invalid lines are a reference to a missing ID, a duplicate ID (the later line), an unclosed zone (its `zone` line) or a stray `}`, a link that would close a cycle, a zone nested two levels deep, and a malformed shape, zone or `@layout` line. Each gets the visual language's wavy underline and line tint. No popups.
- Some problems never block and get no underline: a shape without layout is auto-placed; conflicting colors in one chain resolve to the first in story order (Phase 3), so linking two colored chains in text just works; and **ignored lines**, an `@layout` line for an ID that no longer exists and any unknown line, are skipped. Ignored lines are drawn in the `@layout` gray as a hint that the next regeneration drops them; deleting a shape in text needs only its shape line removed.
- A canvas edit while lines are invalid works on the held model and regenerates canonical text from it, dropping the invalid lines and any text edits made since the hold began. It is undoable, so Undo brings the typed text back (see below).

### Undo and saving

- Text edits are steps in the document's single undo history; there is no separate character-level undo. Applied updates merge into one step until the writer pauses typing for 1 s, moves the caret to another line, leaves the panel, or presses Undo. Paste and cut in the panel are each a step of their own. Undo in the panel applies pending text first, then undoes, so one Undo takes back the last stretch of typing, like a shape's editing session.
- Each undo step also records the panel text whenever it is not plain regenerated text (invalid lines, ignored lines, or text not yet regenerated), and undo and redo restore it. So undoing a canvas edit that dropped invalid lines brings them back, and Undo while the text is invalid first records the typed text as a step, then undoes it: the text returns to its last valid version and Redo brings the typing back. Nothing typed is lost without a way back.
- Save writes the panel text as shown, valid or not; it does not regenerate first, so a save while the panel has focus may be non-canonical until the next regeneration. Recovery files write the same text. Applied or pending panel edits mark the document unsaved.
- Loading a valid file regenerates its text as in Phase 3; anything handled quietly (missing layout, a color conflict, an ignored line) marks it unsaved. Loading an invalid file shows the file's text as it is, opens the panel and underlines the invalid lines; the canvas shows the model built from the lines that parse, which is the held model, and the document is not marked unsaved. A file changed on disk (Phase 3) loads by the same rules.

### Selection

- A caret on a shape's line or its `@layout` line selects that shape on the canvas, without panning. A caret on a zone line (zones are never selected), an invalid line or an ignored line leaves the selection alone.
- While the panel is unfocused, a canvas selection scrolls the panel to the selected shape's line; for a multi-selection, its first shape in story order. While the panel has focus, nothing moves its caret or view.

### Performance and Windows

- On the MacBook Air with the generated 10,000-shape document, recorded in bench mode's CSV: typing in the panel draws in ≤ 8 ms per keystroke (only visible lines are laid out and colored); applying text (parse, check, update the model) runs off the UI thread in ≤ 100 ms, the load target, so typing never stalls; regeneration after a canvas step runs off the UI thread in ≤ 50 ms, the save target. With the panel open and idle, CPU stays near zero.
- A "Phase 7 check on Windows" issue covers IME in the panel, JetBrains Mono rendering, the hairline's resize cursor and drag, Ctrl+E and the panel's shortcuts, and the title-bar toggle beside the caption buttons.

## Phase 8 — Polish pass

Goal: shapes never crowd each other, Desk and Poster arrive, and the whole app feels finished and matches the Phase 1 visual language.

### Spacing pass

Every shape keeps a gap from every other shape: the creation gap between two linked shapes, the minimum gap between all others. This replaces Phase 4's pushing and the earlier "may overlap" rules for theme switching, drags, auto-placement and text edits (ADR 0006).

#### Placed and drawn positions

- A shape has a **placed position**, where the writer put it, and a **drawn position**, where it appears. `@layout` saves only the placed position.
- The **spacing pass** works out every drawn position from the placed positions, sizes and links and the theme's gaps. It reads nothing else, so the same document in the same theme always gives the same picture, on reopening too.
- It runs after every change: edits, undo and redo, text applied from the code panel, a load and a theme switch. Shapes added or grown in text push their neighbors clear like any other growth.
- A push is never saved, never an undo step, and never marks the document unsaved. Undo reverts only what the writer changed, and pushed shapes settle back because the pass runs again.
- Everything the writer sees or points at reads drawn positions: painting, connectors, hit-testing, the marquee, arrow-key navigation, midpoints, keeping a shape in view, drop targets, Copy, Paste, Duplicate and Create zone.
- A file's direction words and story order read drawn positions too; only `@layout` reads placed ones. T2 linked below a very tall T stays `below T`, even when T's placed center has passed T2's. So the word order of saved text can vary by theme: a switch leaves the document saved, and the code panel and the next save pick up the new order. Load then save stays byte-identical within one theme.

#### Who moves

- The larger chain holds still and the smaller one moves. On equal size, the chain whose top placed shape is highest holds still, then the leftmost, then the lower ID. So a small branch that grows into the trunk moves itself, and the trunk stays.
- A chain moves as one block, keeping its own layout, by the shortest move that clears it of every shape holding still. Ties go down, right, left, then up. A pushed chain then crowds smaller ones, which move in turn. A chain can change direction when a growing shape makes another way out shorter; that is accepted.
- Inside a chain, of two shapes that crowd each other, the one farther along the walk from the chain's topmost placed shape moves, with its subtree: along their link if they are linked, otherwise by the shortest way out. The pass compares placed top edges and horizontal centers here, which growth never changes.

#### Changes that write placed positions

- **Insert-and-push stays a saved move**, the one exception to pushes being drawn only: the subtree beyond the new shape moves in the creation's undo step, because only the command knows which shape is below which. Phase 7's direction-word re-placing stays saved too.
- A new linked shape is placed from its source's placed position, so it appears one creation gap from where the source is drawn. Landing on an unrelated shape saves nothing; the pass clears it.
- Moving keeps a drop exact. For a mouse drag, Ctrl/Cmd+Arrows or a zone move, each chain touched first takes its drawn positions as its placed positions, then the movement is added, all in the move's undo step. That chain no longer settles back.
- During a drag nothing makes room: dragged shapes pass over everything, and the pass runs on release. A shape dropped on a larger chain slides off it.

#### What stays the same or goes

- Zones take no part. Shapes pass over zone edges and title bands, a pushed shape keeps its membership wherever it is drawn, and zones never grow.
- "Nothing moves on delete" (Phase 4) holds for placed positions: shapes the deleted shape pushed settle back.
- Linked shapes closer than the creation gap are drawn at it, in existing files too; opening such a file doesn't mark it unsaved. Phase 4's rule that shapes left closer than the gap stay where they are is gone.
- The view doesn't follow a shape that moves while being typed in.

#### Motion and measuring

- Pushed shapes slide with the theme's push transition after every change, undo and text edits included. A change during a slide retargets it. Load, reload and a theme switch are instant.
- Opening a document or switching theme measures every shape before anything is drawn, so shapes never jump apart just after a file opens. Load stays within its 100 ms target.

### Themes and alignment

- Add Desk and Poster and View > Theme, matching `design/visual-language.md` and their Phase 1 mockups. Both use Paper's gaps, so a switch reflows only through fonts.
- Bundle Courier Prime and Space Grotesk (Medium and Bold). Space Grotesk has no italic and GPUI doesn't fake one, so slanted Regular and Bold faces are generated once from the font and bundled as TTFs.
- Add View > Text Alignment (see Text alignment).

### Menus

- The View menu follows `mac-view-menu`: Show Code Panel, Zoom In, Zoom Out, Actual Size, Theme, Text Alignment, Enter Full Screen. The Windows menu button holds the same items.
- Zoom In and Zoom Out are Ctrl/Cmd+= and Ctrl/Cmd+-, each one eased wheel zoom step around the view center. Actual Size is Ctrl/Cmd+0. Theme and Text Alignment have no shortcuts.
- macOS gains a standard Window menu. There is no Help menu; the mockups' Help item is dropped.

### Connectors

A connector stays straight while its two shapes overlap on the axis across it, at the middle of that overlap; otherwise it runs midpoint to midpoint. This keeps resized and pushed shapes from slanting their connectors, and a connector may then end away from the midpoint circle.

### Polish review

A fixed list on macOS: every PNG in `design/mockups/png/<theme>/` compared with the running app, plus the timings in `motion.html`. A difference from an approved mockup is a bug and gets a ticket. Anything beyond the mockups is out of scope. The owner's approval closes the review.

### Performance and Windows

- On the MacBook Air with the generated 10,000-shape document: the existing targets still hold, and the ≤ 8 ms per keystroke includes the spacing pass. Bench mode's CSV also records a generated 50,000-shape document and memory use, for information; they have no target.
- A "Phase 8 check on Windows" issue covers Desk and Poster rendering, the new fonts, the View items in the menu button and full screen. The owner runs it after this phase; it doesn't hold Phase 8 open.

---

## Not in this spec (possible later)

- Export (PNG, PDF).
- Collapsing zones to their title.
- Zones growing to fit their members.
- Keyboard shortcuts for zones.
- Alignment guides or snapping between side shapes and trunk shapes.
- Dark theme.
- Comments in the file format (lines the writer adds that survive a save).
- Renaming IDs from the code panel.
- File associations (opening a `.willow` file from Finder or Explorer), recent files, session restore.
- Packaging: an app icon, a `.app` or installer, and signing.
