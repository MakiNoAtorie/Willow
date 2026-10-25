//! Enter, Esc and Shift+Enter.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Document, Input, Session, Shape, ShapeId};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn pan_right(s: &mut Session) {
    let (pos, mods) = (Default::default(), Default::default());
    let delta = willow_core::Point { x: -1000.0, y: 0.0 };
    s.handle(Input::Wheel { pos, delta, mods }, T);
}

fn session(shapes: &[(&str, i32, i32)]) -> Session {
    let doc = Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        ..Document::default()
    };
    session_of(doc)
}

/// The id of the document's first shape.
fn first(s: &Session) -> ShapeId {
    s.document().shapes[0].id.clone()
}

#[test]
fn enter_with_nothing_selected_selects_the_shape_nearest_the_view_center() {
    // The first shape starts centered, at view center (105, 24); pan 1000 units right.
    let mut s = session(&[("a", 0, 0), ("b", 1000, 40), ("c", 1400, 300)]);
    pan_right(&mut s);
    s.handle(Input::Enter, T);
    assert_eq!(s.selection(), [id("b")]);
    assert!(s.editing().is_none());
}

#[test]
fn enter_ties_go_to_the_first_id_and_an_empty_canvas_does_nothing() {
    let mut s = session(&[("b", 0, 0), ("a", 2000, 0)]);
    pan_right(&mut s);
    s.handle(Input::Enter, T);
    assert_eq!(s.selection(), [id("a")]);
    let mut s = session(&[]);
    assert!(!s.handle(Input::Enter, T).redraw);
    assert!(s.selection().is_empty());
}

#[test]
fn enter_edits_the_selected_shape_with_the_caret_at_the_end_and_again_leaves() {
    let mut s = session_of(doc_with("hello"));
    s.select([first(&s)]);
    s.handle(Input::Enter, T);
    assert_eq!(s.editing().unwrap().caret.offset, 5);
    s.handle(Input::Enter, T);
    assert!(s.editing().is_none());
    assert_eq!(s.selection().len(), 1);
}

#[test]
fn shift_enter_inserts_a_line_break_and_the_shape_grows_and_pushes() {
    let mut s = session(&[("a", 0, 0), ("b", 395, 473)]);
    s.select([id("a")]);
    s.handle(Input::Enter, T);
    let before = s.shape_height(&s.document().shapes[0]);
    s.handle(Input::NewLine, T);
    s.handle(Input::NewLine, T);
    s.handle(Input::Enter, T);
    let a = &s.document().shapes[0];
    assert_eq!(a.text.plain(), "\n\n");
    let grown = s.shape_height(a);
    assert!(grown > before, "{grown} {before}");
    assert!(s.document().shapes[1].y as f32 >= grown + 48.0);
}

#[test]
fn shift_enter_outside_editing_does_nothing() {
    let mut s = session_of(doc_with("x"));
    s.select([first(&s)]);
    assert!(!s.handle(Input::NewLine, T).redraw);
    assert_eq!(s.document().shapes[0].text.plain(), "x");
}

#[test]
fn esc_closes_a_menu_before_deselecting() {
    let mut s = session_of(doc_with("x"));
    let (pos, mods) = (
        willow_core::Point { x: 500.0, y: 400.0 },
        Default::default(),
    );
    let button = willow_core::Button::Right;
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    assert!(s.menu().is_some());
    s.handle(Input::Escape, T);
    assert!(s.menu().is_none());
    assert_eq!(s.selection().len(), 1);
    s.handle(Input::Escape, T);
    assert!(s.selection().is_empty());
}

#[test]
fn esc_leaves_editing_then_deselects() {
    let mut s = session_of(doc_with("x"));
    s.select([first(&s)]);
    s.handle(Input::Enter, T);
    s.handle(Input::Escape, T);
    assert!(s.editing().is_none());
    assert_eq!(s.selection().len(), 1);
    s.handle(Input::Escape, T);
    assert!(s.selection().is_empty());
}
