//! `--bench --machine NAME`: scripts pans, zoom sweeps, typing and idle on the generated
//! 10,000-shape document in a real window and appends CSV rows to `bench/results.csv`.
//!
//! Frame timings come from GPUI's profiler: `draw` is `Window::draw` (layout, prepaint, paint;
//! CPU only), `dirty_to_present` runs from the first invalidation to handing the frame to the
//! GPU, `frames_late`/`missed_refreshes` count presented frames that overran their vsync.
//! Scripted input goes through `Session::handle`, one event per frame, as the platform's would.
//!
//! `file_10k` times, in milliseconds, saves of the generated document (serialize and write
//! beside): `save_unwarmed` from a new session that has measured nothing (a save the moment a
//! file opens), `save` the window's first (after the idle `measure_ahead`), `save_again` the
//! next; then `load` of that file (read, parse, repair, auto-place, a new session). The loaded
//! session replaces the window's, so `open_first_frame` and every later scenario run on it.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use gpui::{AsyncWindowContext, FrameDurationSnapshot, Keystroke, WeakEntity, Window};
use willow_core::{Button, Input, Modifiers, Point, Viewport};

use crate::Willow;

const CSV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bench/results.csv");
/// Screen pixels per scripted pan frame.
const PAN_PX: f32 = 12.0;
const PAN_FRAMES: usize = 240;
/// Frames per zoom sweep leg (100% to 10%, then back).
const SWEEP_FRAMES: usize = 150;
const KEYSTROKES: usize = 200;
const IDLE: Duration = Duration::from_secs(5);

pub(crate) struct Bench {
    machine: String,
    rows: Vec<(String, &'static str, f64, &'static str)>,
    /// Scripted inputs left, one applied per frame, popped from the back.
    frames: Vec<Input>,
    /// `render` calls so far.
    renders: u64,
}

impl Bench {
    pub(crate) fn new(machine: String) -> Self {
        Bench {
            machine,
            rows: Vec::new(),
            frames: Vec::new(),
            renders: 0,
        }
    }

    fn row(&mut self, scenario: &str, metric: &'static str, value: f64, unit: &'static str) {
        println!("{scenario:<20} {metric:<22} {value:>10.3} {unit}");
        self.rows.push((scenario.into(), metric, value, unit));
    }

    fn write_csv(&self) {
        let new = !Path::new(CSV).exists();
        let mut f = (OpenOptions::new().create(true).append(true))
            .open(CSV)
            .expect("bench/results.csv");
        if new {
            writeln!(f, "machine,git_rev,scenario,metric,value,unit").unwrap();
        }
        let rev = git_rev();
        for (s, m, v, u) in &self.rows {
            writeln!(f, "{},{rev},{s},{m},{v:.3},{u}", self.machine).unwrap();
        }
        println!("appended {} rows to {CSV}", self.rows.len());
    }
}

/// Called at the top of `render`: counts the render and applies one scripted input, asking
/// for another frame while the script lasts.
pub(crate) fn step(w: &mut Willow, window: &mut Window) {
    let Some(b) = &mut w.bench else { return };
    b.renders += 1;
    if let Some(input) = b.frames.pop() {
        w.session.handle(input, w.start.elapsed());
        window.request_animation_frame();
    }
}

/// Willow's short git revision, or "unknown" outside a checkout.
fn git_rev() -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();
    let rev = out.ok().filter(|o| o.status.success());
    let rev = rev.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    rev.filter(|r| !r.is_empty())
        .unwrap_or_else(|| "unknown".into())
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
    // Histograms since `before`: each is cumulative for the window.
    let mut draw = after.draw_duration_histogram.clone();
    let _ = draw.subtract(&before.draw_duration_histogram);
    let mut total = after.dirty_to_present_histogram.clone();
    let _ = total.subtract(&before.dirty_to_present_histogram);
    let mut interval = after.present_interval_histogram.clone();
    let _ = interval.subtract(&before.present_interval_histogram);
    let (mut late, mut missed) = (0, 0);
    for a in &after.by_refresh_interval {
        let e =
            (before.by_refresh_interval.iter()).find(|e| e.refresh_interval == a.refresh_interval);
        late += a.frames_late - e.map_or(0, |e| e.frames_late);
        missed += a.missed_refreshes - e.map_or(0, |e| e.missed_refreshes);
    }
    b.row(scenario, "frames", draw.len() as f64, "count");
    b.row(scenario, "draw_p50", ms(draw.value_at_quantile(0.5)), "ms");
    b.row(scenario, "draw_p95", ms(draw.value_at_quantile(0.95)), "ms");
    b.row(scenario, "draw_max", ms(draw.max()), "ms");
    let p95 = ms(total.value_at_quantile(0.95));
    b.row(scenario, "dirty_to_present_p95", p95, "ms");
    let p95 = ms(interval.value_at_quantile(0.95));
    b.row(scenario, "present_interval_p95", p95, "ms");
    b.row(scenario, "frames_late", late as f64, "count");
    b.row(scenario, "missed_refreshes", missed as f64, "count");
}

