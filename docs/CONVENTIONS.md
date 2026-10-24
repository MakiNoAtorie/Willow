# Conventions

Binding rules for build, test and code. Keep under ~8 KB. Add a line only when a rule binds future work; delete lines that stop being true. Implementers propose additions in their report; the orchestrator folds them in at merge.

## Stack
- Rust + GPUI, pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003). Targets: macOS and Windows.
- GPUI won't resolve without the `[patch.crates-io]` block from Zed's root `Cargo.toml` at the pinned rev; copy it into the workspace `Cargo.toml` and re-copy it on every pin bump.
- Build needs the Metal Toolchain on macOS (`xcodebuild -downloadComponent MetalToolchain`) and Visual Studio Build Tools with a Windows SDK on Windows (`fxc` for HLSL). Don't enable `gpui_platform/runtime_shaders` to work around a missing toolchain.

## Layout
- Workspace crates: `crates/willow-core` (GPUI-free, no deps unless needed) and `crates/willow-app` (binary `willow`; run with `cargo run -p willow-app`).
- Visual values live as `Theme` fields in the core, never as constants in the app. Colors are `Rgba(0xRRGGBBAA)`; the app converts with `gpui::rgba(c.0).into()`.
- Theme shadows keep the CSS values from `themes.js`; the app passes `blur / 2` as GPUI's `blur_radius` (GPUI blur is the Gaussian sigma).
- macOS: the title bar floats over the canvas, so add it as the last child or the canvas paints over it.
- Platform window chrome lives in `<name>_<os>.rs` modules gated by `#[cfg(target_os = ...)]` in `main.rs`, each exposing `title_bar(session, window, cx)` with the same signature.
- Type-check Windows-only code on macOS with `cargo clippy -p willow-app --target x86_64-pc-windows-gnu -- -D warnings`.
- Menu items enable from action-handler presence: register handlers with `.when(session.can_x(), …)` on the focused root. Never call `window.is_action_available` during render (GPUI panics); read core state. Menu visuals live in `Theme.menu` (screen px).

## Text
- Shape text offsets are UTF-8 bytes into `RichText::plain()` (paragraphs joined by `\n`). A caret is `Caret { offset, upstream }` (`upstream`: a soft-wrap offset shown at the end of the earlier line); the selection is `Editing.anchor..caret.offset`.
- Layout is the pure `layout(text, width, theme, &dyn Measure)` in `willow-core/src/layout.rs`; read geometry through `Session::layout(&shape)` / `Session::shape_height(&shape)` (cached per shape, keyed on text + width). Editing ops live in `edit.rs` and go through `RichText::replace`, which keeps text normalized.
- The app's measurer and glyph painting live in `willow-app/src/text.rs`. Fonts are embedded from `crates/willow-app/assets` (Literata + `OFL.txt`); the family is `Theme.font`. Caret visuals are `Theme.caret`; caret width is screen px at any zoom.
- Style toggles (`Input::ToggleBold/ToggleItalic`) act on `Editing.anchor..caret.offset`; with an empty selection they set `Editing.pending`, which `set_caret` clears and `Session::replace` applies inside the same `history.change`. A toggle turns the style on unless the whole selection already has it.
- One public way to set a text selection: `Session::set_text_selection`. Clipboard text enters the core via `Input::Paste` (keeps paragraph breaks), never `Input::Text`. The app writes `selected_text()` to the clipboard before sending `Input::Cut`; the core never touches the clipboard.
- In core tests, compute screen points with `s.viewport().to_screen(canvas point)`, not fixed screen constants.
- IME marked text (`Editing.marked`) lives in the edited shape's text but outside `History`; `Session::handle` commits it before any input except Mark/Text, Space key up/down, pointer move, wheel, pinch and reset zoom (the Japanese IME converts on Space). New inputs commit by default.
- The core takes IME selections as byte ranges; only the app converts UTF-16 (`from16`/`to16` in `text.rs`).
- Justification gaps are spaces plus UAX #14 break opportunities next to a CJK cluster; add new gap kinds in the `stretch` closure in `layout.rs`.
- Core tests use `tests/common::Fake` (space 4, CJK 15, other 10, bold 11) with `session_of` / `doc_with`.
- Editing keys are GPUI actions in the `editing` key context; F2, Enter and Esc are bound everywhere. The input handler is registered whenever a shape is selected.

