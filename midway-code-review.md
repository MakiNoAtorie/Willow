# Midway code review (after Phase 5)

Reviewed at `e10837c` (main, Phases 0–5 merged). Read-only review: no code was changed. `cargo test -p willow-core` was run as a baseline and passes (39 test binaries, 0 failures). Everything else below comes from reading the code against `willow-spec.md`, `docs/CONVENTIONS.md` and the ADRs; items marked *unverified* depend on GPUI or OS behaviour that reading alone can't settle and need a hand check.

Scope: `crates/willow-core` (model, history, format, session commands, text, recovery plan) and `crates/willow-app` (GPUI layer, file I/O, recovery I/O, menus, bench), plus the process around them (CI, bench, issues).

## Summary

The core is in good shape: every spec rule for Phases 2–5 that could be checked in the core has a test, the push, delete and clipboard logic matches the spec, undo grouping is consistent, and the GPUI-free seam (ADR 0001) is held. The real problems cluster in three places:

1. **Session state that outlives a document change.** Gestures store shape *indices*, the marquee keeps a snapshot of IDs, and an open context menu is not closed by every keyboard command. A keyboard Delete or Undo during a drag can panic or act on the wrong shape.
2. **Two data-shape bugs in the model.** Deleting a shape leaves its ID in zone member lists (and the ID is reused), and a style toggle across a paragraph break never turns bold/italic off.
3. **The app layer is untested and partly unmeasured.** Close/quit can prompt twice, Alt+F4 quits the whole app on Windows, the Windows Edit menu never enables Cut/Copy/Paste, and nothing has been benched since Phase 4; the last bench row shows idle renders that the README says are zero.

Suggested order: fix the four confirmed bugs (section 1), then the mid-gesture state resets, then rerun the bench and file the Phase 5 rows, then the Windows pass that is already pending.

## 1. Confirmed bugs

Each of these was verified in the code during this review.

### 1.1 Deleting a shape leaves its ID in `zone.members`; the ID is then reused

- Where: `crates/willow-core/src/history.rs:159` (`History::remove`), reached from `crates/willow-core/src/session/delete.rs:27`.
- `remove` drops the shape and its links but never touches `doc.zones[*].members`. Only `join_zone` edits members. `Document::next_ids` (`lib.rs:66`) checks shape and zone IDs only, and the spec says deleted IDs may be reused.
- Scenario: zone `b` holds shape `c`. Delete `c`. Right-click empty canvas outside any zone, New shape. The new shape gets ID `c` and is silently a member of `b`; save writes it inside `b`'s block and reload confirms it. If the new shape is created inside another zone `e`, it is a member of both `b` and `e`; `save` (`format.rs:526`) and `duplicate` (`clipboard.rs:101`) then disagree on which zone it is in.
- Fix: in `History::remove`, take the ID out of every zone's members and record an `Op::Members` per zone so undo restores it. Same for `Document`-level loaders if any path removes shapes there.
- Tests: none; `tests/delete.rs` never uses zones.

### 1.2 Bold/italic can't be toggled off across a paragraph break

- Where: `crates/willow-core/src/edit.rs:67` (`all_styled`), `edit.rs:84` (`flatten`), called from `session/editing.rs:487`.
- `flatten` gives the `\n` between paragraphs the default style. `all_styled` counts that byte, so any selection spanning a paragraph break is never "all bold", and `toggle` always turns the style *on*.
- Scenario: text `**a**\n**b**`, Select All, Cmd+B. Nothing changes (and nothing is recorded in history), so the keypress appears dead. Shift+Down selections that include the trailing `\n` hit it too.
- Fix: skip bytes where `s.as_bytes()[i] == b'\n'` in `all_styled`.
- Tests: none; every test in `tests/style.rs` is single-paragraph.

### 1.3 A keyboard command during a mouse gesture can panic or hit the wrong shape

