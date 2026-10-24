# Willow

Spec: `willow-spec.md`. Build, test and code conventions: `docs/CONVENTIONS.md`.

Local CI: `./ci.sh` on macOS, `ci.bat` on Windows (optional arg: cargo job count, default 4; use 2 when another build runs). Everyone, you and every subagent, runs it before every push and every merge, with output filtered to `error|warning:|FAILED|passed`; the pre-push hook enforces it (`git config core.hooksPath .githooks`). Merge on a green local CI; there is no GitHub CI.

Platform focus: build and verify on macOS; Windows is checked by the owner after all phases, so keep Windows code compiling (`ci.sh` type-checks it) and log each Windows gap or hand test on that phase's Windows check issue (e.g. #18).

Multi-agent work: start ticket implementers with `subagent_type: "implementer"` (`.claude/agents/implementer.md`); prompts carry only ticket, branch, integration branch, scratchpad path, files in flight and excerpts.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `MakiNoAtorie/Willow` (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.