## Input and redraw
- The app sends all input through `Session::handle(input, now)` and calls `cx.notify()` only when `Response.redraw` is true; it requests frames only while `Response.animating`. This keeps idle CPU near zero.
- Undoable model changes go through `History::change` from a named `Session` method; viewport and selection never touch history. Editing sessions wrap changes in `History::begin()`/`end()`; undo while one is open reverts it and keeps it open.
- Paint draws only from the draw query `Session::draw(from, to)`: previews, fill, culling and low-zoom switches live there, not in the app. Cull on position and width before asking for height or layout. Low-zoom values come from `ZoomStyle` (`fade_start`, `fade_end`, `block_mix`).
- Big documents come from `willow_core::generate(n, seed)` (the app's `--generate` uses seed 1); bulk IDs use `id_for(i)`, not `Document::next_id`.
- Gesture mutations land once, on release, through a named `Session` method (e.g. `move_shape`); live previews are queries (`drag_preview`), never written into the document.
- Time-based animations take a `Transition { duration, easing: CubicBezier }` from the theme and OR their state into `Session::animating(now)`. `render` calls `Session::tick(now)`; paint calls `window.request_animation_frame()` only while animating.
- Paint multiplies every canvas-unit value (sizes, radii, shadows, ring widths, grid) by `vp.zoom`; screen-space chrome (the zoom pill) is not scaled. Low-zoom values extend `ZoomStyle`.
- Wheel and pinch inputs carry the canvas-relative pointer position. Modified shortcuts (Cmd/Ctrl+key) go through GPUI actions mapped to `Input` variants, not through `Key`.
- Resize: edge hit zones are 5 screen px (`EDGE_GRAB`); a shape's interior hides edges beneath it. Previews are `Session::resize_preview()` (a `Shape` clone the app paints instead). Commands are `Session::resize_shape` and `Session::autofit`. A press on an edge leaves editing first, so the resize stays its own undo step. Side resizes keep width parity (steps of 2), so the minimum can land one unit above the theoretical one.
- Press-versus-drag uses one threshold, `DRAG_START` (3 screen px) in `session.rs`.
- Screen positions given to the core are relative to the canvas top-left (the app subtracts `canvas_origin`, saved during paint).
- Pointer move/up listen window-wide in paint (`window.on_mouse_event`) so drags continue outside the canvas; pointer down and scroll listen on the canvas div.
- Wheel deltas are content movement in screen px (GPUI's sign). Paint rounds the screen position of canvas (0,0) to whole pixels.

## Local checks (run before pushing)
- `ci.bat` on Windows, `./ci.sh` on macOS: fmt, build, test, clippy `-D warnings`, and the GPUI-free-core rule (ADR 0001). Optional arg: cargo job count (default 4; 2 when another build runs). Add new checks to both scripts.
- Keep output filtered: `./ci.sh 2>&1 | grep -E 'error|warning:|FAILED|passed' | head -40`.

- Don't share one `CARGO_TARGET_DIR` across worktrees: workspace crates collide. Clone the cache per worktree instead: `cp -cR <main>/target <worktree>/target` (APFS clone, skips the cold GPUI compile).

## Tests
- Large or many input files come from generators, not hand-written fixtures. Parser edge cases may use small golden files.
- Benchmark and tool output is CSV.

## Workflow
- Implementers don't poll CI: they run the local checks, push, report. The orchestrator merges on green local checks without waiting for CI; if CI later runs red, it sends the failing job's tail back to the same implementer.
- Model per task: Opus for concurrency, state/serialization, rendering, security-sensitive parsing, cross-module refactors, unexplained failures; Sonnet for mechanical work (pattern-following modules, UI screens, wiring, config, docs, small fixes, merges). State the pick and why in one line.
- Machine-specific facts (tool paths, network quirks) live outside the repo, not here.

## Input
- Bind bare letter keys only inside a non-editing key context: on macOS a context-free binding wins over text input and swallows the letter while editing.
