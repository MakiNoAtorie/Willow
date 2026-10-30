# Bench

`cargo run --release -p willow-app -- --bench --machine NAME` appends rows to `results.csv` (columns `machine,git_rev,scenario,metric,value,unit`; `git_rev` is `git rev-parse --short HEAD` of the Willow checkout, or `unknown`). Scenarios and metrics are documented in `crates/willow-app/src/bench.rs`.

Each `pan_zoom*_cold` row starts on a new session that has laid nothing out, with no idle `measure_ahead` running, so it times the first layout of every shape in view; the matching `_warm` row pans back over the same ground. No `measure_ahead` runs during any scenario after `open_first_frame`. Rows from `324dac2` to `4cf2ae8` ran their cold pans after `measure_ahead` had laid out much of the document, so those cold rows are not cold. `recovery_take` is the UI-thread part of an autosave (`Session::take_recovery`: clone the document, every shape's height).

## MacBook Air M4 (`mba-m4`, #16)

All targets hold with no code changes: draw p95 ≤ 4.6 ms in every pan and zoom scenario, typing draw max 4.1 ms with no late frames, 0 renders while idle.

- Cold pans at 10% and 2% have one 25–105 ms frame: the first layout of every shape that comes into view. p95 is unaffected. The 2% pan uses `set_viewport`; the UI stops at 10%.
- Idle CPU is about 0.57% with 0 renders, the same as an app that was never touched: GPUI's CVDisplayLink thread keeps waking up. That cost isn't Willow's, so nothing was fixed. The 1.3% seen after panning in #6 didn't reproduce in release builds.
- Rerun at `a610176` (two runs, after the #4 review fixes): idle still 0 renders. Draw p95 moves 0.4–1.2 ms between runs of the same build (pan at 2% warm 4.9 then 4.1 ms, zoom sweep warm 5.0 then 4.2 ms), so a single run near 4.6 ms is noise; typing draw p95 is 4.3 ms in both.

## Save and load, 10,000 shapes (`mba-m4`, #27)

Three runs at `324dac2`, load average 4–5 (a system daemon was using about two cores): `save` 11.0, 16.1, 13.3 ms; `load` 16.6, 18.8, 17.8 ms; `open_first_frame` draw 1.6–1.8 ms.

- A save needs every shape's height, so before #27 the first save after opening laid out all 10,000 shapes: about 306 ms, nearly all of it text shaping. Now `TextMeasure` shapes each word once (same advances and saved bytes on the generated document), ASCII skips the grapheme segmenter, and the app lays the document out 200 shapes per idle step after opening (`Session::measure_ahead`, no redraws).
- `save_unwarmed` (a save the instant a file opens, before the idle layout has run) is 35–55 ms. About 20 ms of it is the CPU waking from idle: the same save right after 100 ms of busy work takes 32–35 ms.
- Laying everything out ahead raises idle RSS from about 135 MB to 125–176 MB.
- Phase 2 targets still hold: pan and zoom draw p95 ≤ 6.1 ms; typing draw max ≤ 5.5 ms with no late frames; 0 renders while idle. The exception is the warm 2% pan at 8.0–9.6 ms. That comes from zone titles, and 2% is below the UI's 10% minimum zoom.

## Review fixes (`mba-m4`, #58, rows at `1ae1547`)

First run on the cold/warm split from #65 and the first since Phase 5. Every target holds: pan and zoom draw p95 3.7–5.7 ms (the 2% warm pan, 5.7 ms, is now under the 8 ms target too); typing p95 3.9 ms, max 5.1 ms, no late frames; `recovery_take` (the UI-thread part of autosave on 10k shapes) 7.7 ms; save 11.2 ms, load 17.6 ms; 0 renders while idle after both the pan and the animation, so the 3 renders in the `4cf2ae8` run did not reproduce. The cold pans now show the real first-layout cost: one 16–24 ms frame at 10% and 2% (one late frame at 2%), p95 unaffected.

## Zone drag, 10,000 shapes (`mba-m4`, #76, rows at `af238a8`)

`zone_drag` presses the title band of the largest zone (about 490 zones in the document), zoomed to fit it on screen, moves 240 times out and back, and drops it. Two runs: `draw_p95` 6.0 and 7.2 ms (target ≤ 8 ms), `draw_p50` 4.7 and 4.5 ms, `draw_max` 19 and 26 ms, `dirty_to_present_p95` 9.3 and 9.5 ms, one late frame each.

- The first run of the scenario showed a 2.05 s frame: the drop. `History::order_zones` looked up each zone's parent by scanning every zone's member list, inside a loop over all zone pairs. It now hashes the parents once; the drop is milliseconds.
- The same first run pressed 15 px into the band, which probably hit a member shape, so its p95 (5.2–5.6 ms) was a shape drag's. The bench now picks the press point with `Session::band_at` and fails if none hits the zone.

## Code panel, 10,000 shapes (`mba-m4`, #88, rows at `2d7ea6a`)

The panel text of the generated document is about 1 MB. Every target holds: panel typing `draw_p95` 5.0 ms (target ≤ 8 ms), no late frames, each keystroke handled in ≤ 0.33 ms; applying text: `check` 19 ms off the UI thread plus `apply` 5.4 ms on it (target ≤ 100 ms); regeneration `run` 5.9 ms off the UI thread (target ≤ 50 ms), `take` 2.2 ms on it; `idle_panel_open` 0 renders, 0.57% CPU, the same as the canvas idle rows. The other scenarios are unchanged (draw p95 ≤ 6.3 ms).

- Before the fix each panel keystroke wrapped the whole text again: about 11 ms per key, outside `draw`. Typing now rewraps only the lines it touches (about 0.3 ms per key). A full rewrap (new text from a regeneration, or a panel width step) is about 3 ms, since lines that fit skip wrapping and ASCII lines skip the grapheme segmenter.
- The first run with #86 merged showed `apply` at 189 ms: the direction-word check scanned every link once per shape. The links are hashed once now.
- `undo` (Undo in the panel after an applied edit) is 10.7 ms on the UI thread: it checks the 1 MB text it brings back. No target covers it; it is one late frame per Undo.
- `dirty_to_present_p95` while panel typing is 15 ms (canvas typing in the same run: 18 ms), with no late frames.
