# Conventions

Binding rules for build, test and code. Keep under ~8 KB. Add a line only when a rule binds future work; delete lines that stop being true. Implementers propose additions in their report; the orchestrator folds them in at merge.

## Stack
- Rust + GPUI, pinned to a git revision (see `willow-spec.md`). Targets: macOS and Windows.

## Local checks (run before pushing)
- `cargo fmt --check`
- `cargo clippy --all-targets -q 2>&1 | grep -E '^(error|warning)' | head -40`
- `cargo test -q 2>&1 | tail -40`

## Tests
- Large or many input files come from generators, not hand-written fixtures. Parser edge cases may use small golden files.
- Benchmark and tool output is CSV.

## Workflow
- Implementers don't poll CI; the orchestrator checks CI once at merge and sends failures back to the same implementer.
- Model per task: Opus for concurrency, state/serialization, rendering, security-sensitive parsing, cross-module refactors, unexplained failures; Sonnet for mechanical work (pattern-following modules, UI screens, wiring, config, docs, small fixes, merges). State the pick and why in one line.
- Machine-specific facts (tool paths, network quirks) live outside the repo, not here.