async fn wait(cx: &mut AsyncWindowContext, d: Duration) {
    cx.background_executor().timer(d).await;
}

/// Runs `script` (applied in order, one input per frame) and records its frame rows.
async fn scripted(
    this: &WeakEntity<Willow>,
    cx: &mut AsyncWindowContext,
    scenario: &str,
    start: Option<Viewport>,
    mut script: Vec<Input>,
) {
    script.reverse();
    let before = this
        .update_in(cx, |w, window, cx| {
            if let Some(vp) = start {
                w.session.set_viewport(vp);
            }
            w.bench.as_mut().unwrap().frames = script;
            cx.notify();
            window.frame_duration_snapshot()
        })
        .unwrap();
    loop {
        wait(cx, Duration::from_millis(50)).await;
        let left = this.update(cx, |w, _| w.bench.as_ref().unwrap().frames.len());
        if left.unwrap_or(0) == 0 {
            break;
        }
    }
    wait(cx, Duration::from_millis(100)).await;
    this.update_in(cx, |w, window, _| {
        let after = window.frame_duration_snapshot();
        frame_rows(w.bench.as_mut().unwrap(), scenario, &before, &after);
    })
    .ok();
}

/// (CPU seconds, RSS MB) of this process, from the OS's own tools.
fn process_stats() -> (f64, f64) {
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
        let mut it = (s.split_whitespace()).map(|v| v.replace(',', ".").parse().unwrap_or(0.0));
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
        // [[dd-]hh:]mm:ss.cc
        let cpu = (it.next().unwrap_or("0").split(':'))
            .fold(0.0, |acc, p| acc * 60.0 + p.parse::<f64>().unwrap_or(0.0));
        let rss_kb: f64 = it.next().unwrap_or("0").parse().unwrap_or(0.0);
        (cpu, rss_kb / 1024.0)
    }
}

/// Waits for animations to end, then 1 s more, then records CPU and renders over `IDLE`.
async fn idle(this: &WeakEntity<Willow>, cx: &mut AsyncWindowContext, scenario: &str) {
    while this
        .update(cx, |w, _| w.session.animating(w.start.elapsed()))
        .unwrap_or(false)
    {
        wait(cx, Duration::from_millis(50)).await;
    }
    wait(cx, Duration::from_secs(1)).await;
    let (c0, _) = process_stats();
    let r0 = this.update(cx, |w, _| w.bench.as_ref().unwrap().renders);
    let t0 = Instant::now();
    wait(cx, IDLE).await;
    let (c1, rss) = process_stats();
    let secs = t0.elapsed().as_secs_f64();
    this.update(cx, |w, _| {
        let b = w.bench.as_mut().unwrap();
        let renders = b.renders - r0.unwrap_or(0);
        b.row(scenario, "cpu", (c1 - c0) / secs * 100.0, "percent");
        b.row(scenario, "renders", renders as f64, "count");
        b.row(scenario, "rss", rss, "MB");
    })
    .ok();
}

/// The `file_10k` rows; see the module docs.
async fn files(this: &WeakEntity<Willow>, cx: &mut AsyncWindowContext) {
    let path = std::env::temp_dir().join("willow-bench-10k.willow");
    this.update_in(cx, |w, window, cx| {
        let save = |w: &mut Willow| {
            let t = Instant::now();
            (w.session.save(|b| crate::file::write_beside(&path, b))).expect("bench save");
            t.elapsed().as_secs_f64() * 1e3
        };
        let measure = Box::new(crate::text::TextMeasure::new(window.text_system().clone()));
        let doc = w.session.document().clone();
        let mut fresh = willow_core::Session::new(doc, w.session.theme(), measure);
        let t = Instant::now();
        (fresh.save(|b| crate::file::write_beside(&path, b))).expect("bench save");
        let unwarmed = t.elapsed().as_secs_f64() * 1e3;
        let (first, again) = (save(w), save(w));
        let t = Instant::now();
        let bytes = std::fs::read(&path).expect("bench read");
        let loaded = willow_core::load(&bytes).expect("bench load");
        let content = crate::Content::new(loaded, bytes, None, None);
        let vp = w.session.viewport();
        w.show(content, window, cx);
        let load = t.elapsed().as_secs_f64() * 1e3;
        w.session.set_viewport(vp);
        let b = w.bench.as_mut().unwrap();
        b.row("file_10k", "save_unwarmed", unwarmed, "ms");
        b.row("file_10k", "save", first, "ms");
        b.row("file_10k", "save_again", again, "ms");
        b.row("file_10k", "load", load, "ms");
    })
    .unwrap();
    let _ = std::fs::remove_file(&path);
    scripted(this, cx, "open_first_frame", None, Vec::new()).await;
}

