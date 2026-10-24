use std::time::Duration;
use willow_core::{CubicBezier, Document, Input, Modifiers, PAPER, Point, Session};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn close_pt(a: Point, b: Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

fn session() -> Session {
    let mut s = Session::new(Document::new(), &PAPER);
    s.resize(1000.0, 800.0);
    s
}

const CTRL: Modifiers = Modifiers {
    shift: false,
    ctrl: true,
    alt: false,
    platform: false,
};

const CURSOR: Point = Point { x: 300.0, y: 200.0 };

/// One Ctrl+wheel event at `pos` and time `t`.
fn zoom_wheel(s: &mut Session, pos: Point, dy: f32, t: Duration) -> willow_core::Response {
    s.handle(
        Input::Wheel {
            pos,
            delta: pt(0.0, dy),
            mods: CTRL,
        },
        t,
    )
}

#[test]
fn cubic_bezier_matches_css_reference_values() {
    // Reference values from a bisection solve of the CSS curves.
    let ease_in = CubicBezier(0.42, 0.0, 1.0, 1.0);
    let paper_step = CubicBezier(0.2, 0.8, 0.2, 1.0);
    for (curve, x, y) in [
        (ease_in, 0.25, 0.09346),
        (ease_in, 0.5, 0.31536),
        (paper_step, 0.25, 0.76728),
        (paper_step, 0.5, 0.94608),
        (paper_step, 0.0, 0.0),
        (paper_step, 1.0, 1.0),
    ] {
        assert!(
            (curve.at(x) - y).abs() < 1e-4,
            "{curve:?} at {x}: {}",
            curve.at(x)
        );
    }
}

#[test]
fn a_wheel_zoom_step_eases_in_around_the_fixed_cursor_point() {
    let mut s = session();
    let under_cursor = s.viewport().to_canvas(CURSOR);

    zoom_wheel(&mut s, CURSOR, 60.0, ms(1000));
    let mut zooms = vec![];
    for t in (1000..=1160).step_by(20) {
        s.tick(ms(t));
        let vp = s.viewport();
        assert!(
            close_pt(vp.to_canvas(CURSOR), under_cursor),
            "cursor point moved at {t} ms"
        );
        zooms.push(vp.zoom);
    }

    let end = zooms[6]; // 1120 ms: the 120 ms step is over.
    assert!(end > 1.0, "wheel up zooms in");
    assert!(zooms.windows(2).all(|w| w[0] <= w[1]), "{zooms:?}");
    assert_eq!(zooms[6..], [end, end, end]);
    // Halfway (1060 ms) the Paper step curve is at 94.6%.
    assert!(close(zooms[3], 1.0 + 0.94608 * (end - 1.0)), "{zooms:?}");
}

/// Zoom after one 60 px Ctrl+wheel step has finished.
fn one_step_zoom() -> f32 {
    let mut s = session();
    zoom_wheel(&mut s, CURSOR, 60.0, ms(0));
    s.tick(ms(500));
    s.viewport().zoom
}

#[test]
fn a_step_mid_animation_retargets_from_the_current_zoom_without_a_jump() {
    let step = one_step_zoom();
    let mut s = session();
    zoom_wheel(&mut s, CURSOR, 60.0, ms(1000));
    s.tick(ms(1040));
    let mid = s.viewport().zoom;
    assert!(mid > 1.0 && mid < step);

    zoom_wheel(&mut s, CURSOR, 60.0, ms(1040));
    assert_eq!(s.viewport().zoom, mid, "no jump on the retargeting event");
    s.tick(ms(1041));
    assert!(
        (s.viewport().zoom - mid).abs() < 0.05,
        "no jump right after"
    );

    s.tick(ms(1159));
    assert!(
        s.viewport().zoom < step * step,
        "the new step runs its own 120 ms"
    );
    s.tick(ms(1160));
    assert!(close(s.viewport().zoom, step * step), "two steps compound");
}

#[test]
fn pinch_follows_the_fingers_directly_around_the_gesture_center() {
    let mut s = session();
    let under = s.viewport().to_canvas(CURSOR);

    s.handle(
        Input::Pinch {
            pos: CURSOR,
            delta: 0.25,
        },
        ms(0),
    );

    assert!(close(s.viewport().zoom, 1.25), "applied at once, no easing");
    assert!(close_pt(s.viewport().to_canvas(CURSOR), under));
}

#[test]
fn reset_eases_back_to_100_percent_around_the_window_center() {
    let mut s = session();
    zoom_wheel(&mut s, CURSOR, -300.0, ms(0));
    s.tick(ms(500));
    let center = pt(500.0, 400.0);
    let under = s.viewport().to_canvas(center);
    let zoomed = s.viewport().zoom;

    s.handle(Input::ResetZoom, ms(1000));
    s.tick(ms(1060));
    let mid = s.viewport().zoom;
    assert!(mid > zoomed && mid < 1.0, "eases: {mid}");
    s.tick(ms(1120));

    assert_eq!(s.viewport().zoom, 1.0);
    assert!(close_pt(s.viewport().to_canvas(center), under));
}

#[test]
fn the_pill_shows_the_target_level_holds_700_ms_then_fades_over_400_ms() {
    let mut s = session();
    assert_eq!(s.zoom_pill(ms(0)), None);
    s.handle(
        Input::Pinch {
            pos: CURSOR,
            delta: 0.5,
        },
        ms(1000),
    );

    let opacity = |t| s.zoom_pill(ms(t)).map(|p| p.opacity);
    assert_eq!(opacity(1000), Some(1.0));
    assert_eq!(opacity(1700), Some(1.0));
    // Halfway through the ease-in fade: 1 - 0.31536.
    assert!(close(opacity(1900).unwrap(), 0.68464));
    assert_eq!(opacity(2100), None);
    assert_eq!(s.zoom_pill(ms(1000)).unwrap().percent, 150);
}

#[test]
fn animation_stops_once_the_pill_has_faded_after_the_last_zoom_input() {
    let mut s = session();
    assert!(!s.tick(ms(0)).animating, "idle before any zoom");

    assert!(zoom_wheel(&mut s, CURSOR, 60.0, ms(1000)).animating);
    assert!(zoom_wheel(&mut s, CURSOR, 60.0, ms(1050)).animating);
    for t in (1060..2150).step_by(16) {
        let r = s.tick(ms(t));
        assert!(r.animating && r.redraw, "running at {t}");
    }
    let r = s.tick(ms(2150));
    assert!(!r.animating, "the pill (shown at 1050) is gone at 2150");
    assert!(r.redraw, "one last frame clears the pill");
    assert_eq!(s.tick(ms(2166)), willow_core::Response::default());
}

#[test]
fn zoom_never_leaves_10_to_400_percent() {
    let mut s = session();
    let mut t = 0;
    for dy in [500.0, -500.0] {
        for _ in 0..40 {
            t += 30;
            zoom_wheel(&mut s, CURSOR, dy, ms(t));
            let delta = if dy > 0.0 { 1.0 } else { -0.9 };
            s.handle(Input::Pinch { pos: CURSOR, delta }, ms(t));
            let z = s.viewport().zoom;
            assert!((0.1..=4.0).contains(&z), "{z}");
            for k in 1..=4 {
                s.tick(ms(t + 10 * k));
                let z = s.viewport().zoom;
                assert!((0.1..=4.0).contains(&z), "{z}");
            }
        }
        s.tick(ms(t + 500));
        assert_eq!(s.viewport().zoom, if dy > 0.0 { 4.0 } else { 0.1 });
    }
}