- Where: `crates/willow-core/src/session.rs:735`, `:792`, `:814`, `:824` (gesture arms index `self.doc.shapes[index]`); the `DeleteShapes` (`:945`), `Cut` (`:937`) and `Undo`/`Redo` (`:896`) arms never check `self.gesture`.
- `Gesture::{Press, Resize, Midpoint}` hold a shape *index*. Nothing cancels them when the shape list changes.
- Scenario: shapes A, B, C; A selected. Hold the mouse down on C (index 2), press Backspace. A is deleted, two shapes remain. The next pointer move indexes `shapes[2]` and panics; releasing without moving panics at `:792`. With the deleted shape at a lower index, the release moves/selects/edits the wrong shape instead. Cmd+Z of a just-created shape while pressed does the same. The comment at `link.rs:21` ("the document can't change mid-drag") is not true.
- Fix: clear `gesture` and `links.drag` whenever the document's shape list changes (e.g. in `handle` when the revision changed and the input was not pointer input), or store `ShapeId` in gestures.
- Tests: none.

### 1.4 Windows: Alt+F4 is bound to app-wide Quit

- Where: `crates/willow-app/src/main.rs:959`.
- Alt+F4 runs `Quit`, which settles every window and quits. The spec says only closing the *last* window quits on Windows; Alt+F4 is the OS's "close this window".
- Scenario: three windows open; Alt+F4 in one prompts for all three and closes them all. *Unverified*: if GPUI also lets the OS turn Alt+F4 into `WM_CLOSE`, the user gets both a close prompt and a quit prompt.
- Fix: drop the binding and let `on_window_should_close` handle it, or bind Alt+F4 to a per-window close.

### 1.5 Windows Edit menu never enables Cut, Copy or Paste

- Where: `crates/willow-app/src/titlebar_windows.rs:180` (`_ => false`).
- The comment says "As on macOS: … wait for text selection", but macOS now enables them from `can_cut`/`can_copy`/`can_paste` (`main.rs:463-482`).
- Fix: use the same `session.can_*()` values. Log on #53.

## 2. Likely bugs (need a hand check or a GPUI/OS confirmation)

### 2.1 FocusLost only ends a pan

- `session.rs:888`: on `Input::FocusLost` only `Gesture::Pan` is dropped. Press, Resize, Midpoint, Marquee and Text survive.
- Scenario: press on a shape, Cmd/Alt+Tab away, release there, come back. Every later press sees `gesture.is_some()` and starts nothing; moves keep previewing the old drag; the next left release moves the shape to wherever the pointer is. Whether the OS swallows the release is *unverified*, but the input doc at `input.rs` already says it "may never arrive".
- Fix: clear any gesture and `links.drag` on FocusLost.

### 2.2 The marquee can bring deleted IDs back into the selection

- `session/select.rs:63`, `:83`: `marquee_base` is captured at press and re-unioned on every move. Undo or Delete mid-drag runs `drop_missing_selection`, but the next `marquee_to` re-selects the stale IDs.
- Scenario: select X, Shift+drag from empty canvas, Backspace mid-drag, move. `selection()` contains X again; `only_selected()` returns a missing ID, Tab opens a picker that draws nothing, `can_cut()` is true. A later new shape can inherit the stale ID (see 1.1).
- Fix: filter `marquee_base` in `drop_missing_selection`, or cancel the gesture (1.3).

### 2.3 Some keyboard commands leave an open context menu stale; menu Cut can delete the edited shape

- `session.rs:602-614`: the `closed` list omits `DeleteShapes`, `NewBeside`, `Undo` and `Redo`. The app's menu only `.occlude()`s (`willow-app/src/menu.rs`), so keys reach the session while it is open. `choose(Cut | Delete)` (`session/menu.rs:172`) deletes `self.selected` without checking `editing`.
- Scenario: right-click shape S (menu opens), press Cmd+Shift+N (new shape N, now editing), click Cut in the still-open menu. N is deleted inside its own editing group; `editing` points at a missing shape, the key context stays `editing`, typing goes nowhere until Esc. Milder: right-click S, Backspace; the menu stays open on a deleted shape.
- Fix: add those inputs to `closed`; have `delete` call `stop_editing` when the edited shape is among `ids`.

