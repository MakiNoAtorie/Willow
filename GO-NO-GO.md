# Phase 0: GPUI go/no-go

**Verdict: GO on macOS and Windows.** Every question was answered yes on the MacBook Air M4 and on the Windows desktop (Ryzen 5 9600X, RTX 5060 Ti, [spike-gpui/WINDOWS-CHECKLIST.md](spike-gpui/WINDOWS-CHECKLIST.md)). Nothing failed at the platform layer. Still untested: mixed DPI (no second monitor), a Windows precision touchpad (none; the Mac trackpad passed), the real-IME and dead-key checks on the Mac, and the iGPU run. Two caveats the owner should decide on:

- Text has to stop drawing below about 7 px on screen for the zoom sweep to meet 8 ms.
- The keystroke-to-paint target as written is bounded by vsync pacing, not by GPUI.

- **GPUI**: `zed-industries/zed` @ `79fe6a645f68f8c9230578f154bf99852d9f716f` (main, committed 2026-10-23, picked 2026-10-24). GPUI is 0.2.2 in-tree and is now split into `gpui` + `gpui_platform` (+ `gpui_macos`/`gpui_windows`).
- **Run**: `cargo run --release -p spike-gpui` (demo), `-- --large` (10k shapes; `L` also loads it), `-- --bench --machine NAME`, `-- --selftest`.
- **Evidence**: `spike-gpui/results.csv` (columns machine, gpui_rev, scenario, metric, value, unit), `spike-gpui/evidence/` (screenshots and self-test output).
- **Build note**: this Mac has no Metal Toolchain, so the spike enables `gpui_platform/runtime_shaders`, which compiles shaders at launch. It affects startup only. The real app should install the toolchain (`xcodebuild -downloadComponent MetalToolchain`) and drop the feature.

## How it was measured

`--bench` opens a real window on the generated 10,000-shape document and scripts everything:

- pans of 240 frames at zoom 100 %, 40 %, 10 % and 2 %, each run cold (first visit) then warm;
- a 100 % → 2 % → 100 % zoom sweep, twice;
- 200 keystrokes typed into a shape;
- then idle.

Frame numbers come from GPUI's own profiler (`profiler` feature):

- `draw` is `Window::draw`: layout, prepaint and paint, all CPU.
- `dirty_to_present` runs from invalidation until the frame is handed to Metal.

GPU execution time is not separately measured. Present intervals stayed on vsync (p95 17.4–18.7 ms on the 60 Hz panel; no 33 ms frames showed in the p95). Two runs: text cut off below 4 px on screen (the default) and below 7 px (`--text-cutoff 7`).

## 1. Justified text with mixed bold/italic, edited live while the shape grows: **GO**

GPUI only has left/centre/right alignment, so `spike-gpui/src/text.rs` (~200 lines) does its own layout:

- Each paragraph is shaped once as one line (`WindowTextSystem::layout_line`, which handles mixed fonts in one run list).
- The line is broken greedily at spaces using the shaped glyph x positions.
- Each line's spare width is spread across its inner spaces.
- Glyphs are painted one at a time with `Window::paint_glyph` / `paint_emoji`.

These are public APIs, and they are the same calls GPUI's own `ShapedLine::paint` makes, so there's no performance penalty for going custom. Caret stops and hit-testing come from the same positioned-glyph table.

- Evidence: `evidence/mac-1-demo.png` (justified, bold/italic mixed), `mac-3-bold.png` (bolding re-justifies), `mac-4-grown.png`. Self-test: `typing grows the shape (144 -> 184)`.
- Typing in a 10k document: `typing_10k draw_p95` is 4.6 ms, so relayout plus repaint per keystroke is cheap.
- Gaps that are our code, not GPUI's:
  - CJK breaks only at a character when nothing else fits, so it isn't justified and line-start punctuation is allowed (`。` starts a line in `mac-1-demo.png`). Proper UAX #14 breaking, kinsoku and inter-character justification are a Phase 1 job (e.g. the `unicode-linebreak` crate).
  - Hyphenation: none.

## 2. In-shape editing: caret, mouse selection, IME: **GO** (macOS real-IME check untested)

`EntityInputHandler` (UTF-16 ranges, marked text, `bounds_for_range` for placing the candidate window) maps cleanly onto the custom layout. See `evidence/mac-selftest.txt`, all PASS:

- Mouse drag selection across a justified line break (real `MouseDown`/`MouseMove`/`MouseUp` events).
- Cmd+B on the selection.
- IME marked text with underline (`mac-5-ime-marked.png`), then commit.
- A dead-key sequence (´ + e → é).
- Up/down arrows use the stop table.

The self-test calls the input handler the way `NSTextInputClient` does but doesn't go through the OS. A two-minute real-IME check on the Mac is on the checklist and is still untested.

**Windows**: `evidence/win-selftest.txt` is all PASS, and the real OS paths were checked by hand:

- Caret lands where clicked, drag selects across lines, Shift+click extends, Up/Down keep the column through the CJK line.
- Microsoft Japanese IME through TSF: `nihongo` composes underlined inline, the candidate list opens at the composition (not the window corner), Space cycles, Enter commits (`win-ime.png`). The list opens a few pixels high and covers the bottom of the composition line; usable, and likely our `bounds_for_range` rect rather than `gpui_windows`.
- Dead keys on US-International: é, à, ü.
- The click that activates an inactive window isn't passed to the app, so it places no caret.

## 3. Rounded shapes and straight connectors, panned and zoomed, on 10,000 shapes: **GO** (with a text cutoff)

Shapes are `paint_quad` with corner radii. All visible connectors go into one `PathBuilder::stroke` path per frame. The generator is `generate()` in `main.rs`: vertical trunks of 40 shapes with 0–2-shape side branches on every other row (about 165 trunks), deterministic prose, and ~1 in 6 words bold or italic.

| scenario (warm) | draw p95, cutoff 4 px | draw p95, cutoff 7 px | target |
|---|---|---|---|
| pan at 100 % | 3.7 ms | 3.8 ms | ≤ 8 |
| pan at 40 % | 7.6 ms | 1.6 ms | ≤ 8 |
| pan at 10 % (no text) | 3.0 ms | 3.1 ms | ≤ 8 |
| pan at 2 % (all 10k shapes and links on screen) | 5.5 ms | 5.4 ms | ≤ 8 |
| zoom sweep 100 → 2 → 100 % | **8.7 ms** (cold 9.1) | 5.8 ms (cold 7.0) | ≤ 8 |

- Cost scales with glyphs painted. Around 30–40 % zoom tens of thousands of glyphs are visible, and the text is 4–6 px tall, too small to read.
- Not drawing text below about 7 px on screen brings every scenario under target (substitute bars or a fade at that size; Phase 1 picks the look). If text must stay drawn smaller, the next step is caching each shape's glyph runs as a picture. GPUI has no retained layer API for that, which is a mild minus.
- Cold `draw_max` spikes (20–30 ms) are first-time glyph rasterization at new sizes. Font size follows zoom, quantized to 0.5 px.
- Laying out all 10k shapes at load takes **570–590 ms** on the main thread, through the window's text system. The real app needs shape heights at load, so it should either store heights in the file or lay out incrementally.

**Windows (9600X + RTX 5060 Ti, rows `win-9600x-5060ti`)**: every scenario is under target, even at the default 4 px cutoff.

| scenario (warm) | draw p95, cutoff 4 px | draw p95, cutoff 7 px | Mac, cutoff 4 px |
|---|---|---|---|
| pan at 100 % | 1.2 ms | 1.2 ms | 3.7 ms |
| pan at 40 % | 5.3 ms | 0.5 ms | 7.6 ms |
| pan at 10 % | 1.0 ms | 1.1 ms | 3.0 ms |
| pan at 2 % | 3.0 ms | 3.0 ms | 5.5 ms |
| zoom sweep 100 → 2 → 100 % | 6.3 ms (cold 6.2) | 2.4 ms (cold 2.7) | 8.7 ms |

- Present intervals stayed on vsync (p95 17.3–18.1 ms). Worst `draw_max` was 10.8 ms (cold zoom sweep).
- By hand: wheel and Shift+wheel pan, Ctrl+wheel zooms, `L` loads the 10k document. The owner found Ctrl+wheel zoom choppy: each notch jumps about 49 % in one frame because the spike applies it with no easing. Draw time isn't the cause, and the spec now requires eased wheel zoom in Phase 2.
- Precision touchpad pan and pinch are untested on Windows (none on this machine). The MacBook Air trackpad passed by hand on 2026-10-24: two-finger pan, momentum that stops, pinch around the fingers, Cmd+swipe zoom, smooth on the 10k document, tap-to-click and two-finger tap.

## 4. Idle CPU and memory after an animation: **GO**

The zoom badge fades over 1.2 s using `request_animation_frame` and then stops asking for frames.

- Over the following 5 s: **0 renders**, **0.76 % CPU** (`ps` cputime; the same in all runs).
- RSS: demo document 95 MB; 10k document fully laid out 224 MB; 259–272 MB after the sweeps (glyph atlas).
- Most of the 10k figure is the spike's fat per-glyph layout structs, not GPUI. It shrinks if layouts are kept only for visible shapes and heights are cached.
- **Windows**: **0 renders** and 0.9–1.2 % CPU over the 5 s after the fade. RSS 199–204 MB with the 10k document loaded, 185–201 MB after the sweeps.

## 5. Is the API reasonable for the planned interactions? **GO**

All of these were built in the spike except the native menus:

- **Hover midpoints**: mouse-move plus our own hit test, painted as quads. Trivial.
- **Context menu**: `deferred(anchored().position(p).child(div().occlude()…))`, about 40 lines (`mac-9-context-menu.png`). GPUI itself has no menu widget; Zed's `ui::ContextMenu` pulls in Zed's theme and settings crates. We'd write our own, which is small.
- **Directional picker**: Tab, then an arrow adds a linked shape (`mac-7-picker.png`, `mac-8-panel.png`). Gotcha: GPUI dispatches a bound action *before* capture-phase key listeners, so picker arrows have to go through the arrow actions (or a key context), not `capture_key_down`.
- **Side text panel**: a flex `div` beside the canvas. Trivial. Phase 7's code panel needs a multi-line text editor, and the `gpui` crate has none (Zed's editor isn't reusable). We'd build it on this same custom text layout.
- **Windows title bar**: `div().window_control_area(WindowControlArea::{Drag, Min, Max, Close})`. `gpui_windows` maps `Max` to `HTMAXBUTTON` (events.rs:971), which is what gives Windows 11 its native snap-layout flyout, and handles `WM_DPICHANGED`. The code is in `title_bar()`. Verified on Windows 11: drag, double-click maximize/restore, min/max/close, and the snap flyout on hover and on Win+Z (`win-snap.png`). Mixed DPI is untested (one monitor). The spike first painted shapes over the title bar because its canvas wasn't clipped; one `overflow_hidden()` fixed it.
- **Right-click menu, picker and panel on Windows**: all work. Ctrl+\ can't be typed on the Spanish layout (`\` is AltGr+º), so Phase 5 should pick shortcuts that exist on common layouts.
- **Spike bug found and fixed**: a pan released off the canvas kept panning, because the drag only ended on a mouse-up over the canvas. It now ends on the next move once the button is up. GPUI reports the pressed button on every move, so this was our code.
- **macOS**: a transparent title bar with traffic lights works (`TitlebarOptions`). Native menus (`cx.set_menus`) weren't exercised; Zed uses them daily.

## 6. Emoji fallback with the bundled font: **GO**

IBM Plex Sans (the stand-in bundled font) has no emoji, and 🌳🍂 fall back to Apple Color Emoji and draw in colour (`mac-1-demo.png`). On Windows they fall back to Segoe UI Emoji through DirectWrite and draw in colour at every zoom (`win-demo.png`, `win-emoji.png`).

## Keystroke-to-paint: vsync-bound, needs a target decision

Measured from dispatching the key to the end of the paint that shows it (present follows in the same turn). Over three runs of 200 keystrokes in the 10k document:

- **p50 6–11 ms**
- **p95 14–20 ms**
- max 21–22 ms

The draw itself is 4–5 ms (p95). The rest is waiting: on macOS GPUI draws only on the display-link tick (`gpui_macos/src/window.rs`, `step`), so input waits 0–16.7 ms for the next tick at 60 Hz. Any vsync-paced renderer pays this before photons, the Linebender stack included, so it isn't a GPUI deficit. But the "≤ 16 ms" target as written fails at p95 roughly half the time on a 60 Hz panel.

Suggested restatement: "draw ≤ 8 ms per keystroke and the next vsync is never missed". That one passes.

On Windows the same measure is p50 2.6–2.8 ms, p95 5.4–10.8 ms and max 18 ms, with draw p95 1.5–1.6 ms. It passes the target as written. Why Windows waits less for the frame wasn't investigated.

(GPUI's built-in input-latency histogram doesn't count synthetic text input, because it reaches the input handler after `dispatch_event` returns. Real keystrokes would be counted.)

## Notes for ADR 0001 (GPUI-free core)

- **Shape height depends on text layout, and layout needs GPUI's text system** (a `WindowTextSystem` that belongs to a window). Commands whose result depends on heights (insert-and-push, the "shape below b" lookups, link sides) can't run in a GPUI-free core unless the core takes a `measure(text, width) -> height` function from outside, or heights are stored in the model and refreshed by the UI. Settle this before Phase 2.
- The input handler speaks UTF-16 ranges. The core can stay UTF-8, with conversion at the GPUI edge (done in the spike).
- Nothing else got in the way: the document is a plain struct inside one GPUI entity, and all drawing comes from a snapshot built in `render`.

## What would justify switching to Linebender (winit + Vello + Parley)

None of these is true today. Switch if any turns out to be true:

1. **The Windows checklist fails at the platform layer** (IME candidate placement, snap layouts, mixed-DPI scaling) and can't be fixed with a small patch to a pinned `gpui_windows`. winit does these natively and Parley handles IME composition. IME placement and snap layouts passed on 2026-10-24. Mixed DPI is the one still untested.
2. **Small text has to stay visible** at the 25–40 % zoom that costs 7.6–9 ms, and a per-shape cache can't be built on GPUI's immediate-mode scene. Vello's retained scene fragments are the main draw there.
3. **Owning text layout becomes the dominant cost.** Parley ships UAX #14 line breaking, justified alignment, bidi and selection geometry. With GPUI we keep ~200 lines of layout plus CJK breaking and a multi-line editor for Phase 7.
4. **Pin upgrades become painful.** This rev already split `gpui` into several platform crates, which shows how fast the API surface moves.
