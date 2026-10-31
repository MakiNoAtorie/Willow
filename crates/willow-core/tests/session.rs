mod common;

use std::time::Duration;
use willow_core::{Button, Cursor, Document, Input, Key, Modifiers, PAPER, Point, Session};

const T: Duration = Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// A session over a 1000 x 800 canvas with the new document's shape centered.
fn session() -> Session {
    let mut s = Session::new(Document::new(), &PAPER, Box::new(common::Fake));
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

#[test]
fn losing_focus_releases_space_and_ends_the_pan() {
    let mut s = session();
    let mods = Modifiers::default();
    s.handle(Input::KeyDown(Key::Space), T);
    let (pos, button) = (ON_SHAPE, Button::Left);
    s.handle(Input::PointerDown { pos, button, mods }, T);

    assert_eq!(s.handle(Input::FocusLost, T).cursor, Cursor::Arrow);
    let before = s.viewport();
    let pos = pt(600.0, 500.0);
    s.handle(Input::PointerMove { pos, mods }, T);
    assert_eq!(s.viewport(), before);
    drag(&mut s, Button::Left, ON_SHAPE, pt(600.0, 500.0));
    assert_eq!(s.viewport(), before);
}

fn wheel(s: &mut Session, dx: f32, dy: f32, mods: Modifiers) -> willow_core::Response {
    s.handle(
        Input::Wheel {
            pos: ON_SHAPE,
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

fn click(s: &mut Session, pos: Point, now: Duration) -> willow_core::Response {
    let (button, mods) = (Button::Left, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, now);
    s.handle(Input::PointerUp { pos, button, mods }, now)
}

fn id(s: &str) -> willow_core::ShapeId {
    willow_core::ShapeId(s.into())
}

#[test]
fn press_release_on_an_unselected_shape_selects_it() {
    let mut s = session();
    assert!(s.selection().is_empty());

    click(&mut s, ON_SHAPE, T);

    assert_eq!(s.selection(), [id("a")]);
}

#[test]
fn press_drag_on_a_shape_moves_it_on_release_instead_of_selecting() {
    let mut s = session();
    let mods = Modifiers::default();
    s.handle(
        Input::PointerDown {
            pos: ON_SHAPE,
            button: Button::Left,
            mods,
        },
        T,
    );

    s.handle(
        Input::PointerMove {
            pos: pt(530.0, 380.0),
            mods,
        },
        T,
    );
    // Previewed live; the document only changes on release.
    let p = s.drag_preview().map(|p| (p.id.clone(), p.x, p.y));
    assert_eq!(p, Some((id("a"), 30, -20)));
    assert_eq!((s.document().shapes[0].x, s.document().shapes[0].y), (0, 0));

    s.handle(
        Input::PointerUp {
            pos: pt(530.0, 380.0),
            button: Button::Left,
            mods,
        },
        T,
    );
    assert_eq!(s.drag_preview(), None);
    assert_eq!(
        (s.document().shapes[0].x, s.document().shapes[0].y),
        (30, -20)
    );
    assert!(s.selection().is_empty());
}

#[test]
fn moved_positions_are_whole_canvas_units_at_any_zoom() {
    // A 10 x 7 px drag is 10/z x 7/z canvas units, rounded.
    for (zoom, expected) in [(0.37, (27, 19)), (1.0, (10, 7)), (2.5, (4, 3))] {
        let mut s = session();
        s.set_viewport(willow_core::Viewport {
            zoom,
            ..s.viewport()
        });
        let on = s.viewport().to_screen(pt(60.0, 24.0));

        drag(&mut s, Button::Left, on, pt(on.x + 10.0, on.y + 7.0));

        let a = &s.document().shapes[0];
        assert_eq!((a.x, a.y), expected, "zoom {zoom}");
    }
}

#[test]
fn clicking_empty_canvas_deselects_but_panning_does_not() {
    let mut s = session();
    click(&mut s, ON_SHAPE, T);

    drag(&mut s, Button::Left, pt(50.0, 50.0), pt(90.0, 50.0));
    assert_eq!(s.selection(), [id("a")]);

    click(&mut s, pt(50.0, 50.0), T);
    assert!(s.selection().is_empty());
}

#[test]
fn a_click_where_shapes_overlap_hits_the_later_one() {
    let mut doc = Document::new();
    let b = doc.next_id();
    doc.shapes.push(willow_core::Shape::new(b.clone(), 100, 20));
    // Linked: the spacing pass keeps separate chains apart.
    doc.links.push(willow_core::Link(id("a"), b));
    let mut s = Session::new(doc, &PAPER, Box::new(common::Fake));
    s.resize(1000.0, 800.0);

    // Canvas (150, 30) is inside both a (0..210) and b (100..310).
    let overlap = s.viewport().to_screen(pt(150.0, 30.0));
    click(&mut s, overlap, T);

    assert_eq!(s.selection(), [id("b")]);
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn the_selection_ring_fades_in_and_out_over_120_ms_and_then_stops() {
    let mut s = session();
    let a = id("a");

    assert!(click(&mut s, ON_SHAPE, ms(1000)).animating);
    assert_eq!(s.selection_rings(ms(1000)), vec![(&a, 0.0)]);
    // Paper's `120ms ease-out`: CSS ease-out is 0.685 halfway.
    let mid = s.selection_rings(ms(1060))[0].1;
    assert!((mid - 0.685).abs() < 0.005, "{mid}");
    assert!(s.animating(ms(1060)));
    assert_eq!(s.selection_rings(ms(1120)), vec![(&a, 1.0)]);
    assert!(!s.animating(ms(1120)));

    assert!(click(&mut s, pt(50.0, 50.0), ms(2000)).animating);
    assert_eq!(s.selection_rings(ms(2000)), vec![(&a, 1.0)]);
    assert!(s.animating(ms(2060)));
    assert_eq!(s.selection_rings(ms(2120)), vec![]);
    assert!(!s.animating(ms(2120)));
}

fn pos(s: &Session) -> (i32, i32) {
    let shape = &s.document().shapes[0];
    (shape.x, shape.y)
}

#[test]
fn a_move_is_one_undo_step_that_redo_reapplies() {
    let mut s = session();
    assert!(!s.can_undo() && !s.can_redo());

    drag(&mut s, Button::Left, ON_SHAPE, pt(530.0, 420.0));
    assert_eq!(pos(&s), (30, 20));
    assert!(s.can_undo() && !s.can_redo());

    s.handle(Input::Undo, T);
    assert_eq!(pos(&s), (0, 0));
    assert!(!s.can_undo() && s.can_redo());

    s.handle(Input::Redo, T);
    assert_eq!(pos(&s), (30, 20));
    assert!(s.can_undo() && !s.can_redo());
}

#[test]
fn pan_zoom_and_selection_are_not_undo_steps_and_undo_leaves_them_alone() {
    let mut s = session();
    drag(&mut s, Button::Left, ON_SHAPE, pt(530.0, 420.0));

    click(&mut s, pt(560.0, 425.0), T);
    drag(&mut s, Button::Left, pt(50.0, 50.0), pt(80.0, 10.0));
    wheel(&mut s, 5.0, 7.0, Modifiers::default());
    let pinch = Input::Pinch {
        pos: pt(200.0, 100.0),
        delta: 0.5,
    };
    s.handle(pinch, T);
    let vp = s.viewport();

    s.handle(Input::Undo, T);
    assert_eq!(pos(&s), (0, 0));
    assert_eq!(s.viewport(), vp);
    assert_eq!(s.selection(), [id("a")]);
    assert!(!s.can_undo());
}

#[test]
fn a_new_move_after_undo_clears_redo() {
    let mut s = session();
    drag(&mut s, Button::Left, ON_SHAPE, pt(530.0, 420.0));
    s.handle(Input::Undo, T);

    drag(&mut s, Button::Left, ON_SHAPE, pt(490.0, 400.0));
    assert!(!s.can_redo());
    s.handle(Input::Redo, T);
    assert_eq!(pos(&s), (-10, 0));

    s.handle(Input::Undo, T);
    assert_eq!(pos(&s), (0, 0));
    assert!(!s.can_undo());
}

/// Empty shapes a, b, c stacked 300 apart, a selected.
fn three() -> Session {
    let at = [("a", 0), ("b", 300), ("c", 600)];
    let mut s = common::session_of(Document {
        shapes: (at.iter())
            .map(|&(n, y)| willow_core::Shape::new(id(n), 0, y))
            .collect(),
        ..Document::default()
    });
    s.select([id("a")]);
    s
}

fn at(s: &Session, of: &str) -> (i32, i32) {
    let sh = (s.document().shapes.iter())
        .find(|x| x.id == id(of))
        .unwrap();
    (sh.x, sh.y)
}

fn press(s: &mut Session, canvas: Point) {
    let (pos, mods) = (s.viewport().to_screen(canvas), Modifiers::default());
    s.handle(
        Input::PointerDown {
            pos,
            button: Button::Left,
            mods,
        },
        T,
    );
}

/// Moves to canvas point `canvas` and releases the left button there.
fn release_at(s: &mut Session, canvas: Point) {
    let (pos, mods) = (s.viewport().to_screen(canvas), Modifiers::default());
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(
        Input::PointerUp {
            pos,
            button: Button::Left,
            mods,
        },
        T,
    );
}

#[test]
fn deleting_during_a_press_ends_it_and_moves_nothing() {
    let mut s = three();
    press(&mut s, pt(100.0, 620.0));
    s.handle(Input::DeleteShapes, T);
    assert!(s.drag_preview().is_none());
    release_at(&mut s, pt(100.0, 700.0));
    assert_eq!((at(&s, "b"), at(&s, "c")), ((0, 300), (0, 600)));
    assert!(s.selection().is_empty());
}

#[test]
fn undo_during_a_press_ends_it_and_moves_no_other_shape() {
    let mut s = three();
    s.handle(Input::DeleteShapes, T);
    press(&mut s, pt(100.0, 620.0));
    s.handle(Input::Undo, T);
    release_at(&mut s, pt(100.0, 700.0));
    assert_eq!((at(&s, "b"), at(&s, "c")), ((0, 300), (0, 600)));
}

#[test]
fn cut_during_a_resize_ends_it() {
    let mut s = three();
    // c's right edge, away from its midpoint; the press selects c.
    press(&mut s, pt(210.0, 604.0));
    assert_eq!(s.selection(), [id("c")]);
    let r = s.handle(Input::Cut, T);
    assert_eq!(r.cursor, Cursor::Arrow);
    assert!(s.resize_preview().is_none());
    release_at(&mut s, pt(300.0, 604.0));
    assert_eq!(s.document().shapes.len(), 2);
    assert_eq!(at(&s, "b"), (0, 300));
}

#[test]
fn deleting_during_a_midpoint_press_creates_nothing() {
    let mut s = three();
    // c's right midpoint.
    press(&mut s, pt(210.0, 624.25));
    s.handle(Input::DeleteShapes, T);
    release_at(&mut s, pt(210.0, 624.25));
    assert_eq!(s.document().shapes.len(), 2);
    assert!(s.document().links.is_empty());
}

#[test]
fn losing_focus_ends_every_gesture() {
    let mut s = three();
    press(&mut s, pt(100.0, 620.0));
    s.handle(Input::FocusLost, T);
    // The press is gone: a new one on b starts afresh and its release selects b.
    press(&mut s, pt(100.0, 320.0));
    release_at(&mut s, pt(100.0, 320.0));
    assert_eq!(s.selection(), [id("b")]);
    assert_eq!(at(&s, "c"), (0, 600));
}
