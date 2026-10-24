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
- Platform window chrome lives in `<name>_<os>.rs` modules gated by `#[cfg(target_os = ...)]` in `main.rs`, each exposing `title_bar(theme, window)` with the same signature.

## Input and redraw
- The app sends all input through `Session::handle(input, now)` and calls `cx.notify()` only when `Response.redraw` is true; it requests frames only while `Response.animating`. This keeps idle CPU near zero.
- Gesture mutations land once, on release, through a named `Session` method (e.g. `move_shape`); live previews are queries (`drag_preview`), never written into the document.
- Time-based animations report through `Session::animating(now)`; the app calls `window.request_animation_frame()` from paint only while it is true.
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
