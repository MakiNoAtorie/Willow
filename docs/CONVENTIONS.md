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

## Local checks (run before pushing)
- `ci.bat` on Windows, `./ci.sh` on macOS: fmt, build, test, clippy `-D warnings`, and the GPUI-free-core rule (ADR 0001). Optional arg: cargo job count (default 4; 2 when another build runs). Add new checks to both scripts.
- Keep output filtered: `./ci.sh 2>&1 | grep -E 'error|warning:|FAILED|passed' | head -40`.

- Share the build cache: `CARGO_TARGET_DIR=<main checkout>/target`, so worktrees skip the cold GPUI compile.

## Tests
- Large or many input files come from generators, not hand-written fixtures. Parser edge cases may use small golden files.
- Benchmark and tool output is CSV.

## Workflow
- Implementers don't poll CI: they run the local checks, push, report. The orchestrator merges on green local checks without waiting for CI; if CI later runs red, it sends the failing job's tail back to the same implementer.
- Model per task: Opus for concurrency, state/serialization, rendering, security-sensitive parsing, cross-module refactors, unexplained failures; Sonnet for mechanical work (pattern-following modules, UI screens, wiring, config, docs, small fixes, merges). State the pick and why in one line.
- Machine-specific facts (tool paths, network quirks) live outside the repo, not here.

## Input
- Bind bare letter keys only inside a non-editing key context: on macOS a context-free binding wins over text input and swallows the letter while editing.
