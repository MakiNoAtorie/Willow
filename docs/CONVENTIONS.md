# Conventions

Binding rules, under 8 KB (`ci.sh`): only rules that bind future work and the code can't tell you.

## Stack
- Rust + GPUI pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003); re-copy Zed's `[patch.crates-io]` block on every pin bump.
- Build needs the Metal Toolchain (macOS) or VS Build Tools + Windows SDK (`fxc`). Keep `gpui_platform/runtime_shaders` off.

## Local checks
- `./ci.sh` / `ci.bat`: fmt, build, test, clippy `-D warnings`, GPUI-free core, Windows-target clippy on macOS. Add new checks to both scripts.
- Worktrees copy the cache (`cp -cR <main>/target <worktree>/target`); never share `CARGO_TARGET_DIR`.

## Layout
- New editing inputs go in `Session::handle_text` and are listed in `handle`'s dispatch arm.

## Model
- `doc.zones` order is draw order, parents first.
- Session code changes structure only through `History::add`/`remove`/`link`/`unlink`/`recolor` and `add_zone`/`remove_zone`/`change_zone` (never members)/`join_zone`/`leave_zone`/`order_zones`; `Document::link`/`unlink`/`recolor` are for loaders and generators.
- A multi-change command wraps them in `History::step` (joins an open editing group); commands that open editing call `begin()` and leave closing to `stop_editing`. Never reorder surviving shapes; write a reorder as remove plus add.
- Shapes are removed only through `Session::delete(ids)`, which reconnects chains. ID tie-breaks use `generation_order`.
- Derived indexes cached in `Session` are validated on read; no mutation path invalidates them.

## Files
- Heights for `save` come from `Session::heights()` on the UI thread. Golden files `tests/golden/*.willow` (git `-text`), repairs as `repair-<case>.willow`.
- Loaded files go through `Session::open` / `disk_changed`. `Theme.creation_gap` is the one creation gap.
- File state and recovery timing live in `session/file.rs`; the app only does I/O, recovery off the UI thread.
- `file::settle` is the one Save / Don't Save / Cancel routine and never deletes. Window closes go through `file::close` (or `quit`), guarded by `Willow.settling` / `file::QUITTING`, and delete recovery via `discard_recovery(cx)`.
- Every `History` method that changes the document bumps `revision`. Call `arm_recovery` after anything that may change the session; recovery I/O runs through `Autosave::run`.

## Theme
- Every visual value is a `Theme` field in the core; the app holds no visual constants.
- Shadows keep `themes.js` CSS values; the app passes `blur / 2` as GPUI's `blur_radius`.

## Text
- Offsets are UTF-8 bytes into `RichText::plain()`; outside offsets (`Input::Text`/`Mark`, `set_text_selection`) are clamped with `floor_char_boundary` on entry. The selection is `Editing.anchor..caret.offset`, set publicly only through `Session::set_text_selection`.
- After any open, call `Session::measure_ahead(n)` from idle.
- Layout is the pure `layout(..)` in `layout.rs`; read it through `Session::layout` / `Session::shape_height` (cached on text + width). Edit shape text only through `RichText::replace`.
- Style toggles take `edit::Style`; with an empty selection they set `Editing.pending`.
- The app writes `Session::clipboard_text()` before Cut/Copy and sends `Input::Paste(text)`; the core never touches the clipboard.
- IME marked text (`Editing.marked`) sits outside `History`; `handle` commits it before every input except Mark/Text and view-only inputs. Only the app converts UTF-16 (`text.rs`).

