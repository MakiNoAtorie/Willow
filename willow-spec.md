# Willow — Build Spec

A minimal, polished desktop app for planning stories as diagrams. It is deliberately "just rounded shapes and lines": no feature creep. The writer builds a **plot trunk** (a vertical chain of shapes describing the evolving situation) and places **branches** (separate, unlinked chains for what characters do) beside it. From far away the layout reads like a willow tree, but the app knows nothing about trunks or branches. It only knows shapes, links, chains and zones.

## Stack and platform

- Rust + **GPUI** (Zed's UI framework). GPUI is not a stable published crate: pin a git revision and expect to pay for upgrades.
- Windows and macOS from day one; both must build and feel equally good. Bundle each theme's font so text renders identically on both; Phase 1 picks them, and Phase 0 checks emoji fallback on Windows.
- Light themes only (see Themes below).
- Memory and CPU efficiency matter: idle should be near-zero CPU, and large canvases must stay smooth. Animations must stop their redraw loop when they finish.
- Tests attach to one seam: a core crate with no GPUI dependency holding the document model, every editing command, the undo history and the text format. Tests drive commands and assert on the model and on saved text; the GPUI layer only turns input into commands and draws the model (ADR 0001). Shape height comes from text layout, which needs GPUI's text system, so the core takes a text-measuring function from the UI layer instead of measuring text itself (ADR 0003).
- Tests that need large or many input files build them with file generators rather than hand-written fixtures. Parser edge cases (escaping, invalid files) may use small golden files. Any benchmark or tool output should be CSV.
- Performance targets, measured on a generated 10,000-shape document: p95 frame time ≤ 8 ms while panning and zooming, draw ≤ 8 ms per keystroke while typing in a shape with the next vsync never missed (keystroke-to-paint is bounded by the display refresh, not draw cost; ADR 0003), idle CPU near zero within 1 s of the last input or animation. Reference machines: a MacBook Air M4, which must meet the targets, and a Windows desktop (Ryzen 5 9600X, RTX 5060 Ti), checked for correctness and feel. Optionally repeat the Windows run once on the 9600X's integrated graphics as a stand-in for a weak laptop.

## Look and feel (fixed decisions)

- Polish is a core requirement, not a final step: crisp text, soft shadows, consistent spacing, short eased animations, high frame rate.
- Shapes are rounded rectangles. Connectors are **straight lines, no arrowheads**. Avoid anything that looks like spaghetti.
- Six pastel colors plus one neutral light gray. The neutral is the default for new unlinked shapes and for zones. In the picker all seven appear as **more saturated** circles so they're easy to tell apart. On shapes and zones they're applied as the pastel version.
- The full visual language is defined in Phase 1 and every later phase follows it.

## Themes

- Three themes, all light: **Paper** (the default), **Desk** and **Poster**, fixed in Phase 1. Paper is the one every phase builds and is reviewed against; Desk and Poster are added in Phase 8.
- A theme sets the look only: font and typography, the pastel and picker value of each of the seven colors, canvas background, shape fill, borders, corner radii, padding, shadows, connector stroke, selection, hover and midpoint styles, and menus. It never changes the model or the file. Color names are shared, so a chain saved as sage is sage in every theme.
- View > Theme switches it. The choice is an app preference, the same in every open window and remembered across launches; documents don't store it.
- Switching reflows. Each theme keeps its own font, so shapes keep their position, width and extra height, and their text is re-measured: a shape can grow or shrink, and neighbors make room through the spacing pass (Phase 8; before it, shapes may overlap). Spacing never changes the saved positions, so the document stays saved, and the switch is not undoable.
- New shapes use one default width in every theme (Paper's), so the model never depends on the theme.

## Text alignment

- Shape text is justified by default. View > Text Alignment switches every shape to Justified, Left or Centered. In justified text the last line of a paragraph is left-aligned.
- Like the theme, it is an app preference: the same in every open window, remembered across launches, never stored in documents, and not undoable.
- Alignment only moves words within a line: line breaks, and so shape heights, are the same in all three, so switching never reflows.
- Justified text in narrow shapes can leave wide gaps between words, most of all in Desk's monospaced font. That is accepted; Left is the remedy.

## Window

- The canvas fills the window. No toolbar: every action lives in right-click menus and shortcuts.
- One window per document. Ctrl/Cmd+N opens a new window; several documents can be open at once.
- **macOS**: traffic lights over a transparent title bar, plus the native menu bar (File, Edit, View). The Edit menu carries Undo, Redo, Cut, Copy, Paste so the system shortcuts route correctly.
- **Windows**: a slim custom title bar with the document name, a quiet saved/unsaved dot, and one small menu button holding the same menus. It must behave like a native title bar: drag region, double-click to maximize, Windows 11 snap layouts on the maximize button, correct on mixed-DPI monitors. The owner verifies mixed DPI by hand with a second monitor at a different scale.
- A small toggle on the title bar shows/hides the code panel (Phase 7).
- While zooming, the zoom level appears briefly and fades.

## Core model

- **Shape**: rounded rectangle with rich text (plain, bold, italic), a position, a width and an optional extra height. Positions and sizes are whole canvas units; mouse drags at any zoom round to the nearest unit.
- **Link**: connects two shapes. Which side of each shape the connector leaves from (top, bottom, left, right) is **derived from the shapes' relative positions at draw time**, never stored: compare the two shapes' centers; if |dy| ≥ |dx| the link is vertical (above/below), otherwise horizontal, so an exact diagonal is vertical. So a link's direction follows the shapes when they move, and "the shape below b" always means the one that is actually below. Drawing, the saved text and story order all use this one rule.
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
- Drawing takes every visual value from one theme definition (Paper for now), so adding Desk and Poster in Phase 8 is new values, not new drawing code.
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
- Until Phase 7, the loader repairs what it can and keeps going: a duplicate ID drops the later line; a link that would close a cycle is dropped; a reference to a missing ID drops the link but keeps the shape; a zone nested deeper than one level is flattened into its parent; conflicting colors within one chain resolve to the first in story order.
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

- Hovering a shape's border shows small circles at the edge midpoints. Clicking one creates a new linked shape in that direction, one creation gap away, and enters editing in it.
- If that spot is already taken by the next shape in the chain, the new shape is **inserted** and everything beyond it in that direction (the linked subtree) is pushed along to make room. Nothing else moves here; from Phase 8 the spacing pass keeps unrelated shapes clear (before it they may end up overlapped). Zone membership is unchanged by the insert itself. If several shapes link on that side, only the one nearest the midpoint counts as the next shape, and only its subtree is pushed.
- If the spot is taken by an unrelated shape (another chain, or a shape not linked on that side), the new shape is created at the normal spot. Before Phase 8 it overlaps; from Phase 8 the spacing pass pushes the other chain clear.
- Link two existing shapes by dragging from a midpoint onto another shape. A link that would close a cycle is refused. When this joins two chains, the chain the drag started from keeps its color and the other chain adopts it.
- Remove a link by right-clicking the connector (Remove link). Connectors have a generous hit tolerance and highlight on hover.
- Delete key removes the selected shape(s). Deleting a shape in the middle of a chain reconnects its neighbors on opposite sides: the one above with the one below, the one left with the one right. With several neighbors on a side, the nearest one on one side becomes the hub and every neighbor on the opposite side links to it; the other neighbors on the hub's side lose their link and become their own chains. Links on sides with no opposite neighbor are dropped. This can never close a cycle. Cut behaves the same way.
- Connectors follow shapes as they move.
- Multi-select: Shift+drag for a marquee, Shift+click to toggle. Selections can be moved together.
- Right-click on a shape: Cut, Copy, Paste, Duplicate, Change color (seven circles).
- Right-click on a multi-selection: Cut, Copy, Duplicate, Delete (Create zone is added in Phase 6).
- Right-click on empty space: create an unlinked shape there (a new parallel branch).
- Paste creates copies at the cursor; Duplicate creates copies beside the originals. Copies get fresh IDs and keep the links among themselves (remapped) and their chain color, but are not linked to the originals. Pasted copies join the innermost zone under the cursor; duplicates join the originals' zone. Zones themselves are not copied. The clipboard carries the text format, so paste works between documents; pasting plain text from another app creates one unlinked shape with that text.
- Changing color paints the shape and its whole chain. A new shape created from a chain joins that chain's color. Unlinked shapes start with the neutral color.

## Phase 5 — Keyboard flow

Goal: everything from Phases 2–4 (except multi-select) is fast to do without the mouse. Canvas shortcuts never fire while editing text. Canvas shortcuts use Ctrl/Cmd and Shift only, never Alt/Option: Ctrl+Alt is AltGr on some layouts, and Ctrl+Alt+Arrows rotates the screen on many Windows machines.

- Arrow keys move the selection: follow a link in that direction if one exists, otherwise jump to the nearest shape in that direction.
- Shift+Enter inserts a new line while editing. Esc while not editing deselects.
- Ctrl/Cmd+Arrows move the shape.
- Ctrl/Cmd+Shift+Left/Right change width (symmetric). Ctrl/Cmd+Shift+Up/Down move the bottom edge. Same minimum-size rule as the mouse.
- Ctrl/Cmd+X/C/V cut, copy, paste. Ctrl/Cmd+D duplicates.
- Ctrl/Cmd+Shift+C opens the color picker, navigable by keyboard.
- **Tab** shows the directional picker over the current shape. Pressing an arrow immediately creates the linked shape there and enters editing. If the spot is taken, insert, push or overlap exactly as in Phase 4.
- Ctrl/Cmd+Shift+N creates an unlinked shape.

## Phase 6 — Zones

Goal: group sections of the canvas (arcs, episodes) into containers.

- Create a zone by selecting shapes, right-clicking the selection and choosing **Create zone**. The zone wraps the selection. The item is available only when all selected shapes have the same parent: none (a top-level zone) or the same zone (a nested zone). A selection spanning two zones, or inside an already nested zone, can't become a zone.
- Shapes join a zone by being dropped inside it (the shape's center decides), and leave by being dropped outside. Only a mouse drop changes membership: keyboard moves, text growth, insert-and-push and zone resizing never do. The one exception is new copies: paste and duplicate place copies in a zone as described in Phase 4. From Phase 8, a shape moved by the spacing pass is also judged as if dropped where it ends up.
- Dropping a zone inside another nests it; dropping a nested zone outside un-nests it.
- Moving a zone moves everything inside it. Zones are resizable, including smaller than their contents; members stay members.
- Each zone has an editable title and its own color from the same palette (default: the neutral light gray). It never inherits color from its contents.
- Right-click on a zone: Rename, Change color, Dissolve (remove the container, keep the shapes), Delete (remove the zone and everything in it).
- One level of nesting (e.g. episode inside arc).
- Zones are mouse-only for now.

## Phase 7 — Code panel

Goal: edit the diagram as text, live, alongside the canvas.

- A hideable side panel shows the document's text format (from Phase 3). Ctrl/Cmd+E or the title-bar toggle shows/hides it.
- Canvas and text are two views of the same document. Canvas edits rewrite the text; text edits update the canvas after a short pause. The canvas never rewrites the text while the panel has focus, so the caret never jumps under the writer's hands.
- A text edit, once applied after the pause, is one step in the document's single undo history. Ctrl/Cmd+Z in the panel undoes that step and regenerates the text; there is no separate character-level undo.
- The parser is forgiving: an unclosed quote runs to the end of its line, and unknown lines are ignored.
- When the text is invalid (e.g. a reference to an ID that doesn't exist anywhere in the text, a duplicate ID, an unclosed zone), the canvas quietly holds its last valid state and the offending line gets a subtle underline. No popups.
- The file is the text. Saving with invalid text writes the text as typed, never the last valid model, so nothing the writer typed is lost. Loading an invalid file shows whatever parses, opens the panel and underlines the rest. Invalid lines survive only until the next canvas edit, which regenerates canonical text without them; that edit is undoable, so Ctrl/Cmd+Z brings the lines back.
- Shapes added in text without a position are auto-placed as the Phase 3 loader does: one creation gap from the shape they link to, or below the lowest shape when unlinked. If the auto-placed spot is taken, the shape overlaps it until Phase 8, where the spacing pass pushes the neighbors clear.
- IDs stay stable across edits so links never scramble. Changing an ID token in the text is not a rename: it is a new shape, and the old references become invalid lines.

## Phase 8 — Polish pass

Goal: the whole app feels finished and matches the Phase 1 visual language.

- Review every interaction on both platforms against the approved mockups: animation timing, hover states, cursor changes, spacing and text rendering.
- Add the Desk and Poster themes and View > Theme, matching their Phase 1 mockups.
- Add View > Text Alignment (Left and Centered next to the default Justified).
- Profile memory and frame times on very large generated documents and fix hotspots, against the performance targets on the MacBook Air.
- Shapes never overlap: every shape keeps at least a minimum gap (a theme size, set in this phase) from every other shape. This replaces the earlier "may overlap; nothing is pushed" rules for text growth, theme switching, shape creation in Phase 4/5 and auto-placement in Phase 7.
  - Spacing is its own process, separate from editing. Each shape keeps the position the writer gave it (by moving, creating or placing it). Where it is drawn is worked out from those positions by the spacing pass, which runs after every change.
  - When a shape grows, by typing or resizing, any shape it would come closer to than the gap is pushed away in whatever direction gets it clear soonest, sideways included. Pushes cascade: a pushed shape pushes what it then crowds.
  - A shape in another chain (a branch, say) is never pushed alone: its whole chain moves with it as one block, keeping its own layout. Within the growing shape's own chain, the linked subtree beyond the crowded shape moves, as in Phase 4's insert-and-push.
  - Pushes are never undo steps. Undo reverts only the change the writer made (the text, the resize), and the spacing pass then lets pushed shapes settle back toward their own positions.
  - Zones: a pushed shape joins or leaves a zone by the same rule as a mouse drop (its center decides).

---

## Not in this spec (possible later)

- Export (PNG, PDF).
- Collapsing zones to their title.
- Alignment guides or snapping between side shapes and trunk shapes.
- Dark theme.
- Comments in the file format (lines the writer adds that survive a save).
- Renaming IDs from the code panel.
- File associations (opening a `.willow` file from Finder or Explorer), recent files, session restore.
