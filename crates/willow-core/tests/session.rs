use std::time::Duration;
use willow_core::{Button, Cursor, Document, Input, Key, Modifiers, PAPER, Point, Session};

const T: Duration = Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// A session over a 1000 x 800 canvas with the new document's shape centered.
fn session() -> Session {
    let mut s = Session::new(Document::new(), &PAPER);
    s.resize(1000.0, 800.0);
    s
}

fn drag(s: &mut Session, button: Button, from: Point, to: Point) {
    let mods = Modifiers::default();
    s.handle(
        Input::PointerDown {
            pos: from,
            button,
            mods,
        },
        T,
    );
    s.handle(Input::PointerMove { pos: to, mods }, T);
    s.handle(
        Input::PointerUp {
            pos: to,
            button,
            mods,
        },
        T,
    );
}

#[test]
fn left_drag_on_empty_canvas_pans_by_the_drag_distance() {
    let mut s = session();
    let origin = s.viewport().to_screen(pt(0.0, 0.0));

    drag(&mut s, Button::Left, pt(50.0, 50.0), pt(80.0, 10.0));

    assert_eq!(
        s.viewport().to_screen(pt(0.0, 0.0)),
        pt(origin.x + 30.0, origin.y - 40.0)
    );
}

/// Screen center: inside the centered new shape.
const ON_SHAPE: Point = Point { x: 500.0, y: 400.0 };

#[test]
fn the_new_shape_starts_centered_on_whole_pixels() {
    let s = session();
    // 210 x 48.5 in a 1000 x 800 canvas.
    assert_eq!(s.viewport().to_screen(pt(0.0, 0.0)), pt(395.0, 376.0));
}

#[test]
fn left_drag_on_a_shape_does_not_pan() {
    let mut s = session();
    let before = s.viewport();

    drag(&mut s, Button::Left, ON_SHAPE, pt(600.0, 500.0));

    assert_eq!(s.viewport(), before);
}

#[test]
fn middle_drag_pans_even_over_a_shape() {
    let mut s = session();

    drag(&mut s, Button::Middle, ON_SHAPE, pt(520.0, 390.0));

    assert_eq!(s.viewport().to_screen(pt(0.0, 0.0)), pt(415.0, 366.0));
}

#[test]
fn space_held_turns_left_drag_over_a_shape_into_a_pan() {
    let mut s = session();

    let r = s.handle(Input::KeyDown(Key::Space), T);
    assert_eq!(r.cursor, Cursor::Grab);
    drag(&mut s, Button::Left, ON_SHAPE, pt(520.0, 390.0));
    assert_eq!(s.viewport().to_screen(pt(0.0, 0.0)), pt(415.0, 366.0));

    s.handle(Input::KeyUp(Key::Space), T);
    let before = s.viewport();
    drag(&mut s, Button::Left, ON_SHAPE, pt(600.0, 500.0));
    assert_eq!(s.viewport(), before);
}

fn wheel(s: &mut Session, dx: f32, dy: f32, mods: Modifiers) -> willow_core::Response {
    s.handle(
        Input::Wheel {
            delta: pt(dx, dy),
            mods,
        },
        T,
    )
}

#[test]
fn wheel_and_trackpad_scroll_move_the_content_by_the_delta() {
    let mut s = session();

    let r = wheel(&mut s, 12.5, -40.0, Modifiers::default());

    assert!(r.redraw);
    assert_eq!(s.viewport().to_screen(pt(0.0, 0.0)), pt(407.5, 336.0));
}

#[test]
fn shift_wheel_pans_horizontally() {
    let mut s = session();
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };

    wheel(&mut s, 0.0, -40.0, shift);

    assert_eq!(s.viewport().to_screen(pt(0.0, 0.0)), pt(355.0, 376.0));
}

#[test]
fn a_pan_shows_the_grabbing_cursor_and_ends_on_release() {
    let mut s = session();
    let mods = Modifiers::default();

    let down = Input::PointerDown {
        pos: pt(50.0, 50.0),
        button: Button::Left,
        mods,
    };
    assert_eq!(s.handle(down, T).cursor, Cursor::Grabbing);
    let up = Input::PointerUp {
        pos: pt(50.0, 50.0),
        button: Button::Left,
        mods,
    };
    assert_eq!(s.handle(up, T).cursor, Cursor::Arrow);

    let before = s.viewport();
    let r = s.handle(
        Input::PointerMove {
            pos: pt(90.0, 90.0),
            mods,
        },
        T,
    );
    assert!(!r.redraw);
    assert_eq!(s.viewport(), before);
}
