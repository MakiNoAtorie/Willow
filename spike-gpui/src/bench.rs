//! Scripted measurements (`--bench`). Frame and input timings come from GPUI's
//! own profiler (`profiler` feature): `draw` is Window::draw (layout, prepaint,
//! paint), `dirty_to_present` runs from first invalidation through handing the
//! frame to the GPU. GPU execution time is not included in any of them.
//! Typing latency is our own: keystroke dispatched -> end of the canvas paint
//! of the frame showing it (GPUI's input-latency histogram skips synthetic
//! text input, which reaches the input handler after `dispatch_event`).

use std::{fs::OpenOptions, io::Write, path::Path, time::Duration, time::Instant};

use gpui::{AsyncWindowContext, FrameDurationSnapshot, Keystroke, WeakEntity, Window, point};

use crate::Spike;

pub const GPUI_REV: &str = "79fe6a645f68f8c9230578f154bf99852d9f716f";
const CSV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/results.csv");

pub struct Bench {
    machine: String,
    pub tag: String, // appended to scenario names, e.g. @cutoff7
    rows: Vec<(String, String, f64, &'static str)>,
    /// Remaining scripted frames and per-frame camera motion: (pan x px, zoom factor).
    frames: Vec<(f32, f32)>,
    key_sent: Option<Instant>,
    key_to_paint: Vec<f64>,
}

impl Bench {
    pub fn new(machine: String) -> Self {
        Bench {
            machine,
            tag: String::new(),
            rows: Vec::new(),
            frames: Vec::new(),
            key_sent: None,
            key_to_paint: Vec::new(),
        }
    }

    /// End of the canvas paint; present follows in the same `Window::draw` turn.
    pub fn painted(&mut self) {
        if let Some(t) = self.key_sent.take() {
            self.key_to_paint.push(t.elapsed().as_secs_f64() * 1e3);
        }
    }

    fn row(&mut self, scenario: &str, metric: &str, value: f64, unit: &'static str) {
        let scenario = format!("{scenario}{}", self.tag);
        println!("{scenario:<22} {metric:<26} {value:>10.3} {unit}");
        self.rows.push((scenario, metric.into(), value, unit));
    }

