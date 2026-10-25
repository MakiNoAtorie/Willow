//! Linking by dragging from a midpoint onto another shape, and removing a link from the
//! connector menu.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, Link, MenuAction, MenuEntry, MenuKind, Modifiers, Point,
    Session, Shape, ShapeId, Side,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Empty shapes (210 x 48.5) at their top-left corners with their colors, and links.
fn doc(shapes: &[(&str, i32, i32, Color)], links: &[(&str, &str)]) -> Document {
    Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y, color)| Shape {
                color,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    }
}

fn color(s: &Session, of: &str) -> Color {
    s.document()
        .shapes
        .iter()
        .find(|x| x.id.0 == of)
        .unwrap()
        .color
}

fn mov(s: &mut Session, canvas: Point) {
    let (pos, mods) = (s.viewport().to_screen(canvas), Modifiers::default());
    s.handle(Input::PointerMove { pos, mods }, T);
}

fn button(s: &mut Session, canvas: Point, down: bool, button: Button) {
    let (pos, mods) = (s.viewport().to_screen(canvas), Modifiers::default());
    let i = if down {
        Input::PointerDown { pos, button, mods }
    } else {
        Input::PointerUp { pos, button, mods }
    };
    s.handle(i, T);
}

/// Presses at canvas point `from`, drags to `to` and leaves the pointer down there.
fn drag_to(s: &mut Session, from: Point, to: Point) {
    mov(s, from);
    button(s, from, true, Button::Left);
    mov(s, pt((from.x + to.x) / 2.0, (from.y + to.y) / 2.0));
    mov(s, to);
}

fn drag(s: &mut Session, from: Point, to: Point) {
    drag_to(s, from, to);
    button(s, to, false, Button::Left);
}

/// a (rose) on its own; b and c (sky) linked below it to the right.
fn two_chains() -> Session {
    session_of(doc(
        &[
            ("a", 0, 0, Color::Rose),
            ("b", 400, 0, Color::Sky),
            ("c", 400, 200, Color::Sky),
        ],
        &[("b", "c")],
    ))
}

/// a's right midpoint.
const A_RIGHT: Point = Point { x: 210.0, y: 24.25 };
const B: Point = Point { x: 500.0, y: 20.0 };

#[test]
fn drag_link_joins_chains_in_the_source_color_as_one_undo_step() {
    let mut s = two_chains();
    drag(&mut s, A_RIGHT, B);
    assert!(s.document().links.contains(&Link(id("a"), id("b"))));
    assert_eq!((color(&s, "b"), color(&s, "c")), (Color::Rose, Color::Rose));
    s.handle(Input::Undo, T);
    assert_eq!(s.document().links, vec![Link(id("b"), id("c"))]);
    assert_eq!((color(&s, "b"), color(&s, "c")), (Color::Sky, Color::Sky));
    // A drag never creates a shape.
    assert_eq!(s.document().shapes.len(), 3);
}

fn target(s: &Session) -> Option<String> {
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T);
    f.link_drag.expect("dragging").target.map(|t| t.0.clone())
}

fn ring(s: &Session, of: &str) -> f32 {
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T);
    f.shapes.iter().find(|v| v.shape.id.0 == of).unwrap().ring
}

#[test]
fn a_band_follows_the_pointer_and_a_valid_target_shows_the_ring() {
    let mut s = two_chains();
    drag_to(&mut s, A_RIGHT, pt(300.0, 100.0));
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T);
    let band = f.link_drag.expect("dragging");
    assert_eq!(
        (band.from, band.to, band.target),
        (A_RIGHT, pt(300.0, 100.0), None)
    );
    drop(f);
    mov(&mut s, B);
    assert_eq!(target(&s).as_deref(), Some("b"));
    assert_eq!(ring(&s, "b"), 1.0);
    // Dropping on empty space cancels.
    mov(&mut s, pt(300.0, 100.0));
    button(&mut s, pt(300.0, 100.0), false, Button::Left);
    assert!(
        s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T)
            .link_drag
            .is_none()
    );
    assert_eq!(s.document().links.len(), 1);
    assert!(!s.can_undo());
}

#[test]
fn self_linked_and_cycle_targets_are_refused() {
    // a - b - c in one chain, below each other.
    let chain = || {
        session_of(doc(
            &[
                ("a", 0, 0, Color::Rose),
                ("b", 0, 200, Color::Rose),
                ("c", 0, 400, Color::Rose),
            ],
            &[("a", "b"), ("b", "c")],
        ))
    };
    for onto in [pt(100.0, 20.0), pt(100.0, 220.0), pt(100.0, 420.0)] {
        let mut s = chain();
        drag_to(&mut s, A_RIGHT, onto);
        assert_eq!(target(&s), None);
        button(&mut s, onto, false, Button::Left);
        assert_eq!(s.document().links.len(), 2);
        assert!(!s.can_undo());
    }
}

#[test]
fn the_link_side_follows_positions_not_the_midpoint_dragged_from() {
    // d sits below a; the drag leaves from a's right midpoint.
    let mut s = session_of(doc(
        &[("a", 0, 0, Color::Rose), ("d", 0, 300, Color::Sky)],
        &[],
    ));
    drag(&mut s, A_RIGHT, pt(100.0, 320.0));
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T);
    let [c] = &f.connectors[..] else {
        panic!("one connector");
    };
    assert_eq!((c.from_side, c.to_side), (Side::Bottom, Side::Top));
    assert_eq!((c.from, c.to), (pt(105.0, 48.5), pt(105.0, 300.0)));
}

fn hovered(s: &Session) -> bool {
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T);
    f.connectors[0].hovered
}

#[test]
fn a_connector_highlights_within_8_px_and_remove_link_keeps_both_colors_undoably() {
    // a above b, rose, linked: the connector runs down x = 105 from y 48.5 to 200.
    let mut s = session_of(doc(
        &[("a", 0, 0, Color::Rose), ("b", 0, 200, Color::Rose)],
        &[("a", "b")],
    ));
    mov(&mut s, pt(120.0, 120.0));
    assert!(!hovered(&s));
    mov(&mut s, pt(112.0, 120.0));
    assert!(hovered(&s));
    button(&mut s, pt(112.0, 120.0), true, Button::Right);
    button(&mut s, pt(112.0, 120.0), false, Button::Right);
    let menu = s.menu().expect("connector menu");
    assert_eq!(menu.kind, MenuKind::Connector);
    let [MenuEntry::Item(item)] = &menu.entries[..] else {
        panic!("one item");
    };
    assert_eq!(
        (item.action, item.label),
        (MenuAction::RemoveLink, "Remove link")
    );
    s.handle(Input::MenuChoose(MenuAction::RemoveLink), T);
    assert!(s.document().links.is_empty());
    assert_eq!((color(&s, "a"), color(&s, "b")), (Color::Rose, Color::Rose));
    s.handle(Input::Undo, T);
    assert_eq!(s.document().links, vec![Link(id("a"), id("b"))]);
}

#[test]
fn losing_focus_ends_a_link_drag() {
    let mut s = two_chains();
    drag_to(&mut s, A_RIGHT, B);
    s.handle(Input::FocusLost, T);
    assert!(
        s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T)
            .link_drag
            .is_none()
    );
    // A new drag from c takes c's own chain: a is a valid target.
    drag(&mut s, pt(400.0, 224.25), pt(100.0, 20.0));
    assert!(s.document().links.contains(&Link(id("c"), id("a"))));
}
