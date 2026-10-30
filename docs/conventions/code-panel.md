# Code panel conventions

Binding rules for the panel, reached from `docs/CONVENTIONS.md`.

- State lives in `session/panel.rs`. New text goes through `set_panel_text`, typing through `splice_panel_text`, regeneration through `regenerated`. Paint reads the `panel_*` queries; values come from `Theme.code`, positions from the core's monospace column grid.
- `panel_input` sees input before the canvas and swallows what it doesn't let through. Focus-leaving paths use `leave_panel()`; `show_panel()` opens without focus.
- The app calls `regenerate(cx)` and `arm_text_check(cx)` next to `arm_recovery`; the core decides what is due. Off-thread jobs carry panel version and history revision and land only if both match.
- Typed text reaches the document only through `apply_text`, then `apply_document` (one step, merged by `History::amend` while `Panel.merge` is set). Paste, cut, a line change and leaving go through `end_step()`; `keep_typing()` comes before anything that steps past typed invalid text; `restore_text()` after `history.undo`/`redo`.
- `History.text` changes only through `settle_text`: every path that makes panel text hold the document calls it. Save and recovery write `held_text()`.
- Line kinds come from `format::check(text)` (`Loaded.lines`, one per `str::lines()`); `Panel.lines` is always filled. Files go through `Session::take_file`, recovery through `Session::recover`.
- `Panel.words` is the last text's direction words; re-placing goes through `replace_by_word` before `place`.
