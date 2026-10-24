mod common;

use std::time::Duration;
use willow_core::{Button, Cursor, Document, Input, Modifiers, PAPER, Point, Session};

const T: Duration = Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// A session over a 1000 x 800 canvas with the new document's shape centered.
fn session() -> Session {
    common::session_of(Document::new())
}

// The new shape is 210 wide at screen x 395..605, y 376..424.5 at zoom 1. Edges are grabbed
// away from their midpoints, which win within 10 px.
const RIGHT_EDGE: Point = Point { x: 605.0, y: 385.0 };
const LEFT_EDGE: Point = Point { x: 395.0, y: 385.0 };
const BOTTOM_EDGE: Point = Point { x: 450.0, y: 424.5 };

fn down(s: &mut Session, pos: Point, mods: Modifiers) {
    let button = Button::Left;
    s.handle(Input::PointerDown { pos, button, mods }, T);
}

fn up(s: &mut Session, pos: Point, mods: Modifiers) {
    let button = Button::Left;
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

fn resize_drag(s: &mut Session, from: Point, dx: f32, dy: f32) {
    let mods = Modifiers::default();
    let to = pt(from.x + dx, from.y + dy);
    down(s, from, mods);
    s.handle(Input::PointerMove { pos: to, mods }, T);
    up(s, to, mods);
}

fn shape(s: &Session) -> (i32, u32, u32) {
    let sh = &s.document().shapes[0];
    (sh.x, sh.width, sh.extra_height)
}

#[test]
fn side_drags_resize_symmetrically_in_whole_units_at_several_zooms() {
    for (zoom, d) in [(1.0, 20), (2.0, 10), (0.5, 40)] {
        let mut s = session();
        let mut vp = s.viewport();
        vp.zoom = zoom;
        s.set_viewport(vp);
        let edge = s.viewport().to_screen(pt(210.0, 2.0));
        resize_drag(&mut s, edge, 20.0, 0.0);
        assert_eq!(shape(&s), (-d, 210 + 2 * d as u32, 0), "zoom {zoom}");
    }
    // Dragging the left edge outward (left) grows too.
    let mut s = session();
    resize_drag(&mut s, LEFT_EDGE, -7.0, 0.0);
    assert_eq!(shape(&s), (-7, 224, 0));
}

#[test]
fn bottom_drag_changes_extra_height_only_and_never_below_zero() {
    let mut s = session();
    resize_drag(&mut s, BOTTOM_EDGE, 30.0, 40.0);
    assert_eq!(shape(&s), (0, 210, 40));
    resize_drag(&mut s, pt(450.0, 464.5), 0.0, -100.0);
    assert_eq!(shape(&s), (0, 210, 0));
}

#[test]
fn width_stops_at_padding_plus_widest_cluster() {
    // Widest cluster is a 15 CJK: 2 * 16 + 15 = 47; whole-unit steps from 210 stop at 48.
    let mut s = common::session_of(common::doc_with("ab\u{3042}"));
    let edge = s.viewport().to_screen(pt(210.0, 10.0));
    resize_drag(&mut s, edge, -500.0, 0.0);
    assert_eq!(shape(&s).1, 48);
    // Empty text: one em (15) of room.
    let mut s = session();
    resize_drag(&mut s, RIGHT_EDGE, -500.0, 0.0);
    assert_eq!(shape(&s).1, 48);
}

#[test]
fn narrowing_rewraps_and_grows_the_height() {
    let mut s = common::session_of(common::doc_with("aaaa bbbb cccc"));
    let before = s.shape_height(&s.document().shapes[0]);
    let edge = s.viewport().to_screen(pt(210.0, 10.0));
    resize_drag(&mut s, edge, -80.0, 0.0);
    assert!(s.shape_height(&s.document().shapes[0]) > before);
}

#[test]
fn alt_click_autofits_extra_height_and_keeps_width() {
    let mut s = session();
    resize_drag(&mut s, BOTTOM_EDGE, 0.0, 40.0);
    resize_drag(&mut s, RIGHT_EDGE, 10.0, 0.0);
    let mods = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    let pos = pt(500.0, 400.0);
    down(&mut s, pos, mods);
    up(&mut s, pos, mods);
    assert_eq!(shape(&s), (-10, 230, 0));
}

#[test]
fn cursors_near_edges_are_resize_arrows() {
    let mut s = session();
    let mods = Modifiers::default();
    let mut cursor = |p| s.handle(Input::PointerMove { pos: p, mods }, T).cursor;
    assert_eq!(cursor(pt(603.0, 385.0)), Cursor::ResizeEw);
    assert_eq!(cursor(pt(397.0, 385.0)), Cursor::ResizeEw);
    assert_eq!(cursor(pt(450.0, 423.0)), Cursor::ResizeNs);
    assert_eq!(cursor(pt(500.0, 400.0)), Cursor::Arrow);
    assert_eq!(cursor(pt(700.0, 400.0)), Cursor::Arrow);
}

#[test]
fn each_resize_and_autofit_is_one_undo_step() {
    let mut s = session();
    resize_drag(&mut s, RIGHT_EDGE, 20.0, 0.0);
    resize_drag(&mut s, pt(450.0, 424.5), 0.0, 40.0);
    s.handle(Input::Undo, T);
    assert_eq!(shape(&s), (-20, 250, 0));
    s.handle(Input::Undo, T);
    assert_eq!(shape(&s), (0, 210, 0));
}

#[test]
fn resize_previews_live_and_lands_on_release() {
    let mut s = session();
    down(&mut s, RIGHT_EDGE, Modifiers::default());
    let mods = Modifiers::default();
    s.handle(
        Input::PointerMove {
            pos: pt(625.0, 385.0),
            mods,
        },
        T,
    );
    assert_eq!(s.resize_preview().map(|p| p.width), Some(250));
    assert_eq!(shape(&s), (0, 210, 0));
}

#[test]
fn paper_is_the_theme_under_test() {
    assert_eq!(PAPER.shape.pad_x, 16.0);
}
