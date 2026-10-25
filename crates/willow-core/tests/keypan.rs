//! Keyboard pan: with nothing selected, arrows pan the view by key-down and key-up, not the
//! OS repeat rate.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{Document, Input, Key, Point, Session, Side};

fn ms(t: u64) -> Duration {
    Duration::from_millis(t)
}

/// An empty session at `zoom`, nothing selected.
fn at_zoom(zoom: f32) -> Session {
    let mut s = session_of(Document::default());
    let mut vp = s.viewport();
    vp.zoom = zoom;
    s.set_viewport(vp);
    s
}

/// Ticks every 16 ms from `from` to `to`.
fn run(s: &mut Session, from: u64, to: u64) {
    (from..=to).step_by(16).for_each(|t| {
        s.tick(ms(t));
    });
    s.tick(ms(to));
}

/// How far the view has moved from the origin, in screen pixels.
fn moved(s: &Session) -> Point {
    let vp = s.viewport();
    Point {
        x: vp.origin.x * vp.zoom,
        y: vp.origin.y * vp.zoom,
    }
}

fn down(s: &mut Session, side: Side, t: u64) {
    s.handle(Input::Navigate(side), ms(t));
}

fn up(s: &mut Session, side: Side, t: u64) {
    s.handle(Input::KeyUp(Key::Arrow(side)), ms(t));
}

#[test]
fn a_tap_pans_80_screen_px_at_any_zoom() {
    for zoom in [0.5, 1.0, 2.0] {
        for held in [0, 30, 60] {
            let mut s = at_zoom(zoom);
            down(&mut s, Side::Right, 0);
            run(&mut s, 0, held);
            up(&mut s, Side::Right, held);
            run(&mut s, held, 1000);
            let m = moved(&s);
            assert!((m.x - 80.0).abs() < 0.01, "zoom {zoom} held {held}: {m:?}");
            assert_eq!(m.y, 0.0);
        }
    }
}

#[test]
fn a_held_arrow_pans_at_constant_screen_speed_at_any_zoom_and_eases_out() {
    for zoom in [0.5, 1.0, 2.0] {
        let mut s = at_zoom(zoom);
        down(&mut s, Side::Bottom, 0);
        // OS repeats while held change nothing.
        (30..1000)
            .step_by(33)
            .for_each(|t| down(&mut s, Side::Bottom, t));
        run(&mut s, 0, 500);
        let at = moved(&s).y;
        run(&mut s, 500, 1000);
        let speed = (moved(&s).y - at) / 0.5;
        assert!((speed - 900.0).abs() < 0.5, "zoom {zoom}: {speed} px/s");
        up(&mut s, Side::Bottom, 1000);
        let released = moved(&s).y;
        run(&mut s, 1000, 1060);
        let mid = moved(&s).y - released;
        run(&mut s, 1060, 2000);
        let tail = moved(&s).y - released;
        assert!(
            mid > 0.0 && tail > mid,
            "zoom {zoom}: keeps moving after key-up"
        );
        assert!(tail < 900.0 * 0.12, "zoom {zoom}: eases out, tail {tail}");
        assert_eq!(moved(&s).x, 0.0);
    }
}

#[test]
fn two_held_arrows_pan_diagonally_and_focus_loss_stops_the_pan() {
    let mut s = session_of(Document::default());
    down(&mut s, Side::Left, 0);
    down(&mut s, Side::Top, 0);
    run(&mut s, 0, 500);
    let m = moved(&s);
    assert!(m.x < -300.0 && (m.x - m.y).abs() < 0.01, "{m:?}");
    s.handle(Input::FocusLost, ms(500));
    run(&mut s, 500, 2000);
    let stopped = moved(&s);
    run(&mut s, 2000, 3000);
    assert_eq!(moved(&s), stopped, "no key-up needed after focus loss");
    assert!(!s.animating(ms(3000)));
}

#[test]
fn the_pan_reports_done_once_settled() {
    let mut s = session_of(Document::default());
    let r = s.handle(Input::Navigate(Side::Right), ms(0));
    assert!(r.redraw && r.animating);
    run(&mut s, 0, 400);
    assert!(s.animating(ms(400)), "still held");
    up(&mut s, Side::Right, 400);
    run(&mut s, 400, 520);
    assert!(!s.animating(ms(520)));
    assert!(!s.tick(ms(600)).redraw, "idle: no redraws once settled");
    assert_eq!(s.next_wake(ms(600)), None);
}
