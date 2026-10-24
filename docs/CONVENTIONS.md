# Conventions

Binding rules. Keep under ~8 KB; add only rules that bind future work, delete stale ones. The orchestrator folds implementers' proposals in at merge.

## Stack
- Rust + GPUI pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003); re-copy Zed's `[patch.crates-io]` block on every pin bump.
- Build needs the Metal Toolchain (macOS) or VS Build Tools + Windows SDK (`fxc`). Keep `gpui_platform/runtime_shaders` off.

## Local checks
- `./ci.sh` / `ci.bat`: fmt, build, test, clippy `-D warnings`, GPUI-free core, Windows-target clippy on macOS. Add new checks to both scripts.
- Worktrees get their own cache: `cp -cR <main>/target <worktree>/target`; never share `CARGO_TARGET_DIR`.

## Layout
- Crates: `crates/willow-core` (GPUI-free) and `crates/willow-app` (binary `willow`).
- Core session: input vocabulary in `input.rs`, text editing in `session/editing.rs`. New editing inputs go in `Session::handle_text` and are listed in `handle`'s dispatch arm.
- Platform chrome: `titlebar_<os>.rs` (`title_bar(name, session, window, cx)`); on macOS it is the canvas's last child.

## Model
- Build `Document` literals with `..Document::default()`. Shapes and zones share `ShapeId` and one generator; `doc.zones` order is draw order, parents first.
- Session code changes structure only through `History::add`/`remove`/`link`/`unlink`/`recolor`/`join_zone`; `Document::link`/`unlink`/`recolor` are for loaders and generators.
- A multi-change command wraps them in `History::step` (joins an open editing group); commands that open editing call `begin()` and leave closing to `stop_editing`. Never reorder surviving shapes; write a reorder as remove plus add. Side rule: `willow_core::side`; centers need `Session::shape_height`.
- Shapes are removed only through `Session::delete(ids)` (one step, story order, reconnects, drops them from selection); Cut calls it after copying. Story order outside saving: `format::story_order(doc, heights)`. ID tie-breaks use generation order (shorter first, then alphabetical).
- Derived indexes cached in `Session` are validated against the document on read; no mutation path invalidates them.

## Files
- Format: `format.rs` (`save(doc, heights)`, `load(bytes)`); heights come from `Session::heights()`, taken on the UI thread. Golden files `tests/golden/*.willow` (git `-text`), repairs as `repair-<case>.willow`; tests compare models order-free.
- Loaded files go through `Session::open` / `disk_changed` (apply `Loaded.unplaced` via `format::place`). `Theme.creation_gap` is the one creation gap.
- File state and recovery timing live in `session/file.rs`; the app only does I/O, in `file.rs` (atomic `write_beside`, `alert`, `rfd`) and `recovery.rs` (off the UI thread).
- `file::settle` is the one Save / Don't Save / Cancel routine and never deletes; `file::close` / `quit` delete recovery via `discard_recovery(cx)`.
- Every `History` method that changes the document bumps `revision`. Call `arm_recovery` after anything that may change the session; recovery I/O runs through `Autosave::run`, never locking the UI thread.

## Theme
- Every visual value is a `Theme` field in the core (window chrome: `Theme.chrome`, menus: `Theme.menu`); the app holds no visual constants.
- Colors are `Rgba(0xRRGGBBAA)` (`gpui::rgba(c.0).into()`). Shadows keep `themes.js` CSS values; the app passes `blur / 2` as GPUI's `blur_radius`.
- Timed animations take a `Transition { duration, easing: CubicBezier }`. Low-zoom values live in `ZoomStyle`.

## Text
- Offsets are UTF-8 bytes into `RichText::plain()` (paragraphs joined by `\n`). The selection is `Editing.anchor..caret.offset`, set publicly only through `Session::set_text_selection`.
- Every way of opening a document calls `Session::measure_ahead(n)` from idle afterwards, so saves don't stall.
- Layout is the pure `layout(text, width, theme, &dyn Measure)` in `layout.rs`; read it through `Session::layout` / `Session::shape_height` (cached on text + width). Edits go through `RichText::replace`, which keeps text normalized.
- Justification gaps are spaces plus UAX #14 break opportunities next to a CJK cluster; new gap kinds go in the `stretch` closure.
- Style toggles take `edit::Style`; with an empty selection they set `Editing.pending` (cleared by `set_caret`, applied by `Session::replace`). A toggle turns on unless the whole selection has it.
- Clipboard: the app writes `Session::clipboard_text()` (selected text while editing, else the selected shapes as `willow 1` text) before Cut/Copy, and sends `Input::Paste(text)` (text while editing, else shapes or one plain-text shape). The core never touches the clipboard.
- IME marked text (`Editing.marked`) sits outside `History`; `handle` commits it before every input except Mark/Text, Space up/down, pointer move, wheel, pinch, reset zoom. Only the app converts UTF-16 (`text.rs`).
- The app's measurer and glyph painting live in `willow-app/src/text.rs`; Literata is embedded from `crates/willow-app/assets`. Caret width is screen px at any zoom.

