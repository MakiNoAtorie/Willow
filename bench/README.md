# Bench

`cargo run --release -p willow-app -- --bench --machine NAME` appends rows to `results.csv` (columns `machine,git_rev,scenario,metric,value,unit`; `git_rev` is `git rev-parse --short HEAD` of the Willow checkout, or `unknown`). Scenarios and metrics are documented in `crates/willow-app/src/bench.rs`.

## MacBook Air M4 (`mba-m4`, #16)

All targets hold with no code changes: draw p95 ≤ 4.6 ms in every pan and zoom scenario, typing draw max 4.1 ms with no late frames, 0 renders while idle.

- Cold pans at 10% and 2% have one 25–105 ms frame: the first layout of every shape that comes into view. p95 is unaffected. The 2% pan uses `set_viewport`; the UI stops at 10%.
- Idle CPU is about 0.57% with 0 renders, the same as an app that was never touched: GPUI's CVDisplayLink thread keeps waking up. That cost isn't Willow's, so nothing was fixed. The 1.3% seen after panning in #6 didn't reproduce in release builds.
