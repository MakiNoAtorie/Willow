//! Arrow-key navigation: links on that side first, else the nearest shape that way; a
//! multi-selection collapses first; the view follows an off-screen target.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{Document, Input, Link, Session, Shape, ShapeId, Side};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty shapes at the given top-lefts with the given links, `a` selected.
fn shapes(at: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    let mut s = session_of(Document {
        shapes: (at.iter())
            .map(|&(n, x, y)| Shape::new(id(n), x, y))
            .collect(),
        links: (links.iter()).map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    });
    s.select([id("a")]);
    s
}

fn nav(s: &mut Session, side: Side) {
    s.handle(Input::Navigate(side), T);
}

#[test]
fn a_link_on_that_side_wins_over_a_nearer_unlinked_shape() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 100), ("c", 0, 300)], &[("a", "c")]);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("c")]);
}

#[test]
fn of_several_links_that_way_the_nearest_wins_ties_by_id() {
    let at = [("a", 0, 0), ("c", 0, 500), ("d", 0, 200)];
    let mut s = shapes(&at, &[("a", "c"), ("a", "d")]);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("d")]);
    let at = [("a", 0, 0), ("aa", -300, 300), ("b", 300, 300)];
    let mut s = shapes(&at, &[("a", "aa"), ("a", "b")]);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("b")], "b sorts before aa");
}

#[test]
fn without_a_link_the_nearest_shape_that_way_wins() {
    // b is nearer but right of a; the exact diagonal d counts as below.
    let at = [
        ("a", 0, 0),
        ("b", 150, 20),
        ("c", 0, 500),
        ("d", 300, 300),
        ("e", 0, -100),
    ];
    let mut s = shapes(&at, &[("c", "e")]);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("d")]);
}

#[test]
fn with_nothing_that_way_the_selection_stays() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 300)], &[]);
    nav(&mut s, Side::Top);
    assert_eq!(s.selection(), [id("a")]);
}

#[test]
fn a_multi_selection_collapses_to_its_first_shape_in_story_order_then_moves() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 300), ("d", 0, 600)], &[]);
    s.select([id("b"), id("a")]);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("b")]);
}

#[test]
fn arrows_do_nothing_while_editing_and_are_no_undo_step() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 300)], &[]);
    s.handle(Input::StartEditing, T);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("a")]);
    assert!(s.editing().is_some());
    s.handle(Input::Escape, T);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.selection(), [id("b")]);
    assert!(!s.can_undo());
}

#[test]
fn an_off_screen_target_pans_the_view_just_enough_eased_without_zooming() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 2000)], &[]);
    let (before, margin) = (s.viewport(), s.theme().follow_margin);
    nav(&mut s, Side::Bottom);
    assert_eq!(s.viewport(), before, "eased, not a jump");
    s.tick(Duration::from_millis(60));
    let mid = s.viewport().origin.y;
    assert!(before.origin.y < mid, "partway at 60 ms: {mid}");
    s.tick(Duration::from_millis(120));
    let vp = s.viewport();
    let b = &s.document().shapes[1];
    let bottom = vp.to_screen(willow_core::Point {
        x: 0.0,
        y: b.y as f32 + s.shape_height(b),
    });
    let gap = 800.0 - bottom.y;
    assert!(
        (gap - margin).abs() < 0.01,
        "bottom edge a margin above the screen's: {gap}"
    );
    assert!(mid < vp.origin.y);
    assert_eq!((vp.origin.x, vp.zoom), (before.origin.x, before.zoom));
    assert!(!s.tick(Duration::from_millis(500)).animating);
}

#[test]
fn an_on_screen_target_does_not_pan() {
    let mut s = shapes(&[("a", 0, 0), ("b", 0, 300)], &[]);
    let before = s.viewport();
    nav(&mut s, Side::Bottom);
    s.tick(Duration::from_millis(500));
    assert_eq!(s.viewport(), before);
}