### 2.4 Close and quit can run twice for the same window

- `file.rs:187-252`, `main.rs:1149`, `main.rs:1114`: nothing guards `close`/`quit` against re-entry.
- Scenarios: Cmd+Q twice while the first Save prompt is up starts a second quit loop that prompts every window again; closing a window by hand while a quit prompt is on another window prompts it twice; on Windows the close button may stay clickable during a prompt. *Unverified*: whether AppKit queues a second sheet and whether a GPUI prompt on Windows disables its owner.
- Fix: an app-wide `quitting` flag plus a per-window `settling` flag checked in `close` and `settle`.

### 2.5 A held keyboard pan can run forever on macOS (unverified)

- `session/keypan.rs:49-68`, `session.rs:880`: the pan stops only on `KeyUp(Arrow)` or FocusLost. AppKit does not deliver keyUp for a key released while Cmd is held.
- Scenario: nothing selected, hold Down, press Cmd, release Down, release Cmd. The press never gets `up`; `animating` stays true and the view drifts at ~900 px/s until Down is pressed and released again.
- Fix: release held presses when modifiers change or a Nudge arrives, or add a repeat-timeout fallback.

### 2.6 A second Willow process takes over the first one's live recovery files

- `willow-app/src/recovery.rs:192-221`: launch opens or deletes every recovery file in the folder with no lock and no owner check.
- Scenario: a second instance (Windows has no single-instance rule; `open -n` on macOS) opens the first instance's unsaved documents under the same IDs; both write the same `<id>.willow`; Don't Save in one deletes the other's crash protection.
- Fix: a lock file per ID, or the PID in the sidecar and skip IDs held by a live process.

### 2.7 Save As appends `.willow` after the panel confirmed the name

- `file.rs:106-109`: if the panel returns `story`, the app writes `story.willow`, so an existing `story.willow` can be overwritten without the panel's Replace prompt. *Unverified* whether rfd adds the extension itself on Windows; macOS's NSSavePanel usually does.

### 2.8 A malformed zone line throws off brace matching on load

- `format.rs:97` falling through to `:133`: when `parse_zone` fails (unknown color, bad ID), the line is dropped but nothing is pushed onto `open`, so its `}` closes the *enclosing* zone. Deep and duplicate zones already push `None` for exactly this reason.
- Scenario: `zone p "P" {` / `zone q "Q" color purple {` / `a: "A"` / `}` / `b: "B"` / `}` → `b` ends up loose at top level. Will become common with Phase 7's hand-edited text.

## 3. Spec deviations and gaps

| # | Spec | Code | Note |
|---|------|------|------|
| 3.1 | Open "reuses a blank, untouched untitled window" | `file.rs:69` reuses only the *active* blank window | A blank window elsewhere spawns a third. *Unverified*: whether the panel is still key when the completion runs, in which case no reuse happens at all. |
| 3.2 | Quit on "closing the last window on Windows" | `main.rs:1117` `QuitMode::LastWindowClosed` on both platforms | macOS quits when the last window closes; non-native. |
| 3.3 | "native menu bar (File, Edit, View)" and Cmd+W | `main.rs:914-935`: no View menu, no Close item | View has nothing until Phase 8; Close is missing now, so a window can't be closed from the keyboard. |
| 3.4 | "Canvas shortcuts never fire while editing" | Cmd/Ctrl+0 (`main.rs:971`, no key context) and Ctrl/Cmd+wheel zoom (`session.rs:963`, before the editing guard) fire while editing | Spec forbids *panning* while editing; zoom is unspecified. Decide, then guard or write the sentence. |
| 3.5 | "typing any character except Space" starts editing | `editing.rs:195` `!text.trim().is_empty()` | Any whitespace-only input (NBSP, U+3000, tab) is ignored, not just Space. Probably fine; say so in the spec. |
| 3.6 | A new shape landing on "a shape not linked on that side" is pushed clear | `push.rs:221` skips everything in the source's own chain | A midpoint-created shape that lands on a cousin in its own chain overlaps it. The spec wording ("that shape's whole chain") can't apply to the own chain, so this needs a ruling first. |
| 3.7 | Arrow with "none [in that direction], nothing happens" | `nav.rs:121` still calls `reveal(from)` | Select a shape, wheel it off screen, press an arrow toward empty space: the view eases back to it. |
| 3.8 | Phase 3 "Change color" heading | `session/menu.rs:283` label is "Color" | Wording only. |
| 3.9 | #57 connectors slant after resize | `chain.rs:44` centres new shapes on the side midpoint; resize moves it | Open issue; spec silent. |
| 3.10 | Unknown color name on a linked line | `format.rs:130`, `:425`: `parse_tail` fails whole, dropping the link too | `b: "B" below a color skyy` splits the chain. Parse link and color separately. |
| 3.11 | Conflicting chain colors "resolve to the first in story order" | Writer puts the color on the first *written* line; loader resolves by walk order from the topmost shape | They differ for a chain split across zone blocks. Only hand-edited files hit it. |
| 3.12 | Header "made by a newer Willow" | `format.rs:70`: `willow 2.0` → "not a Willow file"; `willow 01` and `willow +1` load as version 1 | Nit. |

