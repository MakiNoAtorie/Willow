# Spacing conventions

Binding rules for placed and drawn positions, reached from `docs/CONVENTIONS.md`. ADR 0006 has the why.

- `Shape.x/y` is placed. Everything the writer sees reads `Session::drawn_rect(&id)`, `drawn`/`drawn_at` or `Visible.x/y`; whole-document loops use `drawn_top_left(i)`/`drawn_index(i)`. A new read needs a `session/drawn_tests.rs` case. `@layout`, the loader, `create_linked`'s `beside`, resize, zone geometry and the pass itself stay placed.
- `session/drawn.rs` holds the one offset from placed to drawn; slides sit on top of it. `session/spacing.rs` is pure and private (`order`, then `pass`): tested through `Session`, reading only sizes, links, placed positions and the theme's gaps, with ties on placed positions, never story order.
- `Session::spaced()` validates by revision; typing that keeps every height runs no pass. `Session::respace()` forces one, is instant and clears slides: load, reload and a theme switch use it.
- Growth, new shapes and text edits never write placed positions. Only insert-and-push (`Session::push`), word re-placing and moves do.
- Every move is `Session::landing(ids, dx, dy)`, then `drawn::land(h, doc, landing)` in the step. Writing placed anywhere else: `Spaced::slide_moved` for a slide from the old drawn position, otherwise the shape starts at its new placed one.
- A pass starts slides (`Spaced.slides`, read by `sliding`/`animating`). Undo and redo set `Spaced.undone`, so they slide too.
- An open composition counts as growth: every Mark reruns the pass and `heights()` holds it. Marked text changes go through `replace_outside_history`. Recovery waits for `composing()` to end; nothing saved sees marked text.
- Story order goes through `Session::story_order()`; it and direction words read drawn positions, `@layout` placed ones. `format::save(doc, heights, drawn)`: a shape past the end of `drawn` counts as drawn where placed, so the clipboard passes `&[]`.
- Connector ends come only from `draw::connector_ends(DrawnRect, DrawnRect)`.