## Input and redraw
- All input goes through `Session::handle(input, now)`. The app calls `cx.notify()` only on `Response.redraw`, animates only while `Session::animating(now)`, else wakes at `Session::next_wake(now)`.
- Positions given to the core are canvas-relative; wheel deltas are content movement in screen px.
- Arrow key-down is `Input::Navigate`, key-up raw `Input::KeyUp(Key::Arrow)`; held-key animations compute position from key timestamps, never summed frames.
- In `dispatch`, every keyboard command on the selection or document (incl. delete, new-beside, undo, redo) is in the `closed` list that closes an open menu before the `match`; canvas commands ignored while editing join its one guard arm. Key contexts are named, never inline.
- Pointer move/up listen window-wide; down and scroll on the canvas div.
- A non-pointer document change, `Input::FocusLost` and reloads outside `handle` end the pointer gesture via `end_gesture()` (a pan survives); reloads also `stop_editing()`.
- `Input::ModifiersChanged` (from the canvas `on_modifiers_changed`) is view-only; it and `Nudge`/`NudgeSize` release held arrows at the top of `dispatch`.
- Enter/Esc/Shift+Enter send `Input::Enter`/`Escape`/`NewLine`, resolved by state; new popups join Esc's close-first chain in `Session::dispatch`, before the leave-editing arm.
- Cmd/Ctrl shortcuts are GPUI actions mapped to `Input` variants. Editing keys live in the `editing` key context; bind bare letters only in a non-editing context (macOS swallows them otherwise).
- Menu items enable from handler presence: `.when(session.can_x(), …)` on the focused root. `window.is_action_available` panics during render.
- Keyboard commands reuse gesture commits (`move_selection`, `resize_shape`); selection moves call `reveal(id)` before `select`; multi-selections collapse via `first_in_story_order`.
- Previews are queries; the mutation lands once on release through a named `Session` method.
- Viewport and selection never go through `History`; selection is set only via `Session::select(ids)`. Reloads go through `History::replace`, then `drop_missing_selection`. An editing session is one `begin()`/`end()` group.

## Chains
- Menus are core data (`Session::menu()`); items act on `Menu.shape` / `Menu.link`, set at open; clipboard items go through `Willow::choose`.
- Left press priority: midpoint (10 screen px), alt-autofit, resize edge, zone edge, shape, band. `new_shape_at(c, push)` is the one New shape path.
- Commands that open editing (`create_linked`, New shape) put extra ops between `begin()` and `start_editing`. Copies take `next_ids(n)` before anything is measured.
- Pushes go through `session/push.rs`: saved moves in the open step, never touching zones. Anything that can make a shape taller or wider takes `rect_of(id)` before and calls `make_room(id, old)` after. Moves, drags and pastes never push.
- A gesture that changes structure calls `stop_editing` first.

## Zones
- Every rule that puts a shape or zone in a zone uses `Session::zone_at(c, keep)` (`innermost_zone`: the top-drawn zone there); the holder of X is `Document::parent_zone(id)`. New and dropped zones are ordered by `band::raised`.
- Only mouse drops (`drop_shapes`, the band drop) change membership; `move_selection`/`move_shape` never do.
- Zone hits are `zone_edge_at`, then `band_at`; neither hits under a shape. `Frame.zone_ring` is the one ring: chain sources with `.or(..)`.
- `Editing.zone` marks title editing (`Editing.id` is a zone); code on edited text branches on it. `rename_zone(id)` is the one way in. Title geometry comes only from `Session::title_size`/`title_at`/`title_width` (`Measure::advances`; add no `Measure` methods).

## Drawing
- Paint reads only `Session::draw(from, to, now)`: everything, including previews, culling and per-shape overlays. Paint order: every zone fill, `Frame.zone_ring`, shown titles (`VisibleZone.show_title`; edit highlight under, caret over), connectors, shapes; draw-query geometry is canvas units, multiplied by `vp.zoom` in paint.

## Tests
- Drive the core's public API; tests of `pub(crate)` types go in `src/<module>/<name>_tests.rs`. Core tests use `tests/common::Fake` (`session_of` / `doc_with`) and `s.viewport().to_screen(..)` for screen points.
- Property tests use the built-in fixed-seed xorshift and print the case number in each assertion; large inputs come from `willow_core::generate`.
- Bench: `cargo run --release -p willow-app -- --bench --machine NAME` appends to `bench/results.csv`. Bench rows go in their own commit after the code they measure. Scenarios press only where the core's hit-test (e.g. `band_at`) confirms the target.