## 4. Performance, bench and Windows

- **Phase 5 was never benched.** Latest rows in `bench/results.csv` are at `4cf2ae8` ("Phase 4 review fixes"); 40 commits and the whole keyboard flow (keyboard pan, follow, picker, slides) are unmeasured. The Phase 2 done-when and the spec targets are therefore unverified for the current main.
- **Idle regression or noise at `4cf2ae8`:** `idle_after_pan` reports 3 renders and 2.886% CPU; every earlier run has 0 renders and 0.57–0.76%. `bench/README.md:7,20` still says "0 renders while idle" and does not mention this run. Rerun to classify.
- **"Cold" pans are not cold** (`bench.rs:250` → `main.rs:242` → `session.rs:350`): `files()` calls `show()`, which starts `measure_ahead`, so later "cold" scenarios find layouts already cached and the first scenarios compete with the 4 ms layout batches. Either wait for `measure_ahead` before the pans and drop the "first layout" claim, or don't start it after the bench load.
- **Autosave's UI-thread part is unmeasured.** `take_recovery` clones the document and calls `shape_height` for every shape on the UI thread (`session/file.rs:127`, `app/recovery.rs:133`). The spec allows this only if "measured to stay well under one frame"; add a bench row.
- **Windows is entirely unverified.** #17, #18, #28, #42, #53 are open; no `win-*` rows exist; the Windows title bar, menu button, rfd panels, recovery dir, IME, mixed DPI and LF endings are type-checked only. Items 1.4, 1.5, 2.4, 2.7 and the `\\?\` path note below belong on #53.
- `fs::canonicalize` on Windows returns `\\?\C:\…` paths (`file.rs:46`, `:100`, `:129`); they are stored, written to `.path` sidecars and passed to rfd's `set_directory`, which may not accept them. *Unverified*.

## 5. Missing tests (highest value first)

Core (cheap, through the public API):

