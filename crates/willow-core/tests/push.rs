//! Insert-and-push: creating a linked shape on a side that already has one relinks the nearest
//! neighbor there to the new shape and pushes that neighbor's subtree clear.
mod common;

use std::time::Duration;

use willow_core::{Color, Document, Input, Link, Point, Session, Shape, ShapeId, Side, Zone};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty rose shapes `(id, x, y)` (210 x 48.5 each), linked as `links`.
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    let doc = Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    };
    common::session_of(doc)
}

fn at(s: &Session, name: &str) -> (i32, i32) {
    let sh = s.document().shapes.iter().find(|x| x.id.0 == name);
    sh.map(|x| (x.x, x.y)).expect(name)
}

fn linked(s: &Session, a: &str, b: &str) -> bool {
    (s.document().links.iter()).any(|Link(p, q)| (p.0 == a && q.0 == b) || (p.0 == b && q.0 == a))
}

#[test]
fn inserting_mid_trunk_relinks_through_the_new_shape_and_pushes_the_subtree_one_slot() {
    // a, then b one gap away (48.5 tall + 48 gap = 97), then c. New shape "d" lands where b
    // was; b must sit a gap below its bottom (97 + 48.5 + 48 = 193.5, rounded up to 194).
    for (side, b, c, pushed) in [
        (Side::Bottom, (0, 97), (0, 194), (0, 97)),
        (Side::Top, (0, -97), (0, -194), (0, -97)),
        (Side::Right, (258, 0), (516, 0), (258, 0)),
        (Side::Left, (-258, 0), (-516, 0), (-258, 0)),
    ] {
        let mut s = session(
            &[("a", 0, 0), ("b", b.0, b.1), ("c", c.0, c.1)],
            &[("a", "b"), ("b", "c")],
        );
        s.create_linked(&id("a"), side);
        assert!(linked(&s, "a", "d"), "{side:?}");
        assert!(linked(&s, "d", "b"), "{side:?}");
        assert!(!linked(&s, "a", "b"), "{side:?}");
        assert!(linked(&s, "b", "c"), "{side:?}");
        assert_eq!(at(&s, "d"), b, "{side:?}");
        assert_eq!(at(&s, "b"), (b.0 + pushed.0, b.1 + pushed.1), "{side:?}");
        assert_eq!(at(&s, "c"), (c.0 + pushed.0, c.1 + pushed.1), "{side:?}");
        assert_eq!(at(&s, "a"), (0, 0), "{side:?}");
    }
}

#[test]
fn with_several_on_that_side_only_the_nearest_is_relinked_and_pushed() {
    // Below a: b (nearest, with child x) and c farther down-right; e sits on a's right.
    let mut s = session(
        &[
            ("a", 0, 0),
            ("b", -60, 97),
            ("c", 100, 400),
            ("e", 258, 0),
            ("x", -60, 194),
        ],
        &[("a", "b"), ("a", "c"), ("a", "e"), ("b", "x")],
    );
    s.create_linked(&id("a"), Side::Bottom);
    // The new shape "d" is at (0, 97); b goes to 194, a gap below its bottom (145.5).
    assert_eq!(at(&s, "d"), (0, 97));
    assert!(linked(&s, "d", "b") && !linked(&s, "a", "b"));
    assert_eq!((at(&s, "b"), at(&s, "x")), ((-60, 194), (-60, 291)));
    assert!(linked(&s, "a", "c") && linked(&s, "a", "e"));
    assert_eq!((at(&s, "c"), at(&s, "e")), ((100, 400), (258, 0)));
}

#[test]
fn a_distant_neighbor_is_relinked_but_not_moved() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 400)], &[("a", "b")]);
    s.create_linked(&id("a"), Side::Bottom);
    assert!(linked(&s, "a", "c") && linked(&s, "c", "b") && !linked(&s, "a", "b"));
    assert_eq!(at(&s, "b"), (0, 400));
    // Nothing moved, so nothing slides.
    assert!(!s.animating(Duration::from_millis(170)));
}

#[test]
fn one_undo_restores_links_and_positions_and_zones_stay() {
    let mut doc = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194)],
        &[("a", "b"), ("b", "c")],
    )
    .document()
    .clone();
    doc.zones.push(Zone {
        id: id("z"),
        title: "Z".into(),
        color: Color::Sky,
        x: -20,
        y: 80,
        width: 300,
        height: 200,
        members: vec![id("b"), id("c")],
    });
    let mut s = common::session_of(doc.clone());
    s.create_linked(&id("a"), Side::Bottom);
    let text = "Hi".to_string();
    s.handle(
        Input::Text {
            text,
            replace: None,
        },
        T,
    );
    // The push moved c out of the zone's rectangle; membership is unchanged.
    assert_eq!(at(&s, "c"), (0, 291));
    assert_eq!(s.document().zones[0].members, [id("b"), id("c")]);
    s.handle(Input::Escape, T);
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), doc);
    assert!(!s.can_undo());
}

#[test]
fn the_pushed_subtree_slides_from_its_old_place_over_220_ms() {
    let mut s = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194)],
        &[("a", "b"), ("b", "c")],
    );
    s.create_linked(&id("a"), Side::Bottom);
    let slide = |ms: u64, name: &str| {
        let (from, to) = (
            Point { x: 0.0, y: 0.0 },
            Point {
                x: 1000.0,
                y: 800.0,
            },
        );
        let f = s.draw(from, to, Duration::from_millis(ms));
        let v = f.shapes.iter().find(|v| v.shape.id.0 == name).expect(name);
        (v.slide.x, v.slide.y)
    };
    // Drawn where they were (97 units up), easing down to their saved place.
    assert_eq!((slide(0, "b"), slide(0, "c")), ((0.0, -97.0), (0.0, -97.0)));
    let mid = slide(110, "b").1;
    assert!(-97.0 < mid && mid < 0.0, "{mid}");
    assert!(s.animating(Duration::from_millis(200)));
    assert_eq!(slide(220, "b"), (0.0, 0.0));
    assert!(!s.animating(Duration::from_millis(220)));
    // The new shape and the source don't slide.
    assert_eq!((slide(0, "a"), slide(0, "d")), ((0.0, 0.0), (0.0, 0.0)));
}
