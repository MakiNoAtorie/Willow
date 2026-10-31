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
//! file opens), `save` the window's first, `save_again` the next; then `load` of that file
//! (read, parse, repair, auto-place, measure every shape, the spacing pass, a new session). The
//! loaded session replaces the window's, so `open_first_frame` is its first frame.
//!
//! `spacing_pass_10k` times, in milliseconds, full spacing passes on the laid-out document
//! (`SPACING_RUNS` of them, median and worst): `pass` as generated, where little crowds, and
//! `pass_crowded` with every position divided by 3, so that most chains have to move.
//!
//! Each `pan_zoom*_cold` starts on a new session of the same document (empty layout and word
//! caches; GPUI's glyph atlas stays), so its first frame lays out every shape for the spacing
//! pass, which `open` does before a real first frame; `pan_zoom*_warm` then pans back over the
//! same ground on that session. Before the zoom sweeps the bench lays out every shape, as
//! opening a file does, so the zoom sweeps, typing and `recovery_take` run on a fully laid-out
//! document.
//!
//! `zone_drag` drags the largest zone by its title band, out and back over 240 pointer moves,
//! zoomed to fit it on screen; the drop returns it to where it started.
//!
//! `recovery_take` times, in milliseconds, `Session::take_recovery` once the typing has made a
//! recovery write due: autosave's UI-thread part (clone the document, every shape's height
//! and drawn position).
//!
//! Last, the code panel opens on the document's canonical text. `panel_typing_10k` types into
//! the first shape's text through the platform path: frame rows, and `keystroke_*`, the time
//! to dispatch each key (its handling, before any draw). `panel_apply_10k` types one more
//! character and times, in milliseconds, its `check` (parse and check; off the UI thread in
//! the app) and `apply` (update the model and record the text in the history, on the UI
//! thread), then `undo` (Undo in the panel: the typed text comes back and is checked, on the UI
//! thread). `panel_regen_10k` nudges a shape
//! with the panel open and times the regeneration's `take` (clone the document, heights and
//! drawn positions, UI thread), `run` (write the text, off the UI thread) and `land` (show it, UI thread).
//! `idle_panel_open` is idle with the panel focused, once its caret has stopped blinking.

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
/// What the typing scenarios type, again and again. ASCII, so every byte is one key.
const PHRASE: &str = "the quiet harbour remembers everything ";
/// Full spacing passes per `spacing_pass_10k` row.
const SPACING_RUNS: usize = 20;
/// Pointer moves in the zone drag: out for half, back for half.
const DRAG_FRAMES: usize = 240;
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
            writeln!(
                f,
                "{}",
                csv_row(&[&self.machine, &rev, s, m, &format!("{v:.3}"), u])
            )
            .unwrap();
        }
        println!("appended {} rows to {CSV}", self.rows.len());
    }
}

/// A new session on `w`'s document that has laid nothing out.
fn unmeasured(w: &Willow, window: &Window) -> willow_core::Session {
    let measure = Box::new(crate::text::TextMeasure::new(window.text_system().clone()));
    let doc = w.session.document().clone();
    willow_core::Session::new(doc, w.session.theme(), measure)
}

/// One CSV line; a field holding a comma, quote or newline is quoted, its quotes doubled.
fn csv_row(fields: &[&str]) -> String {
    let field = |f: &&str| {
        if f.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", f.replace('"', "\"\""))
        } else {
            f.to_string()
        }
    };
    fields.iter().map(field).collect::<Vec<_>>().join(",")
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

