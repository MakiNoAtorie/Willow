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
