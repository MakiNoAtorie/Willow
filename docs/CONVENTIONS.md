# Conventions

Binding rules for build, test and code. Keep under ~8 KB. Add a line only when a rule binds future work; delete lines that stop being true. Implementers propose additions in their report; the orchestrator folds them in at merge.

## Stack
- Rust + GPUI, pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003). Targets: macOS and Windows.
- GPUI won't resolve without the `[patch.crates-io]` block from Zed's root `Cargo.toml` at the pinned rev; re-copy it on every pin bump.
- Build needs the Metal Toolchain on macOS (`xcodebuild -downloadComponent MetalToolchain`) and Visual Studio Build Tools with a Windows SDK on Windows (`fxc`). Keep `gpui_platform/runtime_shaders` off.

## Local checks
- `./ci.sh` (macOS) / `ci.bat` (Windows): fmt, build, test, clippy `-D warnings`, GPUI-free core (ADR 0001), and on macOS a Windows-target clippy when `x86_64-pc-windows-gnu` is installed. Add new checks to both scripts. `.githooks/pre-push` runs it (`git config core.hooksPath .githooks`).
- Worktrees each get their own build cache: `cp -cR <main>/target <worktree>/target` (APFS clone). A shared `CARGO_TARGET_DIR` makes workspace crates collide.

## Layout
- Crates: `crates/willow-core` (GPUI-free) and `crates/willow-app` (binary `willow`).
- Core session: input vocabulary in `input.rs`, text editing in `session/editing.rs`. New editing inputs go in `Session::handle_text` and are listed in `handle`'s dispatch arm.
- Platform chrome lives in `titlebar_<os>.rs`, each exposing `title_bar(session, window, cx)`. On macOS the title bar is the canvas's last child, or the canvas paints over it.

## Theme
- Every visual value is a `Theme` field in the core (window chrome: `Theme.chrome`, menus: `Theme.menu`); the app holds no visual constants.
- Colors are `Rgba(0xRRGGBBAA)`, converted with `gpui::rgba(c.0).into()`. Shadows keep the CSS values from `themes.js`; the app passes `blur / 2` as GPUI's `blur_radius` (GPUI blur is the Gaussian sigma).
- Timed animations take a `Transition { duration, easing: CubicBezier }`. Low-zoom values live in `ZoomStyle`.

## Text
- Offsets are UTF-8 bytes into `RichText::plain()` (paragraphs joined by `\n`). A caret is `Caret { offset, upstream }` (`upstream`: a soft-wrap offset shown at the end of the earlier line); the selection is `Editing.anchor..caret.offset`, set publicly only through `Session::set_text_selection`.
- Layout is the pure `layout(text, width, theme, &dyn Measure)` in `layout.rs`; read it through `Session::layout` / `Session::shape_height` (cached on text + width). Edits go through `RichText::replace`, which keeps text normalized.
- Justification gaps are spaces plus UAX #14 break opportunities next to a CJK cluster; new gap kinds go in the `stretch` closure.
- Style toggles take `edit::Style`. With an empty selection they set `Editing.pending`, which `set_caret` clears and `Session::replace` applies in the same `history.change`. A toggle turns the style on unless the whole selection has it.
- Clipboard text enters through `Input::Paste` (keeps paragraph breaks). The app writes `selected_text()` to the clipboard before `Input::Cut`; the core never touches the clipboard.
- IME marked text (`Editing.marked`) sits in the shape's text outside `History`. `handle` commits it before every input except Mark/Text, Space key up/down, pointer move, wheel, pinch and reset zoom (Japanese IME converts on Space). The app alone converts UTF-16 (`from16`/`to16` in `text.rs`).
- The app's measurer and glyph painting live in `willow-app/src/text.rs`; Literata is embedded from `crates/willow-app/assets`. Caret width is screen px at any zoom.

## Input and redraw
- All input goes through `Session::handle(input, now)`. The app calls `cx.notify()` only on `Response.redraw`, requests animation frames only while `Session::animating(now)`, and otherwise redraws from a timer at `Session::next_wake(now)` (the caret blink). This keeps idle at 0 renders.
- Positions given to the core are relative to the canvas top-left. Wheel and pinch carry the pointer position; wheel deltas are content movement in screen px (GPUI's sign).
- Pointer move/up listen window-wide (`window.on_mouse_event`) so drags continue outside the canvas; pointer down and scroll listen on the canvas div. `Input::FocusLost` (window deactivation) resets held keys and pans.
- Cmd/Ctrl shortcuts are GPUI actions mapped to `Input` variants. Editing keys live in the `editing` key context; bind bare letters only in a non-editing context, since on macOS a context-free binding swallows the letter while editing.
- Menu items enable from handler presence: `.when(session.can_x(), …)` on the focused root. Read core state during render; `window.is_action_available` panics there.
- Gestures: press-versus-drag uses `DRAG_START` (3 screen px), edges `EDGE_GRAB` (5 screen px). Previews are queries (`drag_preview`, `resize_preview`); the mutation lands once on release through a named `Session` method (`move_shape`, `resize_shape`, `autofit`). Side resizes keep width parity, so the minimum can land one unit above the theoretical one.
- Undoable changes go through `History::change` from a named `Session` method; viewport and selection never do. An editing session is one `begin()`/`end()` group.

## Drawing
- Paint reads only `Session::draw(from, to, now)`: previews, fill, culling, low-zoom switches and each shape's `ring`, `selection`, `marked` and `caret`. Overlays paint with their shape. Cull on position and width before asking for layout.
- Paint multiplies canvas-unit values by `vp.zoom`; screen-space chrome (the zoom pill) is unscaled. The screen position of canvas (0,0) is rounded to whole pixels.

## Tests
- Drive the core's public API. Core tests use `tests/common::Fake` (space 4, CJK 15, other 10, bold 11) with `session_of` / `doc_with`, and compute screen points with `s.viewport().to_screen(..)`.
- Large inputs come from generators: `willow_core::generate(n, seed)` (`--generate` uses seed 1; bulk IDs use `id_for(i)`). Tool output is CSV.
- Bench: `cargo run --release -p willow-app -- --bench --machine NAME` appends to `bench/results.csv` (`git_rev` = Willow's short HEAD). It feeds input through `Session::handle` and typing through `window.dispatch_keystroke`; gpui keeps the `profiler` feature. Judge idle by `renders == 0` (~0.5% CPU is GPUI's display link).

## Workflow
- Model per task: Opus for concurrency, state/serialization, rendering, security-sensitive parsing, cross-module refactors, unexplained failures; Sonnet for mechanical work (pattern-following modules, UI screens, wiring, config, docs, small fixes, merges). State the pick and why in one line.
- Machine-specific facts (tool paths, network quirks) live outside the repo.
