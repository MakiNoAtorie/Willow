# Windows checks for the owner (Ryzen 5 9600X, RTX 5060 Ti)

Needs Rust and Visual Studio Build Tools with a Windows SDK (GPUI compiles its HLSL shaders with `fxc` at build time). Run everything from the repo root on branch `prototype/gpui-viability`.

## 1. Numbers (appended to `spike-gpui/results.csv`)

Each run takes about 45 s and quits by itself. Don't touch the mouse while it runs.

```powershell
cargo run --release -p spike-gpui -- --bench --machine win-9600x-5060ti
cargo run --release -p spike-gpui -- --bench --machine win-9600x-5060ti --text-cutoff 7
```

Optional weak-laptop stand-in: Settings → System → Display → Graphics → add `target\release\spike-gpui.exe` → Power saving (or plug the monitor into the motherboard), then:

```powershell
cargo run --release -p spike-gpui -- --bench --machine win-9600x-igpu
```

Done 2026-10-24: both `win-9600x-5060ti` runs are in `results.csv`. The iGPU run was not done.

## 2. Scripted editing checks

```powershell
cargo run --release -p spike-gpui -- --selftest > spike-gpui\evidence\win-selftest.txt
```

All lines should read PASS. Screenshots are macOS-only; take the ones below with Win+Shift+S and save them as `spike-gpui/evidence/win-<name>.png`.

## 3. By hand (`cargo run --release -p spike-gpui`)

Tick each item, and add a note where it fails.

Run on 2026-10-24 on one 4K monitor, driven by the agent through computer use, with the owner trying pan and zoom by hand.

- [x] **Text**: the top-left shape is justified, and its bold/italic words look like `evidence/mac-1-demo.png`. Screenshot: `win-demo`.
- [x] **Emoji**: 🌳🍂 in the lower-left shape draw in colour (Segoe UI Emoji fallback), not as boxes. Screenshot: `win-emoji`.
- [x] **Caret and selection**: click into a shape and the caret lands where you clicked. Drag selects across lines. Shift+click extends. Arrow keys move, including up and down.
  - Note: Up/Down keep the column through the CJK line.
  - Note: the click that brings an inactive window to the front is not passed to the app, so it places no caret. The second click works.
- [x] **Live growth**: typing at the end of a shape keeps it justified, and the shape grows as you type.
- [x] **IME, Japanese** (Microsoft IME): type `nihongo` and it shows underlined. The candidate window sits next to the caret, not at the window corner. Space cycles candidates, Enter commits. Screenshot: `win-ime`.
  - Note: the candidate list opens at the composition start but a few pixels high, so it covers the bottom of the composition line instead of sitting just below it. Usable; cause not found (likely the rect from `bounds_for_range`).
  - Note: if the canvas pans during a composition, the list stays where it opened.
- [x] **Dead keys** (US-International layout): `'` then `e` gives é, `` ` `` then `a` gives à, and `"` then `u` gives ü.
- [x] **Title bar**: dragging the title moves the window, and double-clicking it maximizes and restores. Min, max and close work.
  - Fixed: shapes painted over the title bar because the canvas wasn't clipped (`overflow_hidden` added). Spike layout, not GPUI.
- [x] **Snap layouts**: hovering the maximize button shows the Windows 11 snap flyout, and Win+Z does too. Screenshot: `win-snap`.
- [ ] **Mixed DPI**: with a second monitor at a different scale (e.g. 100 % and 150 %), drag the window across. Text stays crisp, the title bar keeps its height, and nothing is blurry or mis-sized after the move. Also try maximizing on each monitor.
  - **UNTESTED**: only one monitor is connected.
- [ ] **Feel**: wheel pans, Ctrl+wheel zooms, and a precision touchpad pans and pinch-zooms. Press `L` (not while editing) to load the 10k document and check it stays smooth.
  - Pass: wheel and Shift+wheel pan, Ctrl+wheel zooms, `L` loads the 10k document and pans and zooms through it.
  - **UNTESTED**: precision touchpad (none on this machine).
  - Owner: Ctrl+wheel zoom feels choppy. Each notch jumps about 49 % in one frame with no easing (`on_scroll`); draw time is fine. The spec now requires eased wheel zoom (Phase 2).
  - Fixed: a pan released off the canvas (over the title bar or outside the window) kept panning. The drag now ends on the next move once the left button is up. Spike code, not GPUI.
- [x] **Menu, picker, panel**: right-click a shape opens the menu, and a swatch recolours the shape. Tab then an arrow adds a linked shape. Ctrl+\ toggles the side panel.
  - Note: Ctrl+\ can't be typed on the Spanish layout (`\` is AltGr+º). It works on the US layout. Pick layout-independent shortcuts in Phase 5.

## 4. Two minutes on the Mac too

The self-test drives the input handler directly, so it can't exercise the real macOS input path (`NSTextInputClient`).

**UNTESTED**: skipped by the owner for now.

- [ ] Japanese (Kana or Romaji) input: `nihongo` shows underlined, the candidate window sits at the caret, and Enter commits.
- [ ] Option+e then e gives é.

Commit `results.csv`, the evidence files and this ticked list on the branch.