    fn write_csv(&self) {
        let new = !Path::new(CSV).exists();
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(CSV)
            .expect("results.csv");
        if new {
            writeln!(f, "machine,gpui_rev,scenario,metric,value,unit").unwrap();
        }
        for (s, m, v, u) in &self.rows {
            writeln!(f, "{},{},{s},{m},{v:.3},{u}", self.machine, &GPUI_REV[..12]).unwrap();
        }
        println!("appended {} rows to {CSV}", self.rows.len());
    }
}

/// Called from `render`: applies one scripted camera step and keeps frames
/// coming until this scenario's script runs out.
pub fn step(s: &mut Spike, window: &mut Window) {
    let Some(b) = &mut s.bench else { return };
    let Some((dx, factor)) = b.frames.pop() else {
        return;
    };
    s.offset.x += dx / s.zoom;
    if factor != 1.0 {
        let c = window.viewport_size();
        s.zoom_at(factor, point(c.width / 2.0, c.height / 2.0));
    }
    window.request_animation_frame();
}

fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

fn frame_rows(
    b: &mut Bench,
    scenario: &str,
    before: &FrameDurationSnapshot,
    after: &FrameDurationSnapshot,
) {
    let mut draw = after.draw_duration_histogram.clone();
    let _ = draw.subtract(&before.draw_duration_histogram);
    let mut total = after.dirty_to_present_histogram.clone();
    let _ = total.subtract(&before.dirty_to_present_histogram);
    let mut interval = after.present_interval_histogram.clone();
    let _ = interval.subtract(&before.present_interval_histogram);
    b.row(scenario, "frames", draw.len() as f64, "count");
    b.row(scenario, "draw_p50", ms(draw.value_at_quantile(0.50)), "ms");
    b.row(scenario, "draw_p95", ms(draw.value_at_quantile(0.95)), "ms");
    b.row(scenario, "draw_max", ms(draw.max()), "ms");
    b.row(
        scenario,
        "dirty_to_present_p95",
        ms(total.value_at_quantile(0.95)),
        "ms",
    );
    b.row(
        scenario,
        "present_interval_p95",
        ms(interval.value_at_quantile(0.95)),
        "ms",
    );
}

async fn wait(cx: &mut AsyncWindowContext, d: Duration) {
    cx.background_executor().timer(d).await;
}

/// Waits until the scripted frames for the current scenario are used up.
async fn drain(this: &WeakEntity<Spike>, cx: &mut AsyncWindowContext) {
    loop {
        wait(cx, Duration::from_millis(50)).await;
        let left = this
            .update(cx, |s, _| s.bench.as_ref().map_or(0, |b| b.frames.len()))
            .unwrap_or(0);
        if left == 0 {
            wait(cx, Duration::from_millis(100)).await;
            return;
        }
    }
}

fn process_stats() -> (f64, f64) {
    // (cpu seconds, rss MB) of this process, via the OS's own tools.
    let pid = std::process::id().to_string();
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "$p=Get-Process -Id {pid}; \"$($p.TotalProcessorTime.TotalSeconds) $($p.WorkingSet64)\""
            ))
            .output()
            .expect("powershell");
        let s = String::from_utf8_lossy(&out.stdout);
        let mut it = s
            .split_whitespace()
            .map(|v| v.replace(',', ".").parse::<f64>().unwrap_or(0.0));
        let cpu = it.next().unwrap_or(0.0);
        (cpu, it.next().unwrap_or(0.0) / 1_048_576.0)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "time=,rss=", "-p", &pid])
            .output()
            .expect("ps");
        let s = String::from_utf8_lossy(&out.stdout);
        let mut it = s.split_whitespace();
        // time is [[dd-]hh:]mm:ss.cc
        let cpu = it.next().unwrap_or("0").split(':').fold(0.0, |acc, part| {
            acc * 60.0 + part.parse::<f64>().unwrap_or(0.0)
        });
        let rss_kb: f64 = it.next().unwrap_or("0").parse().unwrap_or(0.0);
        (cpu, rss_kb / 1024.0)
    }
}

