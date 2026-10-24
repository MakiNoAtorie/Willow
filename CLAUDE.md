# Willow

Spec: `willow-spec.md`. Build, test and code conventions: `docs/CONVENTIONS.md`.

Multi-agent work: start ticket implementers with `subagent_type: "implementer"` (`.claude/agents/implementer.md`); prompts carry only ticket, branch, integration branch, scratchpad path, files in flight and excerpts.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `MakiNoAtorie/Willow` (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.
