# Conventions

Binding rules for build, test and code. Keep under ~8 KB. Add a line only when a rule binds future work; delete lines that stop being true. Implementers propose additions in their report; the orchestrator folds them in at merge.

## Stack
- Rust + GPUI, pinned to a git revision (see `willow-spec.md`). Targets: macOS and Windows.

## Local checks (run before pushing)
- `ci.bat` on Windows, `./ci.sh` on macOS: fmt, build, test, clippy `-D warnings`, and the GPUI-free-core rule (ADR 0001). Optional arg: cargo job count (default 4; 2 when another build runs). Add new checks to both scripts.
- Keep output filtered: `./ci.sh 2>&1 | grep -E 'error|warning:|FAILED|passed' | head -40`.

## Tests
- Large or many input files come from generators, not hand-written fixtures. Parser edge cases may use small golden files.
- Benchmark and tool output is CSV.

## Workflow
- Implementers don't poll CI: they run the local checks, push, report. The orchestrator merges on green local checks without waiting for CI; if CI later runs red, it sends the failing job's tail back to the same implementer.
- Model per task: Opus for concurrency, state/serialization, rendering, security-sensitive parsing, cross-module refactors, unexplained failures; Sonnet for mechanical work (pattern-following modules, UI screens, wiring, config, docs, small fixes, merges). State the pick and why in one line.
- Machine-specific facts (tool paths, network quirks) live outside the repo, not here.
