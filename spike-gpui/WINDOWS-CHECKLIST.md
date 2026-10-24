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

## 2. Scripted editing checks

```powershell
cargo run --release -p spike-gpui -- --selftest > spike-gpui\evidence\win-selftest.txt
```

All lines should read PASS. Screenshots are macOS-only; take the ones below with Win+Shift+S and save them as `spike-gpui/evidence/win-<name>.png`.

## 3. By hand (`cargo run --release -p spike-gpui`)

Tick each item, and add a note where it fails.

- [ ] **Text**: the top-left shape is justified, and its bold/italic words look like `evidence/mac-1-demo.png`. Screenshot: `win-demo`.
- [ ] **Emoji**: 🌳🍂 in the lower-left shape draw in colour (Segoe UI Emoji fallback), not as boxes. Screenshot: `win-emoji`.
- [ ] **Caret and selection**: click into a shape and the caret lands where you clicked. Drag selects across lines. Shift+click extends. Arrow keys move, including up and down.
- [ ] **Live growth**: typing at the end of a shape keeps it justified, and the shape grows as you type.
- [ ] **IME, Japanese** (Microsoft IME): type `nihongo` and it shows underlined. The candidate window sits next to the caret, not at the window corner. Space cycles candidates, Enter commits. Screenshot: `win-ime`.
- [ ] **Dead keys** (US-International layout): `'` then `e` gives é, `` ` `` then `a` gives à, and `"` then `u` gives ü.
- [ ] **Title bar**: dragging the title moves the window, and double-clicking it maximizes and restores. Min, max and close work.
- [ ] **Snap layouts**: hovering the maximize button shows the Windows 11 snap flyout, and Win+Z does too. Screenshot: `win-snap`.
- [ ] **Mixed DPI**: with a second monitor at a different scale (e.g. 100 % and 150 %), drag the window across. Text stays crisp, the title bar keeps its height, and nothing is blurry or mis-sized after the move. Also try maximizing on each monitor.
- [ ] **Feel**: wheel pans, Ctrl+wheel zooms, and a precision touchpad pans and pinch-zooms. Press `L` (not while editing) to load the 10k document and check it stays smooth.
- [ ] **Menu, picker, panel**: right-click a shape opens the menu, and a swatch recolours the shape. Tab then an arrow adds a linked shape. Ctrl+\ toggles the side panel.

## 4. Two minutes on the Mac too

The self-test drives the input handler directly, so it can't exercise the real macOS input path (`NSTextInputClient`).

- [ ] Japanese (Kana or Romaji) input: `nihongo` shows underlined, the candidate window sits at the caret, and Enter commits.
- [ ] Option+e then e gives é.

Commit `results.csv`, the evidence files and this ticked list on the branch.
