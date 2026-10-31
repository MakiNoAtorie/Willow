# Theme conventions

Binding rules for themes and View choices, reached from `docs/CONVENTIONS.md`.

- A new theme is a const with `..PAPER` listing only its differences, plus a `THEMES` entry. A new field goes on the struct and `PAPER`, neutral there; others inherit.
- Reuse what Desk and Poster added before adding a field: `ShapeStyle.tab`/`pad_top`, `SelectionStyle.lift_x/lift_y`, `ZoneStyle.pill`/`inset`, `Theme.weave`, `Theme.title_ink()`.
- Paint at the drawn position plus `v.lift`, midpoints too; hit-testing never sees the lift. `Session::title_width` is the only title measure.
- A View choice is one `choice(...)` line in `view_menu.rs`'s `themes()`/`alignments()`, its unit action in that file's `actions!` and a handler in `register`; both platforms read the lists. macOS checkmarks are fixed when `set_menus(cx)` runs, so a switch calls it again.
- Theme and alignment are `Prefs` values applied in `render`, never in `History` or the document. Render guards `Session::set_theme` by name, never pointer; a switch calls `regenerate(cx)` and `arm_recovery`. `layout(..)`'s `Align` only moves x.
- New bundled faces go in `bundled_fonts()` in `main.rs`, which the face-selection test reads. Space Grotesk's slanted faces keep the family name and are selected by style.