1. Delete a zone member, create or paste a shape, save, reload: the new shape must not be in the old zone (1.1).
2. Style toggle on a selection crossing `\n`, all-bold and mixed (1.2).
3. Backspace / Cmd+Z during a held Press, Resize or Midpoint gesture: no panic, the right shape acts (1.3). Undo or Delete mid-marquee keeps `selection()` a subset of the document (2.2).
4. With a context menu open: DeleteShapes, NewBeside, Undo close it; `MenuChoose(Cut)` never deletes the edited shape (2.3). FocusLost during Press/Resize/Marquee clears the gesture (2.1).
5. A malformed zone line nested in a zone keeps later siblings in the parent (2.8); an unknown color on a linked line keeps the link (3.10); conflicting colors in a chain split across blocks (3.11).
6. Clusters of several code points (decomposed `e\u{301}`, ZWJ emoji, flag pairs) through Left/Right, Backspace and word jumps; only precomposed `é` is covered today.
7. Two or more trailing spaces on a full justified line: caret stops and selection rects stay within the width (`layout.rs:243`, `:294` only clamp `end_x`).
8. `generate` never produces `"`, `*`, `\`, line breaks, loose top-level chains or empty paragraphs, so escaping is checked only by `text.willow`; adding them to the generator covers them across all seeds.
9. `launch_plan` with `recovered_at == document_modified` (currently opens; nothing pins it); `History::replace` while a group is open, then undo/redo.

App (none exist beyond `tests/overlays.rs`; these are hand-check items until there is a seam):

10. UTF-16 ↔ UTF-8 (`text.rs` `from16`/`to16`) with surrogate pairs and mid-pair offsets. This is pure code and could move behind a test.
11. Open reuse/focus, quit per-document prompting, Save As, atomic write + alert, `check_disk` on focus, recovery write/delete ordering.

## 6. Smaller findings

Core:

- `edit.rs:47` / `:58`: typed text at the start of a paragraph takes the style of the byte *after* the range, so Select All over bold text and typing gives plain text, and a new paragraph after a bold one is plain. The doc comment describes this rule, so it may be intended; most editors keep the replaced text's style.
- `session/editing.rs:65`, `:497`, `edit.rs:44`: the core trusts input ranges. An out-of-range `Mark` sets `marked`/caret past the end; the next `Move(Left)` or `unmark` panics on a slice. Not reachable from GPUI today (`from16` clamps), reachable through the public `set_text_selection`. Clamp in the core.
- `layout.rs:226`, `editing.rs:137`: U+2028/U+2029 survive the paste filter; `linebreaks` reports them mandatory but layout treats them as ordinary opportunities, so no break shows.
- `editing.rs:113` via `set_caret` at `:340`: Shift+Enter after a pending Cmd+B drops the pending style.
- `editing.rs:137-139` duplicates `clipboard::clean` (`clipboard.rs:176`); reuse it.
- `edit.rs:135-151`: word motion treats emoji/symbols as part of the word ("hi 👍" + Delete(WordLeft) deletes both). Check against platform convention.
- `session.rs:291`: the layout cache key is ID + text + width, with no theme. Fine while `theme: &'static` is fixed at `Session::new`; a Phase 8 theme switch must clear `layouts`.
- `theme.rs:513-519`: Paper's `fade_end` 0.5 × 15 = 7.5 px is consistent with ADR 0003's 7 px cutoff, but the thresholds are zoom ratios; a later theme with a font under 14 units would draw text below 7 px. Derive from screen px, or test per theme.
- `session/file.rs:78`: a reload whose bytes differ but whose document is identical (CRLF rewrite by git, re-indent) still pushes a no-op undo step and stops editing. Skip `replace` when `loaded.document == self.doc`.
- `format.rs:398-420`: layout numbers are not range-checked; `w0` or x near `i32::MAX` load without repair, and `clipboard.rs:106` / `beside.rs:19` add `x + width as i32`, which overflows (debug panic, release wrap; *unverified* at runtime).
- `session.rs:556`: "first resize" means size == (0,0), so a 0×0 resize (minimised window on Windows) followed by a restore re-centres the view on shape 1. *Unverified*. Use a `sized_once` flag.
- `history.rs:219-225`: the doc comment "Opens a group…" sits on `is_open`; `begin` has none.

App:

- `file.rs:12`, `:26`, `:103`: the rfd dialogs have no parent window, so the window stays usable with a panel open (Cmd+S twice on an untitled doc opens two save panels; closing during settle's Save panel starts another close). *Unverified* whether rfd without a parent is modal on macOS.
- `file.rs:163`: `mtime` equality short-circuits before reading. On 1–2 s filesystems (HFS+, exFAT) an outside rewrite within the same tick is never seen and the next Save overwrites it. Comparing size too is cheap.
- `file.rs:171`, `session/file.rs:71`: while the file is missing, every focus calls `disk_changed(None)` → `changed(self.now)`, re-arming a recovery write each time; `self.now` is the last input time, so the write is already overdue and fires at once. `check_disk` also notifies when nothing changed.
- `file.rs:147`, `recovery.rs:100`, `:165`: nothing cleans up a `.<name>.tmp` left by a crash mid-save, an orphan `<id>.path` without its `.willow`, or a stray `.<id>.willow.tmp`; `scan` never lists them. Unreadable recovery files are skipped at launch and kept forever.
- `recovery.rs:76`, `:103`: if the recovery folder can't be created or `HOME`/`APPDATA` is unset, the only sign is an `eprintln!`; crash protection is silently off.
- `main.rs:378-385`: pointer moves from outside the canvas (title bar on Windows, open app menu) are passed to the core, so shapes under a menu can show hover.
- `main.rs:1114`: Quit from the Dock or at logout runs GPUI's own path, not `file::quit`; if GPUI quits straight away, up to ~5 s of changes since the last recovery write are lost without a prompt. *Unverified*.
- `text.rs:104-114`: the word-advance cache splits only at ASCII space, so a CJK paragraph is one "word" per run and every keystroke or IME update adds a whole-paragraph entry; the cache is never evicted (roughly 7.5 MB for a 1,000-character paragraph). `text.rs:140-174` `paint_text` also walks every glyph of the paragraph per line (O(lines × glyphs) per frame).
- `bench.rs:68`: CSV fields are not escaped; a `--machine` name with a comma breaks the row.
- `tests/overlays.rs:16`: the inset check accepts any `.top`/`.left` in the next 6 lines, including a child's, and `read_to_string` panics if `src/` ever gets a subdirectory.
- `main.rs:105` `WHEEL_LINE` and `session.rs:78`, `:81`, `midpoints.rs:11` (`DRAG_START`, `EDGE_GRAB`, `MIDPOINT_HIT`) sit outside `Theme`. Input thresholds, not visuals, so arguably fine; `MIDPOINT_HIT` is spec-fixed.

## 7. Process notes

- `ci.sh` and `ci.bat` are in step (fmt, build, test, clippy, GPUI-free core, CONVENTIONS size); the pre-push hook is installed. `cargo test` passes; `./ci.sh` was not rerun here.
- No `TODO`/`FIXME`/`#[ignore]` in `crates/`. 13 `ponytail:` ceiling notes mark deliberate O(n) scans (marquee, connector hit test, chain labelling, `is_saved` whole-document compare, `link_color`); none is a problem at 10k shapes per the last bench, but all are untested at Phase 5's command mix.
- `#[allow]`s are all justified (`dead_code` on shared test helpers, two `type_complexity`, one `new_without_default` where `Default` and `new` legitimately differ).
- Open issues: #57 (connector slant), #17/#18/#28/#42/#53 (Windows checks), #4 (Phase 2 spec issue still open although Phase 2 is done; close or re-scope).

## 8. Suggested tickets

1. **Mid-gesture document changes** (1.3, 2.1, 2.2, 2.3): one ticket; reset gestures, link drag and `marquee_base` when the shape list changes or focus is lost; close menus on every keyboard command; `delete` stops editing when it deletes the edited shape. Tests 3 and 4 above.
2. **Zone membership on delete** (1.1): `History::remove` prunes members with an undoable `Op::Members`. Test 1.
3. **Style toggle across paragraphs** (1.2), with the paragraph-start style rule decided at the same time. Test 2.
4. **Bench rerun for Phase 5** plus the idle-renders question, the "cold" fix and an autosave row; update `bench/README.md`.
5. **Loader robustness for Phase 7** (2.8, 3.10, 3.11, 3.12, layout range checks): small, and worth doing before the code panel makes hand-edited files normal. Test 5.
6. **Windows pass** (#53): add 1.4, 1.5, 2.4, 2.7, the `\\?\` path note and the Alt+F4 double-prompt check to the issue.
7. **Spec rulings needed**: zoom while editing (3.4), own-chain landing on midpoint creation (3.6), arrow toward empty space (3.7), macOS last-window quit (3.2), blank-window reuse scope (3.1), connector slant (#57).
