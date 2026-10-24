---
status: accepted
---

# GPUI is the UI stack, with our own text layout

Phase 0 showed GPUI (pinned to `zed-industries/zed` @ `79fe6a6`) can carry Willow on the MacBook Air M4. The primary source is `GO-NO-GO.md`, `spike-gpui/results.csv` and `spike-gpui/evidence/` on the never-merged branch `prototype/gpui-viability`. We keep GPUI over the Linebender stack (winit + Vello + Parley) because these already work in a ~1,000-line spike:

- canvas drawing and pan/zoom on 10,000 shapes (draw p95 3.7–5.5 ms while panning);
- editing with IME, caret and mouse selection;
- idle with no redraws after an animation;
- the Windows title-bar hooks for snap layouts and DPI changes.

The price is that we own text layout: GPUI has no justified alignment, so shapes use custom line breaking and glyph placement on top of `layout_line`/`paint_glyph`.

The owner's Windows checks (`spike-gpui/WINDOWS-CHECKLIST.md`) passed on 2026-10-24: IME, dead keys, snap layouts and emoji. Mixed DPI is still untested (no second monitor). If it, or any later Windows check, fails at the platform layer and a small patch to the pinned `gpui_windows` can't fix it, we switch stacks.

Windows (9600X + RTX 5060 Ti): draw p95 1.0–6.3 ms across pan and zoom sweep, idle renders 0, keystroke-to-paint p95 5.4 ms; rows `win-9600x-5060ti` in `spike-gpui/results.csv` on `prototype/gpui-viability`.

## Consequences

- **Text at small sizes**: text is not drawn below about 7 px on screen. With text down to 4 px, the zoom sweep misses the 8 ms p95 (8.7–9.1 ms); with the 7 px cutoff it passes (5.8–7.0 ms). Phase 1 picks what replaces text at that size.
- **Keystroke-to-paint target**: GPUI draws on the display-link tick, so keystroke-to-paint is the wait for the next vsync plus a 4–5 ms draw (p95 14–20 ms at 60 Hz). We restate the target as "draw ≤ 8 ms per keystroke and the next vsync is never missed" instead of "≤ 16 ms".
- **Shape height and the GPUI-free core (ADR 0001)**: shape height comes from text layout, and text layout needs GPUI's window text system. The core therefore takes a text-measuring function from the UI layer (or stores heights the UI refreshes) instead of measuring text itself.
- **Things to build ourselves**: CJK line breaking (UAX #14, kinsoku), a context-menu widget and Phase 7's multi-line editor. GPUI's crate ships none of them.
