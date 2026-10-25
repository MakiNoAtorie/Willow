//! Multi-selection: the Shift+drag marquee, Shift+click toggling, click collapse and clearing,
//! and dragging the whole selection as one undo step.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{Button, Document, Input, Modifiers, Point, Session, Shape, ShapeId};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty shapes a (0,0), b (400,0), c (0,400), d (400,400), unlinked.
fn four() -> Session {
    let at = [("a", 0, 0), ("b", 400, 0), ("c", 0, 400), ("d", 400, 400)];
    session_of(Document {
        shapes: (at.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        ..Document::default()
    })
}

/// Screen point of canvas point (x, y).
fn at(s: &Session, x: f32, y: f32) -> Point {
    s.viewport().to_screen(Point { x, y })
}

fn mods(shift: bool) -> Modifiers {
    Modifiers {
        shift,
        ..Modifiers::default()
    }
}

fn down(s: &mut Session, pos: Point, shift: bool) {
    let (button, mods) = (Button::Left, mods(shift));
    s.handle(Input::PointerDown { pos, button, mods }, T);
}

fn mv(s: &mut Session, pos: Point, shift: bool) {
    s.handle(
        Input::PointerMove {
            pos,
            mods: mods(shift),
        },
        T,
    );
}

fn up(s: &mut Session, pos: Point, shift: bool) {
    let (button, mods) = (Button::Left, mods(shift));
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

/// A click at canvas point (x, y).
fn click(s: &mut Session, x: f32, y: f32, shift: bool) {
    let p = at(s, x, y);
    down(s, p, shift);
    up(s, p, shift);
}

fn selected(s: &Session) -> Vec<&str> {
    s.selection().iter().map(|i| i.0.as_str()).collect()
}

#[test]
fn marquee_adds_touched_shapes_to_the_selection_live() {
    let mut s = four();
    click(&mut s, 10.0, 10.0, false);
    assert_eq!(selected(&s), ["a"]);

    let from = at(&s, -50.0, 350.0);
    down(&mut s, from, true);
    // Touches c's top edge only.
    let p = at(&s, 20.0, 410.0);
    mv(&mut s, p, true);
    assert_eq!(selected(&s), ["a", "c"]);
    let m = s.draw(Point::default(), at(&s, 2000.0, 2000.0), T).marquee;
    assert_eq!(
        m,
        Some((Point { x: -50.0, y: 350.0 }, Point { x: 20.0, y: 410.0 }))
    );

    let p = at(&s, 450.0, 410.0);
    mv(&mut s, p, true);
    assert_eq!(selected(&s), ["a", "c", "d"]);
    // Shrinking back lets d go again.
    let p = at(&s, 20.0, 410.0);
    mv(&mut s, p, true);
    assert_eq!(selected(&s), ["a", "c"]);

    let p = at(&s, 450.0, 410.0);
    mv(&mut s, p, true);
    up(&mut s, p, true);
    assert_eq!(selected(&s), ["a", "c", "d"]);
    let m = s.draw(Point::default(), at(&s, 2000.0, 2000.0), T).marquee;
    assert_eq!(m, None);
    assert!(!s.can_undo());
}

#[test]
fn shift_click_toggles_plain_click_collapses_and_empty_click_clears() {
    let mut s = four();
    click(&mut s, 10.0, 10.0, false);
    click(&mut s, 410.0, 10.0, true);
    click(&mut s, 10.0, 410.0, true);
    assert_eq!(selected(&s), ["a", "b", "c"]);
    click(&mut s, 410.0, 10.0, true);
    assert_eq!(selected(&s), ["a", "c"]);

    // A plain click in the multi-selection selects only that shape; the next one edits it.
    click(&mut s, 10.0, 410.0, false);
    assert_eq!(selected(&s), ["c"]);
    assert!(s.editing().is_none());
    click(&mut s, 10.0, 410.0, false);
    assert!(s.editing().is_some_and(|e| e.id == id("c")));

    // Shift+click while editing leaves editing and adds the shape.
    click(&mut s, 410.0, 410.0, true);
    assert!(s.editing().is_none());
    assert_eq!(selected(&s), ["c", "d"]);

    click(&mut s, 300.0, 300.0, false);
    assert!(s.selection().is_empty());
}

/// Every shape's top-left, in document order.
fn spots(s: &Session) -> Vec<(i32, i32)> {
    s.document().shapes.iter().map(|s| (s.x, s.y)).collect()
}

#[test]
fn dragging_a_selected_shape_moves_the_selection_as_one_step_pushing_nothing() {
    let mut s = four();
    click(&mut s, 10.0, 10.0, false);
    click(&mut s, 410.0, 10.0, true);
    click(&mut s, 10.0, 410.0, true);
    let before = s.document().clone();

    // Drag b onto d: d is overlapped, not pushed.
    let (from, to) = (at(&s, 410.0, 10.0), at(&s, 410.0, 400.0));
    down(&mut s, from, false);
    mv(&mut s, to, false);
    let frame = s.draw(Point::default(), at(&s, 2000.0, 2000.0), T);
    let drawn: Vec<(i32, i32)> = frame.shapes.iter().map(|v| (v.x, v.y)).collect();
    assert_eq!(drawn, [(0, 390), (400, 390), (0, 790), (400, 400)]);
    assert_eq!(s.document(), &before);
    up(&mut s, to, false);

    assert_eq!(spots(&s), [(0, 390), (400, 390), (0, 790), (400, 400)]);
    assert_eq!(selected(&s), ["a", "b", "c"]);
    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn deleting_or_undoing_mid_marquee_never_brings_deleted_shapes_back() {
    let mut s = four();
    click(&mut s, 10.0, 10.0, false);
    let from = at(&s, -50.0, 350.0);
    down(&mut s, from, true);
    s.handle(Input::DeleteShapes, T);
    assert!(s.selection().is_empty());
    let p = at(&s, 20.0, 410.0);
    mv(&mut s, p, true);
    assert!(s.selection().is_empty(), "{:?}", selected(&s));
    up(&mut s, p, true);

    // Undo mid-marquee: a duplicate of b goes away and stays away.
    click(&mut s, 410.0, 10.0, false);
    s.handle(Input::Duplicate, T);
    let copy = s.selection().to_vec();
    let from = at(&s, -50.0, 350.0);
    down(&mut s, from, true);
    s.handle(Input::Undo, T);
    mv(&mut s, p, true);
    let ids: Vec<&ShapeId> = s.document().shapes.iter().map(|x| &x.id).collect();
    assert!(
        s.selection().iter().all(|i| ids.contains(&i)),
        "{:?}",
        selected(&s)
    );
    assert!(!copy.iter().any(|c| s.selection().contains(c)));
}