fn at(zoom: f32, origin: Point, size: Point) -> Option<Viewport> {
    Some(Viewport { origin, zoom, size })
}

pub(crate) async fn run(this: WeakEntity<Willow>, cx: &mut AsyncWindowContext) {
    wait(cx, Duration::from_millis(1500)).await;
    let size = this.update(cx, |w, _| w.session.viewport().size).unwrap();
    let center = Point {
        x: size.x / 2.0,
        y: size.y / 2.0,
    };
    let mods = Modifiers::default();
    let pan = |dx: f32| Input::Wheel {
        pos: center,
        delta: Point { x: dx, y: 0.0 },
        mods,
    };
    files(&this, cx).await;
    // Each zoom pans right over new ground (cold: first layout of revealed shapes), then back
    // over the same ground (warm).
    let origin = Point { x: -40.0, y: -40.0 };
    for (name, zoom) in [("100", 1.0), ("40", 0.4), ("10", 0.1), ("2", 0.02)] {
        let cold = vec![pan(-PAN_PX); PAN_FRAMES];
        let s = format!("pan_zoom{name}_cold");
        scripted(&this, cx, &s, at(zoom, origin, size), cold).await;
        let warm = vec![pan(PAN_PX); PAN_FRAMES];
        scripted(&this, cx, &format!("pan_zoom{name}_warm"), None, warm).await;
    }
    idle(&this, cx, "idle_after_pan").await;

    // 100% to 10% and back around the canvas center, twice: cold rasterizes glyphs at each
    // size, warm reuses the atlas.
    let k = 0.1f32.powf(1.0 / SWEEP_FRAMES as f32);
    let pinch = |f: f32| Input::Pinch {
        pos: center,
        delta: f - 1.0,
    };
    let mut sweep = vec![pinch(k); SWEEP_FRAMES];
    sweep.extend(vec![pinch(1.0 / k); SWEEP_FRAMES]);
    let origin = Point {
        x: 20_000.0,
        y: 2_000.0,
    };
    for pass in ["cold", "warm"] {
        let s = format!("zoom_sweep_{pass}");
        scripted(&this, cx, &s, at(1.0, origin, size), sweep.clone()).await;
    }

    // Typing into the first shape: select it, F2, then keystrokes through the platform path.
    // Characters in the first shape, which the keystrokes should grow.
    let chars = |w: &Willow| w.session.document().shapes[0].text.plain().chars().count();
    let (before, chars0) = this
        .update_in(cx, |w, window, cx| {
            let s = &mut w.session;
            s.set_viewport(Viewport {
                origin: Point { x: -40.0, y: -40.0 },
                zoom: 1.0,
                size,
            });
            let pos = Point { x: 60.0, y: 60.0 };
            let now = w.start.elapsed();
            let button = Button::Left;
            s.handle(Input::PointerDown { pos, button, mods }, now);
            s.handle(Input::PointerUp { pos, button, mods }, now);
            s.handle(Input::StartEditing, now);
            assert!(s.editing().is_some(), "bench: no shape to type into");
            cx.notify();
            (window.frame_duration_snapshot(), chars(w))
        })
        .unwrap();
    wait(cx, Duration::from_millis(300)).await;
    // ASCII, so every byte is one key.
    const PHRASE: &str = "the quiet harbour remembers everything ";
    for i in 0..KEYSTROKES {
        let j = i % PHRASE.len();
        let key = match &PHRASE[j..j + 1] {
            " " => "space",
            k => k,
        };
        let key = Keystroke::parse(key).unwrap();
        // Outside any entity update, as the platform would.
        cx.update(|window, cx| window.dispatch_keystroke(key, cx))
            .ok();
        wait(cx, Duration::from_millis(40)).await;
    }
    wait(cx, Duration::from_millis(200)).await;
    this.update_in(cx, |w, window, _| {
        let after = window.frame_duration_snapshot();
        let typed = (chars(w) - chars0) as f64;
        let b = w.bench.as_mut().unwrap();
        b.row("typing_10k", "chars_typed", typed, "count");
        frame_rows(b, "typing_10k", &before, &after);
    })
    .ok();

    // Stop editing and show the zoom pill, so both the caret and the pill have to wind down.
    this.update(cx, |w, cx| {
        let now = w.start.elapsed();
        w.session.handle(Input::StopEditing, now);
        let pos = center;
        w.session.handle(Input::Pinch { pos, delta: 0.0 }, now);
        cx.notify();
    })
    .ok();
    idle(&this, cx, "idle_after_anim").await;
    this.update(cx, |w, cx| {
        w.bench.as_ref().unwrap().write_csv();
        cx.quit();
    })
    .ok();
}
