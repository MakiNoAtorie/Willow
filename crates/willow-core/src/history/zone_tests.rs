//! Zone steps: undo and redo exactness, grouping, and member-list consistency.

use super::History;
use crate::{Color, Document, Shape, ShapeId, Zone};

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(name: &str, x: i32, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: name.into(),
        color: Color::Neutral,
        x,
        y: 0,
        width: 300,
        height: 300,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

/// Shapes a, b, c, d; zone p holds a and the nested zone q, which holds b. c and d are loose.
fn nested() -> Document {
    let mut doc = Document::default();
    for (i, n) in ["a", "b", "c", "d"].into_iter().enumerate() {
        doc.shapes.push(Shape::new(id(n), 300 * i as i32, 0));
    }
    doc.zones = vec![zone("p", 0, &["a", "q"]), zone("q", 280, &["b"])];
    doc
}

fn ids(doc: &Document) -> Vec<&str> {
    doc.zones.iter().map(|z| z.id.0.as_str()).collect()
}

/// Every member exists, nothing is in two zones, and parents draw before their children.
fn assert_consistent(doc: &Document, case: &str) {
    let mut seen = std::collections::HashSet::new();
    for (i, z) in doc.zones.iter().enumerate() {
        for m in &z.members {
            let shape = doc.shapes.iter().any(|s| &s.id == m);
            let child = doc.zones.iter().position(|c| &c.id == m);
            assert!(
                shape || child.is_some(),
                "{case}: {} lists missing {}",
                z.id.0,
                m.0
            );
            assert!(
                child.is_none_or(|c| c > i),
                "{case}: {} drawn before parent",
                m.0
            );
            assert!(seen.insert(m.clone()), "{case}: {} in two zones", m.0);
        }
    }
}

#[test]
fn adding_a_zone_takes_its_members_from_their_old_zone_and_undoes_exactly() {
    let (mut doc, mut h) = (nested(), History::default());
    let before = doc.clone();

    h.add_zone(&mut doc, zone("r", 600, &["c", "b"]));
    let after = doc.clone();
    assert_eq!(ids(&doc), ["p", "q", "r"]);
    assert!(doc.zones[1].members.is_empty());
    assert_consistent(&doc, "added");

    for _ in 0..2 {
        h.undo(&mut doc);
        assert_eq!(doc, before);
        h.redo(&mut doc);
        assert_eq!(doc, after);
    }
}

#[test]
fn removing_a_zone_leaves_no_dangling_member_or_parent_references() {
    let (mut doc, mut h) = (nested(), History::default());
    let before = doc.clone();

    // The nested zone: its parent stops listing it; its shape is loose.
    assert_eq!(
        h.remove_zone(&mut doc, &id("q")),
        Some(before.zones[1].clone())
    );
    assert_eq!(ids(&doc), ["p"]);
    assert_eq!(doc.zones[0].members, [id("a")]);
    assert_consistent(&doc, "nested removed");

    // The top-level zone, holding a nested one: that becomes top-level, keeping its members.
    h.undo(&mut doc);
    assert_eq!(doc, before);
    h.remove_zone(&mut doc, &id("p"));
    assert_eq!(ids(&doc), ["q"]);
    assert_eq!(doc.zones[0].members, [id("b")]);
    assert_consistent(&doc, "parent removed");
    let after = doc.clone();

    h.undo(&mut doc);
    assert_eq!(doc, before);
    h.redo(&mut doc);
    assert_eq!(doc, after);
    assert_eq!(h.remove_zone(&mut doc, &id("p")), None);
}

#[test]
fn changing_a_zone_undoes_and_redoes_exactly() {
    let (mut doc, mut h) = (nested(), History::default());
    let before = doc.clone();

    h.change_zone(&mut doc, &id("q"), |z| {
        (z.x, z.y, z.width, z.height) = (10, 20, 400, 500);
        z.title = "Act two".into();
        z.color = Color::Sky;
    });
    let after = doc.clone();
    assert_eq!((doc.zones[1].x, doc.zones[1].height), (10, 500));

    h.undo(&mut doc);
    assert_eq!(doc, before);
    h.redo(&mut doc);
    assert_eq!(doc, after);

    // A change that leaves the zone as it was adds nothing.
    h.undo(&mut doc);
    h.change_zone(&mut doc, &id("q"), |z| z.x = 280);
    assert!(!h.can_undo() && h.can_redo());
}

#[test]
fn several_zone_and_shape_changes_in_one_step_undo_as_one() {
    let (mut doc, mut h) = (nested(), History::default());
    let before = doc.clone();

    h.begin();
    h.change_zone(&mut doc, &id("p"), |z| z.x = 50);
    h.change(&mut doc, &id("a"), |s| s.x = 50);
    h.step(&mut doc, |h, doc| {
        h.add_zone(doc, zone("r", 900, &["c", "d"]));
        h.join_zone(doc, &id("p"), &id("r"));
        h.remove_zone(doc, &id("q"));
    });
    h.change_zone(&mut doc, &id("r"), |z| z.title = "New".into());
    h.change_zone(&mut doc, &id("p"), |z| z.width = 900);
    h.remove(&mut doc, &id("c"));
    h.end(&doc);
    let after = doc.clone();
    assert_eq!(ids(&doc), ["p", "r"]);
    assert_eq!(doc.zones[1].members, [id("d")]);
    assert_consistent(&doc, "one step");

    for _ in 0..2 {
        h.undo(&mut doc);
        assert_eq!(doc, before);
        assert!(!h.can_undo());
        h.redo(&mut doc);
        assert_eq!(doc, after);
    }

    // A group that changes a zone and changes it back adds no step, but keeps a join made
    // in between.
    h.begin();
    h.change_zone(&mut doc, &id("r"), |z| z.title = "Newer".into());
    h.change_zone(&mut doc, &id("r"), |z| z.title = "New".into());
    h.end(&doc);
    h.begin();
    h.change_zone(&mut doc, &id("r"), |z| z.title = "Newer".into());
    h.join_zone(&mut doc, &id("r"), &id("b"));
    h.change_zone(&mut doc, &id("r"), |z| z.title = "New".into());
    h.end(&doc);
    h.undo(&mut doc);
    assert_eq!(doc, after);
    h.undo(&mut doc);
    assert_eq!(doc, before);
}

#[test]
fn setting_the_draw_order_keeps_parents_first_and_undoes_exactly() {
    let (mut doc, mut h) = (nested(), History::default());
    doc.zones.push(zone("s", 900, &["c"]));
    let before = doc.clone();

    // q asked for below its parent: it draws right above p instead.
    h.order_zones(&mut doc, &[id("q"), id("s"), id("p")]);
    let after = doc.clone();
    assert_eq!(ids(&doc), ["s", "p", "q"]);
    assert_consistent(&doc, "ordered");
    assert_eq!(doc.zones[2], before.zones[1]);

    for _ in 0..2 {
        h.undo(&mut doc);
        assert_eq!(doc, before);
        h.redo(&mut doc);
        assert_eq!(doc, after);
    }

    // The order it already has adds nothing.
    h.undo(&mut doc);
    h.order_zones(&mut doc, &[id("p"), id("q"), id("s")]);
    assert!(!h.can_undo() && h.can_redo());
}
