//! New shape beside the chain (Ctrl/Cmd+Shift+N): a new branch one creation gap right of the
//! selected shape's chain, top-aligned with the selection. Paper's creation gap is 48.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{Color, Document, Input, Link, Point, Session, Shape, ShapeId, Zone};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn session(at: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    session_of(Document {
        shapes: (at.iter())
            .map(|&(n, x, y)| Shape::new(id(n), x, y))
            .collect(),
        links: (links.iter()).map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    })
}

/// The newest shape, after checking exactly one was added to `before`.
fn created(s: &Session, before: usize) -> Shape {
    assert_eq!(s.document().shapes.len(), before + 1);
    s.document().shapes.last().unwrap().clone()
}

#[test]
fn it_goes_a_gap_right_of_the_chain_top_aligned_with_the_selection() {
    // The chain's right edge is b's (x 100 + width), not a's.
    let mut s = session(
        &[("a", 0, 0), ("b", 100, 300), ("z", 900, 900)],
        &[("a", "b")],
    );
    let w = s.document().shapes[0].width as i32;
    s.select([id("a")]);
    s.handle(Input::NewBeside, T);
    let n = created(&s, 3);
    assert_eq!((n.x, n.y), (100 + w + 48, 0));
    assert_eq!(n.color, Color::Neutral);
    assert_eq!(s.selection(), [n.id]);
    assert!(s.editing().is_some());
}

#[test]
fn with_nothing_selected_it_is_centered_in_the_view() {
    let mut s = session(&[("a", 0, 0)], &[]);
    s.select([]);
    let c = s.viewport().to_canvas(Point { x: 500.0, y: 400.0 });
    s.handle(Input::NewBeside, T);
    let n = created(&s, 1);
    let h = s.shape_height(&n);
    assert_eq!(n.x, (c.x - n.width as f32 / 2.0).round() as i32);
    assert_eq!(n.y, (c.y - h / 2.0).round() as i32);
}

#[test]
fn a_multi_selection_places_from_its_first_shape_in_story_order() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 300)], &[]);
    let w = s.document().shapes[0].width as i32;
    s.select([id("b"), id("a")]);
    s.handle(Input::NewBeside, T);
    let n = created(&s, 2);
    assert_eq!((n.x, n.y), (w + 48, 0));
}

#[test]
fn it_joins_the_innermost_zone_and_pushes_a_chain_it_lands_on() {
    let mut doc = Document {
        shapes: vec![Shape::new(id("a"), 0, 0), Shape::new(id("p"), 300, 0)],
        ..Document::default()
    };
    let w = doc.shapes[0].width as i32;
    doc.zones.push(Zone {
        id: id("zz"),
        title: String::new(),
        color: Color::Neutral,
        x: w,
        y: -20,
        width: 1000,
        height: 400,
        members: vec![id("p")],
    });
    let mut s = session_of(doc);
    s.select([id("a")]);
    s.handle(Input::NewBeside, T);
    let n = created(&s, 2);
    assert_eq!(s.document().zones[0].members.last(), Some(&n.id));
    let p = &s.document().shapes[1];
    assert!(p.x >= n.x + n.width as i32, "p {} cleared new {}", p.x, n.x);
}

#[test]
fn one_undo_removes_it_and_its_text() {
    let mut s = session(&[("a", 0, 0)], &[]);
    s.select([id("a")]);
    s.handle(Input::NewBeside, T);
    s.handle(
        Input::Text {
            text: "hi".into(),
            replace: None,
        },
        T,
    );
    s.handle(Input::Undo, T);
    assert_eq!(s.document().shapes.len(), 1);
}

#[test]
fn it_does_nothing_while_editing() {
    let mut s = session(&[("a", 0, 0)], &[]);
    s.select([id("a")]);
    s.handle(Input::StartEditing, T);
    s.handle(Input::NewBeside, T);
    assert_eq!(s.document().shapes.len(), 1);
}
