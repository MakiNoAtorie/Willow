//! Links, chains, chain color and zones in the core model.

use willow_core::{Color, Document, Point, Shape, ShapeId, Side, Zone, side};

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Unlinked neutral shapes with these IDs.
fn doc_of(ids: &[&str]) -> Document {
    let mut doc = Document::new();
    doc.shapes = ids.iter().map(|i| Shape::new(id(i), 0, 0)).collect();
    doc
}

#[test]
fn a_link_that_would_close_a_cycle_is_refused() {
    let mut doc = doc_of(&["a", "b", "c"]);
    assert!(doc.link(&id("a"), &id("b")));
    assert!(doc.link(&id("b"), &id("c")));

    assert!(!doc.link(&id("c"), &id("a")), "a-b-c-a is a cycle");
    assert!(
        !doc.link(&id("b"), &id("a")),
        "a second a-b link is a cycle"
    );
    assert!(
        !doc.link(&id("a"), &id("a")),
        "a shape can't link to itself"
    );
    assert!(!doc.link(&id("a"), &id("zz")), "no such shape");
    assert_eq!(doc.links.len(), 2);
}

fn zone(i: &str, members: &[&str]) -> Zone {
    Zone {
        id: id(i),
        title: "Act I".into(),
        color: Color::Neutral,
        x: 0,
        y: 0,
        width: 300,
        height: 200,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

#[test]
fn shapes_and_zones_share_one_id_generator_and_deleted_ids_come_back() {
    let mut doc = doc_of(&["a", "b"]);
    doc.zones.push(zone("c", &["a"]));
    assert_eq!(doc.next_id(), id("d"), "c is a zone's");

    doc.shapes.push(Shape::new(doc.next_id(), 0, 0));
    doc.shapes.retain(|s| s.id != id("b"));
    assert_eq!(doc.next_id(), id("b"), "a deleted shape's ID may be reused");
    doc.zones.clear();
    doc.shapes.push(Shape::new(id("b"), 0, 0));
    assert_eq!(doc.next_id(), id("c"), "so may a deleted zone's");
}

#[test]
fn the_side_rule_compares_centers_and_an_exact_diagonal_is_vertical() {
    let p = |x, y| Point { x, y };
    let o = p(0.0, 0.0);
    assert_eq!(side(o, p(10.0, 50.0)), Side::Bottom);
    assert_eq!(side(o, p(-10.0, -50.0)), Side::Top);
    assert_eq!(side(o, p(50.0, 10.0)), Side::Right);
    assert_eq!(side(o, p(-50.0, -10.0)), Side::Left);
    assert_eq!(side(o, p(30.0, 30.0)), Side::Bottom);
    assert_eq!(side(o, p(30.0, -30.0)), Side::Top);
    assert_eq!(side(p(30.0, 30.0), o), Side::Top);
}

fn color(doc: &Document, i: &str) -> Color {
    doc.shapes.iter().find(|s| s.id.0 == i).unwrap().color
}

#[test]
fn a_join_repaints_the_adopted_chain_and_a_split_keeps_colors() {
    // Chains a-b (sage) and c-d (rose).
    let mut doc = doc_of(&["a", "b", "c", "d"]);
    doc.link(&id("a"), &id("b"));
    doc.link(&id("c"), &id("d"));
    doc.recolor(&id("b"), Color::Sage);
    doc.recolor(&id("d"), Color::Rose);
    assert_eq!(
        color(&doc, "a"),
        Color::Sage,
        "recolor paints the whole chain"
    );
    assert_eq!(color(&doc, "c"), Color::Rose);

    // Joining from b: c-d adopts sage.
    assert!(doc.link(&id("b"), &id("d")));
    for i in ["a", "b", "c", "d"] {
        assert_eq!(color(&doc, i), Color::Sage, "{i}");
    }

    // Splitting into a-b and c-d again keeps sage on both halves.
    assert!(doc.unlink(&id("d"), &id("b")));
    assert!(!doc.unlink(&id("d"), &id("b")), "already gone");
    assert_eq!(doc.chain(&id("a")).len(), 2);
    for i in ["a", "b", "c", "d"] {
        assert_eq!(color(&doc, i), Color::Sage, "{i}");
    }
}