## Input and redraw
- All input goes through `Session::handle(input, now)`. The app calls `cx.notify()` only on `Response.redraw`, animates only while `Session::animating(now)`, else wakes at `Session::next_wake(now)`; idle is 0 renders.
- Positions given to the core are relative to the canvas top-left. Wheel and pinch carry the pointer position; wheel deltas are content movement in screen px (GPUI's sign).
- Pointer move/up listen window-wide (`window.on_mouse_event`); down and scroll on the canvas div. `Input::FocusLost` resets held keys and pans.
- Cmd/Ctrl shortcuts are GPUI actions mapped to `Input` variants. Editing keys live in the `editing` key context; bind bare letters only in a non-editing context (macOS swallows them otherwise).
- Menu items enable from handler presence: `.when(session.can_x(), …)` on the focused root. Read core state during render; `window.is_action_available` panics there.
- Gestures: press-versus-drag uses `DRAG_START` (3 screen px), edges `EDGE_GRAB` (5 screen px). Previews are queries; the mutation lands once on release through a named `Session` method.
- Undoable changes go through `History` from a named `Session` method; viewport and selection never do. Selection is `Session::selection() -> &[ShapeId]`, set only via `Session::select(ids)`; single-shape code uses `only_selected()`. Draw reads rings via `Session::ring(id, now)`. Reloads go through `History::replace`, then `drop_missing_selection`. An editing session is one `begin()`/`end()` group.

## Chains
- Menus are core data (`Session::menu()`: kind, screen/canvas point, entries); the app's `menu.rs` only draws them, all values from `Theme.menu`. New menu: `MenuKind` + `open_*_menu` from the right-press branch; new item: `MenuAction` + `MenuItem` + a `Session::choose` arm. Items needing the clipboard go through the app's `Willow::choose`. Items target the shape hit at `Menu.canvas`. Shortcuts are core `Shortcut`; the app adds Cmd/Ctrl.
- Left press priority: midpoint (10 screen px), alt-autofit, resize edge, shape. Tests grab edges away from midpoints.
- Linked creation: `Session::create_linked(id, side)`; extra ops go between its `begin()` and `start_editing`. Copies (paste, duplicate): `insert_copies` with `fresh_ids(n)` assigned before measuring. Paste point: open menu's point, else last pointer in the canvas, else view center.
- Pushes: `Session::push(ids, dx, dy)` in `session/push.rs`, a saved move in the open step, never touching zones; distance from `clearance` (zero when clear). Blocks: own chain `subtree(root, from)`, other chains `doc.chain(id)`; side neighbors `linked_on(id, side)`, nearest first.
- Multi-selection moves only via `Session::move_selection` (no push). Menu recolors via `ripple_recolor`. Link drag and connector hover live in `session/link.rs`. A gesture that changes structure calls `stop_editing` first.
- Animations are `Visible` fields computed in the core (`appear`, `slide`, blended `fill`) and counted in `animating`; undo/redo clears slides. Screen-constant overlays (midpoints, marquee, link drag) are `Frame` fields painted after shapes.

## Drawing
- Paint reads only `Session::draw(from, to, now)`: previews, fill, culling, low-zoom switches and each shape's `ring`, `selection`, `marked` and `caret`. Overlays paint with their shape. Paint order: zones (with titles), connectors, shapes; draw-query geometry is canvas units. Cull on position and width before asking for layout.
- Paint multiplies canvas-unit values by `vp.zoom`; screen-space chrome (the zoom pill) is unscaled. The screen position of canvas (0,0) is rounded to whole pixels.

## Tests
- Drive the core's public API. Core tests use `tests/common::Fake` (space 4, CJK 15, other 10, bold 11) with `session_of` / `doc_with`, and compute screen points with `s.viewport().to_screen(..)`.
- Property tests use the built-in fixed-seed xorshift (no new dependency) and print the case number in every assertion.
- Large inputs come from generators: `willow_core::generate(n, seed)` (`--generate` uses seed 1). Tool output is CSV.
- Bench: `cargo run --release -p willow-app -- --bench --machine NAME` appends to `bench/results.csv` (`git_rev` = Willow's short HEAD). gpui keeps the `profiler` feature. Bench rows go in their own commit after the code they measure.

## Workflow
- Model per task: Opus for state, serialization, rendering, gestures, cross-module refactors, unexplained failures; Sonnet for mechanical work. State the pick and why in one line.