pub async fn run(this: WeakEntity<Spike>, cx: &mut AsyncWindowContext) {
    wait(cx, Duration::from_millis(1500)).await;
    // The real app needs every shape's height, so lay out all 10k texts up front.
    this.update_in(cx, |s, window, _| {
        let ts = window.text_system().clone();
        let t = Instant::now();
        for sh in &mut s.shapes {
            sh.layout = Some(std::rc::Rc::new(crate::text::layout(
                &sh.text,
                &sh.styles,
                sh.w - 2.0 * crate::PAD,
                &ts,
            )));
        }
        let ms = t.elapsed().as_secs_f64() * 1e3;
        s.bench
            .as_mut()
            .unwrap()
            .row("load_10k", "layout_all", ms, "ms");
    })
    .ok();
    let (_, rss) = process_stats();
    this.update(cx, |s, _| {
        s.bench.as_mut().unwrap().row("load_10k", "rss", rss, "MB")
    })
    .ok();

    // Pan scenarios: (name, zoom, frames, px per frame). Each pans right then
    // back over the same ground, so "cold" includes first-time text layout of
    // newly revealed shapes and "warm" is cached.
    let scenarios: [(&str, f32, usize, f32); 4] = [
        ("pan_zoom100", 1.0, 240, 12.0),
        ("pan_zoom40", 0.4, 240, 12.0),
        ("pan_zoom10", 0.1, 240, 12.0),
        ("pan_zoom2", 0.02, 240, 12.0),
    ];
    for (name, zoom, n, dx) in scenarios {
        for (pass, dir) in [("cold", 1.0), ("warm", -1.0)] {
            let before = this
                .update_in(cx, |s, window, cx| {
                    if pass == "cold" {
                        s.zoom = zoom;
                        s.offset = point(-40.0, -40.0);
                    }
                    s.bench.as_mut().unwrap().frames = vec![(dx * dir, 1.0); n];
                    cx.notify();
                    window.frame_duration_snapshot()
                })
                .unwrap();
            drain(&this, cx).await;
            this.update_in(cx, |s, window, _| {
                let after = window.frame_duration_snapshot();
                frame_rows(
                    s.bench.as_mut().unwrap(),
                    &format!("{name}_{pass}"),
                    &before,
                    &after,
                );
            })
            .ok();
        }
    }

    // Zoom sweep 100% -> 2% -> 100% around the window centre, twice: the cold
    // pass rasterizes glyphs at every new size, the warm pass reuses the atlas.
    for pass in ["cold", "warm"] {
        let before = this
            .update_in(cx, |s, window, cx| {
                s.zoom = 1.0;
                s.offset = point(20_000.0, 2_000.0);
                let k = (0.02f32).powf(1.0 / 150.0);
                // frames are popped from the back: zoom out first, then back in
                let mut f = vec![(0.0, 1.0 / k); 150];
                f.extend(vec![(0.0, k); 150]);
                s.bench.as_mut().unwrap().frames = f;
                cx.notify();
                window.frame_duration_snapshot()
            })
            .unwrap();
        drain(&this, cx).await;
        this.update_in(cx, |s, window, _| {
            let after = window.frame_duration_snapshot();
            frame_rows(
                s.bench.as_mut().unwrap(),
                &format!("zoom_sweep_{pass}"),
                &before,
                &after,
            );
        })
        .ok();
    }

    // Typing into a shape in the 10k document: 200 keystrokes, 40 ms apart.
    let keys: Vec<&'static str> = "the quiet harbour remembers everything "
        .chars()
        .map(|c| match c {
            ' ' => "space",
            c => Box::leak(c.to_string().into_boxed_str()) as &'static str,
        })
        .collect();
    let before_frames = this
        .update_in(cx, |s, window, cx| {
            s.zoom = 1.0;
            s.offset = point(-40.0, -40.0);
            let end = s.shapes[0].text.len();
            s.start_editing(0, end, window, cx);
            window.frame_duration_snapshot()
        })
        .unwrap();
    wait(cx, Duration::from_millis(300)).await;
    let h0 = this.update(cx, |s, _| s.shapes[0].h()).unwrap();
    for i in 0..200 {
        let key = keys[i % keys.len()];
        this.update(cx, |s, _| {
            s.bench.as_mut().unwrap().key_sent = Some(Instant::now())
        })
        .ok();
        // Dispatch outside any entity update, as the platform would.
        cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx))
            .ok();
        wait(cx, Duration::from_millis(40)).await;
    }
    wait(cx, Duration::from_millis(200)).await;
    this.update_in(cx, |s, window, _| {
        let h1 = s.shapes[0].h();
        let fr = window.frame_duration_snapshot();
        let b = s.bench.as_mut().unwrap();
        let mut lat = std::mem::take(&mut b.key_to_paint);
        lat.sort_by(f64::total_cmp);
        let q = |p: f64| lat[((lat.len() - 1) as f64 * p).round() as usize];
        b.row("typing_10k", "keystrokes", lat.len() as f64, "count");
        b.row("typing_10k", "key_to_paint_p50", q(0.5), "ms");
        b.row("typing_10k", "key_to_paint_p95", q(0.95), "ms");
        b.row("typing_10k", "key_to_paint_max", q(1.0), "ms");
        b.row("typing_10k", "shape_growth", (h1 - h0) as f64, "units");
        frame_rows(b, "typing_10k", &before_frames, &fr);
    })
    .ok();

    // Idle: stop editing, trigger the zoom-badge fade (1.2 s), then watch.
    this.update_in(cx, |s, window, cx| {
        s.stop_editing(cx);
        let c = window.viewport_size();
        s.zoom_at(1.0, point(c.width / 2.0, c.height / 2.0));
        cx.notify();
    })
    .ok();
    wait(cx, Duration::from_millis(1200 + 1000)).await;
    let (c0, _) = process_stats();
    let r0 = this.update(cx, |s, _| s.renders).unwrap();
    let t0 = Instant::now();
    wait(cx, Duration::from_secs(5)).await;
    let (c1, rss) = process_stats();
    let secs = t0.elapsed().as_secs_f64();
    this.update_in(cx, |s, _, cx| {
        let renders = s.renders - r0;
        let b = s.bench.as_mut().unwrap();
        b.row(
            "idle_after_anim",
            "cpu",
            (c1 - c0) / secs * 100.0,
            "percent",
        );
        b.row("idle_after_anim", "renders", renders as f64, "count");
        b.row("idle_after_anim", "rss", rss, "MB");
        b.write_csv();
        cx.quit();
    })
    .ok();
}

