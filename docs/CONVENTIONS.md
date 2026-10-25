# Conventions

Binding rules. `ci.sh` keeps this file under 8 KB: add only rules that bind future work and that the code can't tell you, delete stale ones.

## Stack
- Rust + GPUI pinned to `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (ADR 0003); re-copy Zed's `[patch.crates-io]` block on every pin bump.
- Build needs the Metal Toolchain (macOS) or VS Build Tools + Windows SDK (`fxc`). Keep `gpui_platform/runtime_shaders` off.

## Local checks
- `./ci.sh` / `ci.bat`: fmt, build, test, clippy `-D warnings`, GPUI-free core, Windows-target clippy on macOS. Add new checks to both scripts.
- Worktrees get their own cache: `cp -cR <main>/target <worktree>/target`; never share `CARGO_TARGET_DIR`.

## Layout
- Crates: `crates/willow-core` (GPUI-free) and `crates/willow-app` (binary `willow`).
- Core session: input vocabulary in `input.rs`, text editing in `session/editing.rs`. New editing inputs go in `Session::handle_text` and are listed in `handle`'s dispatch arm.

## Model
- Shapes and zones share `ShapeId` and one generator; `doc.zones` order is draw order, parents first.
- Session code changes structure only through `History::add`/`remove`/`link`/`unlink`/`recolor`/`join_zone`; `Document::link`/`unlink`/`recolor` are for loaders and generators.
- A multi-change command wraps them in `History::step` (joins an open editing group); commands that open editing call `begin()` and leave closing to `stop_editing`. Never reorder surviving shapes; write a reorder as remove plus add. Reuse `side`, `Side::unit`, `Shape::center`, `next_ids`, `Viewport::center`, `format::walks`.
- Shapes are removed only through `Session::delete(ids)`, which reconnects chains. ID tie-breaks use `generation_order`.
- Derived indexes cached in `Session` are validated against the document on read; no mutation path invalidates them.

## Files
- Format: `format.rs`; heights for `save` come from `Session::heights()` on the UI thread. Golden files `tests/golden/*.willow` (git `-text`), repairs as `repair-<case>.willow`.
- Loaded files go through `Session::open` / `disk_changed`. `Theme.creation_gap` is the one creation gap.
- File state and recovery timing live in `session/file.rs`; the app only does I/O (`file.rs`, `recovery.rs` off the UI thread).
- `file::settle` is the one Save / Don't Save / Cancel routine and never deletes; `file::close` / `quit` delete recovery via `discard_recovery(cx)`.
- Every `History` method that changes the document bumps `revision`. Call `arm_recovery` after anything that may change the session; recovery I/O runs through `Autosave::run`, never locking the UI thread.

## Theme
- Every visual value is a `Theme` field in the core; the app holds no visual constants.
- Shadows keep `themes.js` CSS values; the app passes `blur / 2` as GPUI's `blur_radius`.
- Timed animations take a `Transition { duration, easing: CubicBezier }`.

## Text
- Offsets are UTF-8 bytes into `RichText::plain()`. The selection is `Editing.anchor..caret.offset`, set publicly only through `Session::set_text_selection`.
- After any open, call `Session::measure_ahead(n)` from idle so saves don't stall.
- Layout is the pure `layout(..)` in `layout.rs`; read it through `Session::layout` / `Session::shape_height` (cached on text + width). Edits go through `RichText::replace`, which keeps text normalized.
- New justification gap kinds go in the `stretch` closure.
- Style toggles take `edit::Style`; with an empty selection they set `Editing.pending`. A toggle turns on unless the whole selection has it.
- The app writes `Session::clipboard_text()` before Cut/Copy and sends `Input::Paste(text)`; the core never touches the clipboard.
- IME marked text (`Editing.marked`) sits outside `History`; `handle` commits it before every input except Mark/Text and view-only inputs. Only the app converts UTF-16 (`text.rs`).

## Input and redraw
- All input goes through `Session::handle(input, now)`. The app calls `cx.notify()` only on `Response.redraw`, animates only while `Session::animating(now)`, else wakes at `Session::next_wake(now)`; idle is 0 renders.
- Positions given to the core are canvas-relative; wheel deltas are content movement in screen px.
- Arrow key-down is `Input::Navigate`, key-up raw `Input::KeyUp(Key::Arrow)`; held-key animations compute position from key timestamps, never summed frames.
- In `dispatch`, keyboard commands on the selection close an open menu in the check before the `match`; canvas commands ignored while editing join its one guard arm. App key contexts are named (`canvas` = `!editing`, `nav` = `!editing && !picker`, `ed`, `picker`), never inline.
- Pointer move/up listen window-wide (`window.on_mouse_event`); down and scroll on the canvas div. `Input::FocusLost` resets held keys and pans.
- Enter/Esc/Shift+Enter send `Input::Enter`/`Escape`/`NewLine`, resolved by state; new popups join Esc's close-first chain in `Session::dispatch`, before the leave-editing arm.
- Cmd/Ctrl shortcuts are GPUI actions mapped to `Input` variants. Editing keys live in the `editing` key context; bind bare letters only in a non-editing context (macOS swallows them otherwise).
- Menu items enable from handler presence: `.when(session.can_x(), …)` on the focused root. Read core state during render; `window.is_action_available` panics there.
- The Tab picker takes every key via `cx.intercept_keystrokes`; the core closes it before `dispatch`'s `match`.
- Keyboard commands reuse gesture commits (`move_selection`, `resize_shape`, `least_growth`); selection moves call `reveal(id)` before `select`; multi-selections collapse via `first_in_story_order`.
- Previews are queries; the mutation lands once on release through a named `Session` method.
- Undoable changes go through `History` from a named `Session` method; viewport and selection never do. Selection is set only via `Session::select(ids)`. Reloads go through `History::replace`, then `drop_missing_selection`. An editing session is one `begin()`/`end()` group.

## Chains
- Menus are core data (`Session::menu()`); the app's `menu.rs` only draws them from `Theme.menu`. The keyboard color picker is `MenuKind::Picker` (one `Colors` row); popup keys bind in the `picker` key context. Items act on `Menu.shape` / `Menu.link`, set at open; clipboard items go through the app's `Willow::choose`.
- Left press priority: midpoint (10 screen px), alt-autofit, resize edge, shape.
- `new_shape_at(c, push)` is the one New shape path (`push` for keyboard creation).
- Commands that open editing (`create_linked`, New shape) put extra ops between `begin()` and `start_editing`. Copies get `next_ids(n)` before anything is measured.
- Pushes go through `session/push.rs`: saved moves in the open step, never touching zones. Anything that can make a shape taller or wider takes `rect_of(id)` before and calls `make_room(id, old)` after. A pushed block keeps shapes it was already closer than the gap to at that distance; only the change itself pushes to the full gap. Moves, drags and pastes never push.
- A gesture that changes structure calls `stop_editing` first.
- Animations are `Visible` fields computed in the core and counted in `animating`; screen-constant overlays are `Frame` fields painted after shapes.

## Drawing
- Paint reads only `Session::draw(from, to, now)`: everything, including previews, culling and per-shape overlays. Overlays paint with their shape. Paint order: zones (with titles), connectors, shapes; draw-query geometry is canvas units. Cull on position and width before asking for layout.
- Paint multiplies canvas-unit values by `vp.zoom`.

## Tests
- Drive the core's public API. Core tests use `tests/common::Fake` with `session_of` / `doc_with`, and compute screen points with `s.viewport().to_screen(..)`.
- Property tests use the built-in fixed-seed xorshift and print the case number in each assertion.
- Large inputs come from `willow_core::generate(n, seed)`.
- Bench: `cargo run --release -p willow-app -- --bench --machine NAME` appends to `bench/results.csv`. Bench rows go in their own commit after the code they measure.