/// Waits for animations and the caret's blink to end, then 1 s more, then records CPU and
/// renders over `IDLE`.
async fn idle(this: &WeakEntity<Willow>, cx: &mut AsyncWindowContext, scenario: &str) {
    while this
        .update(cx, |w, _| {
            let now = w.start.elapsed();
            w.session.animating(now) || w.session.next_wake(now).is_some()
        })
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
        let mut fresh = unmeasured(w, window);
        let t = Instant::now();
        (fresh.save(|b| crate::file::write_beside(&path, b))).expect("bench save");
        let unwarmed = t.elapsed().as_secs_f64() * 1e3;
        w.session.heights();
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

/// The median and worst of `SPACING_RUNS` full passes on `session`, in milliseconds.
fn passes(session: &willow_core::Session) -> (f64, f64) {
    let mut times: Vec<f64> = (0..SPACING_RUNS)
        .map(|_| timed(|| session.respace()).0)
        .collect();
    times.sort_by(f64::total_cmp);
    (times[SPACING_RUNS / 2], times[SPACING_RUNS - 1])
}

/// The `spacing_pass_10k` rows; see the module docs.
async fn spacing(this: &WeakEntity<Willow>, cx: &mut AsyncWindowContext) {
    this.update_in(cx, |w, window, _| {
        let (median, worst) = passes(&w.session);
        let mut doc = w.session.document().clone();
        for s in &mut doc.shapes {
            (s.x, s.y) = (s.x / 3, s.y / 3);
        }
        let measure = Box::new(crate::text::TextMeasure::new(window.text_system().clone()));
        let crowded = willow_core::Session::new(doc, w.session.theme(), measure);
        crowded.respace();
        let (crowded_median, crowded_worst) = passes(&crowded);
        let b = w.bench.as_mut().unwrap();
        b.row("spacing_pass_10k", "pass_p50", median, "ms");
        b.row("spacing_pass_10k", "pass_max", worst, "ms");
        b.row("spacing_pass_10k", "pass_crowded_p50", crowded_median, "ms");
        b.row("spacing_pass_10k", "pass_crowded_max", crowded_worst, "ms");
    })
    .unwrap();
}

/// Milliseconds `f` takes, and what it returns.
fn timed<T>(f: impl FnOnce() -> T) -> (f64, T) {
    let t = Instant::now();
    let out = f();
    (t.elapsed().as_secs_f64() * 1e3, out)
}

/// The `panel_*` and `idle_panel_open` rows; see the module docs.
async fn code_panel(this: &WeakEntity<Willow>, cx: &mut AsyncWindowContext) {
    // Open the panel and wait for its first regeneration to land.
    (this.update(cx, |w, cx| w.input(Input::TogglePanel, cx))).unwrap();
    while this
        .update(cx, |w, _| w.session.panel().text().is_empty())
        .unwrap_or(false)
    {
        wait(cx, Duration::from_millis(50)).await;
    }
    // The caret inside the first shape's text, so what is typed stays valid and applies.
    let (before, len0) = this
        .update_in(cx, |w, window, cx| {
            let now = w.start.elapsed();
            let text = w.session.panel().text();
            let at = text
                .find(": \"")
                .expect("bench: no shape line in the panel")
                + 3;
            for _ in 0..text[..at].chars().count() {
                w.session
                    .handle(Input::Move(willow_core::Motion::Right), now);
            }
            cx.notify();
            (
                window.frame_duration_snapshot(),
                w.session.panel().text().len(),
            )
        })
        .unwrap();
    wait(cx, Duration::from_millis(300)).await;
    let mut keys = Vec::new();
    for i in 0..KEYSTROKES {
        let j = i % PHRASE.len();
        let key = match &PHRASE[j..j + 1] {
            " " => "space",
            k => k,
        };
        let key = Keystroke::parse(key).unwrap();
        let (ms, _) = timed(|| cx.update(|window, cx| window.dispatch_keystroke(key, cx)));
        keys.push(ms);
        wait(cx, Duration::from_millis(40)).await;
    }
    keys.sort_by(f64::total_cmp);
    wait(cx, Duration::from_millis(300)).await;
    this.update_in(cx, |w, window, _| {
        let after = window.frame_duration_snapshot();
        let typed = (w.session.panel().text().len() - len0) as f64;
        let b = w.bench.as_mut().unwrap();
        b.row("panel_typing_10k", "chars_typed", typed, "count");
        let p95 = keys[keys.len() * 95 / 100];
        b.row("panel_typing_10k", "keystroke_p95", p95, "ms");
        b.row(
            "panel_typing_10k",
            "keystroke_max",
            keys[keys.len() - 1],
            "ms",
        );
        frame_rows(b, "panel_typing_10k", &before, &after);
    })
    .ok();

    // One more character after a pause, so it is an undo step of its own: its check (off the
    // UI thread in the app), its apply, and the undo that brings back the typed text.
    wait(cx, Duration::from_millis(1100)).await;
    this.update(cx, |w, cx| {
        let s = &mut w.session;
        let now = w.start.elapsed();
        let doc = s.document().clone();
        let (text, replace) = ("x".to_string(), None);
        s.handle(Input::Text { text, replace }, now);
        let job = s.take_text_check(now + Duration::from_secs(1));
        let job = job.expect("bench: no panel text check due after typing");
        let (check, done) = timed(|| job.run());
        let (apply, _) = timed(|| s.text_checked(done));
        assert!(*s.document() != doc, "bench: the typed text didn't apply");
        let (undo, _) = timed(|| s.handle(Input::Undo, now));
        assert!(
            *s.document() == doc,
            "bench: undo didn't take the character back"
        );
        let b = w.bench.as_mut().unwrap();
        b.row("panel_apply_10k", "check", check, "ms");
        b.row("panel_apply_10k", "apply", apply, "ms");
        b.row("panel_apply_10k", "undo", undo, "ms");
        cx.notify();
    })
    .ok();

    // A canvas step with the panel open: leave the panel, nudge a shape, regenerate.
    this.update(cx, |w, cx| {
        let s = &mut w.session;
        let now = w.start.elapsed();
        s.handle(Input::Escape, now);
        // Leaving regenerates what was typed; land that first.
        if let Some(job) = s.take_regeneration() {
            s.regenerated(job.run());
        }
        let id = s.document().shapes[0].id.clone();
        s.select([id]);
        s.handle(Input::Nudge(willow_core::Side::Right), now);
        let (take, job) = timed(|| s.take_regeneration());
        let job = job.expect("bench: no regeneration due after a nudge");
        let (run, done) = timed(|| job.run());
        let (land, _) = timed(|| s.regenerated(done));
        let b = w.bench.as_mut().unwrap();
        b.row("panel_regen_10k", "take", take, "ms");
        b.row("panel_regen_10k", "run", run, "ms");
        b.row("panel_regen_10k", "land", land, "ms");
        cx.notify();
    })
    .ok();

    // Back in the panel, its caret blinking, then nothing.
    (this.update(cx, |w, cx| w.input(Input::FocusPanel, cx))).ok();
    idle(this, cx, "idle_panel_open").await;
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
    spacing(&this, cx).await;
    // Each zoom pans right over new ground on a new session (cold: first layout of every shape
    // in view), then back over the same ground (warm).
    let origin = Point { x: -40.0, y: -40.0 };
    for (name, zoom) in [("100", 1.0), ("40", 0.4), ("10", 0.1), ("2", 0.02)] {
        (this.update_in(cx, |w, window, _| w.session = unmeasured(w, window))).unwrap();
        let cold = vec![pan(-PAN_PX); PAN_FRAMES];
        let s = format!("pan_zoom{name}_cold");
        scripted(&this, cx, &s, at(zoom, origin, size), cold).await;
        let warm = vec![pan(PAN_PX); PAN_FRAMES];
        scripted(&this, cx, &format!("pan_zoom{name}_warm"), None, warm).await;
    }
    idle(&this, cx, "idle_after_pan").await;
    this.update(cx, |w, _| drop(w.session.heights())).unwrap();

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

    // Drag the largest zone by its title band, out and back, zoomed to fit it on screen
    // (not below the UI's 10% minimum). The drop returns it to where it started.
    let zone = this
        .update(cx, |w, _| {
            let zs = &w.session.document().zones;
            zs.iter()
                .max_by_key(|z| z.width as u64 * z.height as u64)
                .cloned()
        })
        .unwrap()
        .expect("bench: the generated document has no zone");
    let zoom = (size.x * 0.8 / zone.width as f32)
        .min(size.y * 0.8 / zone.height as f32)
        .clamp(0.1, 1.0);
    // The zone's top-left corner at screen (60, 80). Press in the middle of its band, at the
    // first point along it where the session says the zone's band is under the pointer (a member
    // shape or a nested zone can cover the rest); no such point fails the bench.
    let corner = Point { x: 60.0, y: 80.0 };
    let origin = Point {
        x: zone.x as f32 - corner.x / zoom,
        y: zone.y as f32 - corner.y / zoom,
    };
    let grab = this
        .update(cx, |w, _| {
            let s = &mut w.session;
            s.set_viewport(Viewport { origin, zoom, size });
            let index = s.document().zones.iter().position(|z| z.id == zone.id);
            let y = corner.y + s.theme().zone.band / 2.0;
            let width = zone.width as f32 * zoom;
            (0..)
                .map(|i| Point {
                    x: corner.x + 8.0 + 8.0 * i as f32,
                    y,
                })
                .take_while(|p| p.x < corner.x + width)
                .find(|&p| s.band_at(p) == index)
        })
        .unwrap()
        .expect("bench: no point on the largest zone's title band hits it");
    let button = Button::Left;
    let mut drag = vec![Input::PointerDown {
        pos: grab,
        button,
        mods,
    }];
    let step = |i: usize| {
        let k = (i.min(DRAG_FRAMES - i)) as f32;
        let pos = Point {
            x: grab.x + 3.0 * k,
            y: grab.y + 2.0 * k,
        };
        Input::PointerMove { pos, mods }
    };
    drag.extend((1..=DRAG_FRAMES).map(step));
    drag.push(Input::PointerUp {
        pos: grab,
        button,
        mods,
    });
    scripted(&this, cx, "zone_drag", at(zoom, origin, size), drag).await;

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

    // Autosave's UI-thread part, as if 60 s had passed since the typing.
    this.update(cx, |w, _| {
        let due = w.start.elapsed() + Duration::from_secs(60);
        let t = Instant::now();
        let r = w.session.take_recovery(due);
        let took = t.elapsed().as_secs_f64() * 1e3;
        let write = matches!(r, Some(willow_core::Recovery::Write { .. }));
        assert!(write, "bench: no recovery write due after typing");
        (w.bench.as_mut().unwrap()).row("recovery_take", "take_recovery", took, "ms");
    })
    .ok();

    // Stop editing and show the zoom pill, so both the caret and the pill have to wind down.
    this.update(cx, |w, cx| {
        let now = w.start.elapsed();
        w.session.handle(Input::Escape, now);
        let pos = center;
        w.session.handle(Input::Pinch { pos, delta: 0.0 }, now);
        cx.notify();
    })
    .ok();
    idle(&this, cx, "idle_after_anim").await;
    code_panel(&this, cx).await;
    this.update(cx, |w, cx| {
        w.bench.as_ref().unwrap().write_csv();
        cx.quit();
    })
    .ok();
}

#[cfg(test)]
mod tests {
    #[test]
    fn csv_row_quotes_fields_with_commas_quotes_or_newlines() {
        let row = super::csv_row(&["a,b", "4cf2ae8", "say \"hi\"", "two\nlines", "1.000", "ms"]);
        assert_eq!(
            row,
            "\"a,b\",4cf2ae8,\"say \"\"hi\"\"\",\"two\nlines\",1.000,ms"
        );
    }
}