// ---- --selftest: scripted editing checks with screenshots (macOS only for the shots) ----

fn shot(window: &Window, name: &str) {
    let b = window.bounds();
    let (x, y, w, h) = (
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    );
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/evidence/");
    let _ = std::process::Command::new("screencapture")
        .args([
            "-x",
            &format!("-R{x},{y},{w},{h}"),
            &format!("{path}mac-{name}.png"),
        ])
        .status();
}

fn check(name: &str, ok: bool) {
    println!("{} {name}", if ok { "PASS" } else { "FAIL" });
}

/// Window position of the caret stop for byte `i` of shape `idx`, mid-line.
fn pos_of(s: &mut Spike, idx: usize, i: usize, window: &mut Window) -> gpui::Point<gpui::Pixels> {
    let sh = &s.shapes[idx];
    let layout = sh.layout.clone().expect("laid out");
    let st = layout.stop_for(i);
    let p = point(
        sh.x + crate::PAD + st.x,
        sh.y + crate::PAD + (st.line as f32 + 0.5) * crate::text::LINE_HEIGHT,
    );
    let _ = window;
    s.canvas_origin + s.to_screen(p)
}

pub async fn selftest(this: WeakEntity<Spike>, cx: &mut AsyncWindowContext) {
    use gpui::{
        EntityInputHandler, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
        PlatformInput,
    };
    let step = Duration::from_millis(400);
    wait(cx, Duration::from_millis(1200)).await;
    this.update_in(cx, |_, w, _| shot(w, "1-demo")).ok();

    // Mouse drag selection from "harbour" (line 2) to the end of "letter" (line 3).
    let (from, to, text) = this
        .update(cx, |s, _| {
            let t = &s.shapes[0].text;
            let a = t.find("harbour").unwrap();
            let b = t.find("letter").unwrap() + "letter".len();
            (a, b, t[a..b].to_string())
        })
        .unwrap();
    let (pa, pb) = this
        .update_in(cx, |s, w, _| (pos_of(s, 0, from, w), pos_of(s, 0, to, w)))
        .unwrap();
    cx.update(|w, cx| {
        let m = Modifiers::default();
        w.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: pa,
                modifiers: m,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
    })
    .ok();
    wait(cx, step).await;
    // second press now that the shape is in editing mode and laid out
    cx.update(|w, cx| {
        let m = Modifiers::default();
        w.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: pa,
                modifiers: m,
                click_count: 1,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: pa,
                modifiers: m,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseMove(MouseMoveEvent {
                position: pb,
                pressed_button: Some(MouseButton::Left),
                modifiers: m,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: pb,
                modifiers: m,
                click_count: 1,
            }),
            cx,
        );
    })
    .ok();
    wait(cx, step).await;
    let (sel, two_lines) = this
        .update(cx, |s, _| {
            let l = s.shapes[0].layout.clone().expect("laid out");
            (s.sel.clone(), l.stop_for(from).line != l.stop_for(to).line)
        })
        .unwrap();
    check(
        &format!("mouse selection across a justified line break selects {text:?} (got {sel:?})"),
        sel == (from..to) && two_lines,
    );
    this.update_in(cx, |_, w, _| shot(w, "2-selection")).ok();

    // Bold the selection.
    cx.update(|w, cx| w.dispatch_action(Box::new(crate::Bold), cx))
        .ok();
    wait(cx, step).await;
    let bold = this
        .update(cx, |s, _| {
            s.shapes[0].styles[from..to].iter().all(|st| st.bold)
        })
        .unwrap();
    check("Cmd+B bolds the selection", bold);
    this.update_in(cx, |_, w, _| shot(w, "3-bold")).ok();

    // Type at the end; the shape grows.
    let h0 = this
        .update_in(cx, |s, _, cx| {
            let n = s.shapes[0].text.len();
            s.move_to(n, cx);
            s.shapes[0].h()
        })
        .unwrap();
    for key in "space g r o w i n g space t e x t space k e e p s space w r a p p i n g space j u s t i f i e d".split(' ') {
        cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx)).ok();
        wait(cx, Duration::from_millis(15)).await;
    }
    wait(cx, step).await;
    let h1 = this.update(cx, |s, _| s.shapes[0].h()).unwrap();
    check(&format!("typing grows the shape ({h0} -> {h1})"), h1 > h0);
    this.update_in(cx, |_, w, _| shot(w, "4-grown")).ok();

    // IME composition the way NSTextInputClient drives it: marked text, then commit.
    let n = this
        .update_in(cx, |s, w, cx| {
            let n = s.shapes[0].text.len();
            s.replace_and_mark_text_in_range(None, " にほんご", Some(5..5), w, cx);
            n
        })
        .unwrap();
    wait(cx, step).await;
    let marked = this.update(cx, |s, _| s.marked.clone()).unwrap();
    check(
        &format!("IME marked range is set ({marked:?})"),
        marked == Some(n..n + " にほんご".len()),
    );
    this.update_in(cx, |_, w, _| shot(w, "5-ime-marked")).ok();
    this.update_in(cx, |s, w, cx| {
        s.replace_text_in_range(None, " 日本語", w, cx)
    })
    .ok();
    // Dead key: ´ is marked, then e commits é.
    this.update_in(cx, |s, w, cx| {
        s.replace_and_mark_text_in_range(None, "´", Some(1..1), w, cx)
    })
    .ok();
    this.update_in(cx, |s, w, cx| s.replace_text_in_range(None, "é", w, cx))
        .ok();
    wait(cx, step).await;
    let ok = this
        .update(cx, |s, _| {
            s.shapes[0].text.ends_with(" 日本語é") && s.marked.is_none()
        })
        .unwrap();
    check("IME commit and dead-key sequence produce \" 日本語é\"", ok);
    this.update_in(cx, |_, w, _| shot(w, "6-ime-committed"))
        .ok();

    // Directional picker on the top-right shape: Tab, then Right adds a linked shape.
    let n0 = this
        .update_in(cx, |s, w, cx| {
            s.start_editing(2, 0, w, cx);
            s.shapes.len()
        })
        .unwrap();
    cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse("tab").unwrap(), cx))
        .ok();
    wait(cx, step).await;
    this.update_in(cx, |_, w, _| shot(w, "7-picker")).ok();
    cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse("right").unwrap(), cx))
        .ok();
    wait(cx, step).await;
    for key in "n e w space b r a n c h".split(' ') {
        cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx))
            .ok();
    }
    wait(cx, step).await;
    let (n1, linked) = this
        .update(cx, |s, _| (s.shapes.len(), s.links.last().copied()))
        .unwrap();
    check(
        "Tab then Right adds a linked shape",
        n1 == n0 + 1 && linked == Some((2, n0)),
    );
    cx.update(|w, cx| w.dispatch_action(Box::new(crate::TogglePanel), cx))
        .ok();
    wait(cx, step).await;
    this.update_in(cx, |_, w, _| shot(w, "8-panel")).ok();

    // Context menu on the bottom-right shape.
    let p = this
        .update_in(cx, |s, w, cx| {
            s.stop_editing(cx);
            s.panel = false;
            let sh = &s.shapes[3];
            let _ = w;
            s.canvas_origin + s.to_screen(point(sh.x + 40.0, sh.y + 20.0))
        })
        .unwrap();
    cx.update(|w, cx| {
        let m = Modifiers::default();
        w.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Right,
                position: p,
                modifiers: m,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Right,
                position: p,
                modifiers: m,
                click_count: 1,
            }),
            cx,
        );
    })
    .ok();
    wait(cx, step).await;
    let open = this.update(cx, |s, _| s.menu.is_some()).unwrap();
    check("right-click opens the context menu", open);
    this.update_in(cx, |_, w, _| shot(w, "9-context-menu")).ok();

    // Plain letters reach the text while editing, even ones bound to canvas actions.
    this.update_in(cx, |s, w, cx| {
        s.menu = None;
        let n = s.shapes[0].text.len();
        s.start_editing(0, n, w, cx);
    })
    .ok();
    cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse("l").unwrap(), cx))
        .ok();
    wait(cx, step).await;
    let ok = this
        .update(cx, |s, _| s.shapes[0].text.ends_with('l'))
        .unwrap();
    check("typing l while editing inserts it", ok);

    // A trailing space at the end of a paragraph moves the caret.
    let caret = |s: &mut Spike| {
        let st = s.shapes[0]
            .layout
            .clone()
            .expect("laid out")
            .stop_for(s.cursor());
        (st.line, st.x)
    };
    let before = this.update(cx, |s, _| caret(s)).unwrap();
    cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse("space").unwrap(), cx))
        .ok();
    wait(cx, step).await;
    let after = this.update(cx, |s, _| caret(s)).unwrap();
    check(
        &format!("a trailing space moves the caret ({before:?} -> {after:?})"),
        after > before,
    );

    // Caret stops sit only on grapheme boundaries (e + combining acute is one).
    this.update_in(cx, |s, w, cx| {
        s.replace_text_in_range(None, "e\u{301}x", w, cx)
    })
    .ok();
    wait(cx, step).await;
    let ok = this
        .update(cx, |s, _| {
            use unicode_segmentation::UnicodeSegmentation;
            let sh = &s.shapes[0];
            let bounds: Vec<usize> = sh
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([sh.text.len()])
                .collect();
            sh.layout
                .as_ref()
                .expect("laid out")
                .stops
                .iter()
                .all(|st| bounds.contains(&st.index))
        })
        .unwrap();
    check("caret stops only on grapheme boundaries", ok);

    // Clicking another shape mid-composition drops the composition.
    this.update_in(cx, |s, w, cx| {
        s.replace_and_mark_text_in_range(None, "か", Some(1..1), w, cx)
    })
    .ok();
    let p = this
        .update(cx, |s, _| {
            let sh = &s.shapes[3];
            s.canvas_origin + s.to_screen(point(sh.x + 40.0, sh.y + 20.0))
        })
        .unwrap();
    cx.update(|w, cx| {
        let m = Modifiers::default();
        w.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: p,
                modifiers: m,
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        w.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: p,
                modifiers: m,
                click_count: 1,
            }),
            cx,
        );
    })
    .ok();
    wait(cx, step).await;
    let ok = this
        .update(cx, |s, _| s.editing == Some(3) && s.marked.is_none())
        .unwrap();
    check(
        "switching shapes mid-composition clears the marked range",
        ok,
    );
    this.update(cx, |s, cx| s.stop_editing(cx)).ok();
    cx.update(|w, cx| w.dispatch_keystroke(Keystroke::parse("l").unwrap(), cx))
        .ok();
    wait(cx, step).await;
    let n = this.update(cx, |s, _| s.shapes.len()).unwrap();
    check(
        &format!("L loads the 10k document when not editing ({n} shapes)"),
        n == 10_000,
    );

    // The generated 10k document at three zoom levels.
    for (zoom, name) in [
        (0.6, "10-large-zoom60"),
        (0.25, "11-large-zoom25"),
        (0.03, "12-large-zoom3"),
    ] {
        this.update(cx, |s, cx| {
            s.menu = None;
            (s.shapes, s.links) = crate::generate(10_000);
            s.zoom = zoom;
            s.offset = point(-40.0, -40.0);
            cx.notify();
        })
        .ok();
        wait(cx, Duration::from_millis(1600)).await; // let the zoom badge fade
        this.update_in(cx, |_, w, _| shot(w, name)).ok();
    }
    // Loaded and only seen far out, below the text cutoff: shapes still have
    // their real heights, not the minimum.
    let ok = this
        .update(cx, |s, _| s.shapes.iter().all(|sh| sh.layout.is_some()))
        .unwrap();
    check(
        "a document first seen below the text cutoff has every shape's height",
        ok,
    );
    this.update(cx, |_, cx| cx.quit()).ok();
}
