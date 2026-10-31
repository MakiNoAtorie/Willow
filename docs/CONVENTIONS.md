# Conventions

Binding rules, under 8 KB (`ci.sh`): only rules that bind future work and the code can't tell you.

## Stack
- Rust + GPUI pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003); re-copy Zed's `[patch.crates-io]` block on every pin bump.
- Build needs the Metal Toolchain (macOS) or VS Build Tools + Windows SDK (`fxc`). Keep `gpui_platform/runtime_shaders` off.

## Local checks
- `./ci.sh` / `ci.bat`: fmt, build, test, clippy `-D warnings`, GPUI-free core, Windows-target clippy on macOS. Add new checks to both scripts.
- Worktrees copy the cache (`cp -cR <main>/target <worktree>/target`); never share `CARGO_TARGET_DIR`.

## Layout
- New shape-editing inputs go in `Session::handle_text`, listed in `handle`'s dispatch arm; panel keys go in `panel_key`.

## Model
- `doc.zones` order is draw order, parents first.
- Session code changes structure only through `History::add`/`remove`/`link`/`unlink`/`recolor` and `add_zone`/`remove_zone`/`change_zone` (never members)/`join_zone`/`leave_zone`/`order_zones`; text edits through `apply_document`. `Document::link`/`unlink`/`recolor` are for loaders and generators.
- A multi-change command wraps them in `History::step` (joins an open editing group); commands that open editing call `begin()` and leave closing to `stop_editing`. Never reorder surviving shapes; write a reorder as remove plus add.
- Shapes are removed only through `Session::delete(ids)`, which reconnects chains. ID tie-breaks use `generation_order`.
- Derived indexes cached in `Session` are validated on read.

## Files
- Saved data is `Session::heights()` plus `drawn_positions()`, gathered on the UI thread. Golden files `tests/golden/*.willow`, repairs as `repair-<case>.willow`.
- `Theme.creation_gap` is the one creation gap.
- File state and recovery timing live in `session/file.rs`; the app only does I/O, recovery off the UI thread.
- `file::settle` is the one Save / Don't Save / Cancel routine. Window closes go through `file::close` (or `quit`) and delete recovery via `discard_recovery(cx)`.
- Every `History` method that changes the document bumps `revision`. Call `arm_recovery` after anything that may change the session.

- App preferences are the `Prefs` global (`prefs.rs`), in `prefs::app_dir()`, the one per-user folder: applied in `render`, clamped per window by the core; a change updates the global and calls `cx.refresh_windows()`, saved at gesture end off the UI thread.

## Theme
- Every visual value is a `Theme` field in the core; the app holds no visual constants.
- Adding a theme, a theme field, a bundled font or a View choice: read `docs/conventions/themes.md`.
- Shadows keep `themes.js` values; the app passes `blur / 2` as `blur_radius`.

## Text
- Offsets are UTF-8 bytes; outside offsets are clamped with `floor_char_boundary` on entry. Shape text's selection is set publicly only through `Session::set_text_selection`.
- Layout is the pure `layout(..)` in `layout.rs`; read it through `Session::layout` / `Session::shape_height` (cached on text + width). Edit shape text only through `RichText::replace`.
- The app writes `Session::clipboard_text()` before Cut/Copy and sends `Input::Paste(text)`; the core never touches the clipboard.
- IME marked text (`Editing.marked`) sits outside `History`; `handle` commits it before every input except Mark/Text and view-only inputs. Only the app converts UTF-16 (`text.rs`).

## Input and redraw
- All input goes through `Session::handle(input, now)`. The app calls `cx.notify()` only on `Response.redraw`, animates only while `Session::animating(now)`, else wakes at `Session::next_wake(now)`.
- Positions given to the core are canvas-relative; wheel deltas are content movement in screen px.
- Held-key animations compute position from key timestamps, never summed frames.
- In `dispatch`, every keyboard command on the selection or document joins the `closed` list that closes an open menu; canvas commands ignored while editing join its one guard arm. Key contexts are named, never inline.
- Pointer move/up listen window-wide; down and scroll on the canvas or panel div.
- A non-pointer document change, `Input::FocusLost` and reloads outside `handle` end the pointer gesture via `end_gesture()` (a pan survives); reloads also `stop_editing()`.
- New popups join Esc's close-first chain in `Session::dispatch`, before the leave-editing arm.
- Cmd/Ctrl shortcuts are GPUI actions mapped to `Input` variants. Editing keys live in the `editing` key context; bind bare letters only in a non-editing context (macOS swallows them otherwise).
- Menu items enable via `.when(session.can_x(), …)`; `window.is_action_available` panics during render.
- Keyboard commands reuse gesture commits; selection moves call `reveal(id)` before `select`.
- Viewport and selection never go through `History`; selection is set only via `Session::select(ids)`. Reloads go through `History::replace`, then `drop_missing_selection`.

## Chains
- Menus are core data (`Session::menu()`); clipboard items go through `Willow::choose`.
- `new_shape_at(c)` is the one New shape path.
- Touching placed or drawn positions, the spacing pass, moves or slides: read `docs/conventions/spacing.md`.

## Zones
- Every rule that puts a shape or zone in a zone uses `Session::zone_at(c, keep)`; the holder of X is `Document::parent_zone(id)`. New and dropped zones are ordered by `band::raised`.
- Only mouse drops (`drop_shapes`, the band drop) change membership; `move_selection`/`move_shape` never do.
- `Editing.zone` marks title editing; `rename_zone(id)` is the one way in. Title geometry comes only from `Session::title_size`/`title_at`/`title_width`; add no `Measure` methods.

## Code panel
- Touching `session/panel*.rs`, `format::check` or the app's `panel.rs`: read `docs/conventions/code-panel.md`.

## Drawing
- Canvas paint reads only `Session::draw(from, to, now)`, previews and overlays included; large overlapping runs go in `paint_layer`. Paint order: zone fills, `Frame.zone_ring`, titles, connectors, shapes; draw-query geometry is canvas units, multiplied by `vp.zoom` in paint.

## Tests
- Drive the core's public API; tests of `pub(crate)` types go in `src/<module>/<name>_tests.rs`. Core tests use `tests/common::Fake` (`session_of` / `doc_with`) and `s.viewport().to_screen(..)` for screen points.
- Property tests use the built-in fixed-seed xorshift; large inputs come from `willow_core::generate`. Tests with panel text pending must type it: a paste applies at once.
- Bench: `cargo run --release -p willow-app -- --bench --machine NAME [--theme NAME] [--shapes N]` appends to `bench/results.csv`. Bench rows go in their own commit after the code they measure. Scenarios press only where the core's hit-test (e.g. `band_at`) confirms the target. Time a background job's UI-thread and off-thread parts as separate metrics.
