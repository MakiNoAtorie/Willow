//! History steps: grouping, and undo and redo exactness of structural steps.

use super::History;
use crate::{Color, Document, Shape, ShapeId};

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn x(doc: &Document) -> i32 {
    doc.shapes[0].x
}

#[test]
fn a_group_is_one_step_and_undo_inside_it_reverts_the_group_so_far() {
    let (mut doc, mut h) = (Document::new(), History::default());
    let a = doc.shapes[0].id.clone();

    h.begin();
    h.change(&mut doc, &a, |s| s.x = 5);
    h.change(&mut doc, &a, |s| s.x = 9);
    assert!(h.can_undo());

    // Undo while the group is open: back to before the group, still grouping.
    h.undo(&mut doc);
    assert_eq!(x(&doc), 0);
    assert!(!h.can_undo() && h.can_redo());
    h.redo(&mut doc);
    assert_eq!(x(&doc), 9);

    h.change(&mut doc, &a, |s| s.width = 300);
    h.end(&doc);
    h.undo(&mut doc);
    assert_eq!((x(&doc), doc.shapes[0].width), (9, 210));
    h.undo(&mut doc);
    assert_eq!(x(&doc), 0);
    assert!(!h.can_undo());
}

/// a-b linked (rose), c alone (sky), in draw order a, b, c.
fn three() -> Document {
    let mut doc = Document::default();
    for (n, x) in [("a", 0), ("b", 0), ("c", 400)] {
        doc.shapes.push(Shape::new(id(n), x, 0));
    }
    doc.shapes[1].y = 200;
    doc.link(&id("a"), &id("b"));
    doc.recolor(&id("a"), Color::Rose);
    doc.recolor(&id("c"), Color::Sky);
    doc
}

#[test]
fn undo_and_redo_of_a_structural_step_restore_the_document_exactly() {
    let mut doc = three();
    let mut h = History::default();
    let before = doc.clone();

    h.step(&mut doc, |h, doc| {
        h.add(doc, Shape::new(id("d"), 0, 400));
        assert!(h.link(doc, &id("b"), &id("d")));
        h.remove(doc, &id("a"));
        h.recolor(doc, &id("c"), Color::Sage);
        assert!(h.link(doc, &id("c"), &id("b")));
    });
    let after = doc.clone();
    assert_ne!(before, after);
    assert!(!after.shapes.iter().any(|s| s.id == id("a")));
    assert!(!after.links.iter().any(|l| l.0 == id("a") || l.1 == id("a")));
    assert!(after.shapes.iter().all(|s| s.color == Color::Sage));

    for _ in 0..2 {
        h.undo(&mut doc);
        assert_eq!(doc, before);
        h.redo(&mut doc);
        assert_eq!(doc, after);
    }
    h.undo(&mut doc);
    assert!(!h.can_undo());
}

#[test]
fn a_group_mixing_shape_edits_with_a_new_linked_shape_is_one_step() {
    let mut doc = three();
    let mut h = History::default();
    let before = doc.clone();

    h.begin();
    h.change(&mut doc, &id("b"), |s| s.x = 50);
    h.step(&mut doc, |h, doc| {
        h.add(doc, Shape::new(id("d"), 0, 400));
        h.link(doc, &id("b"), &id("d"));
    });
    h.change(&mut doc, &id("d"), |s| s.width = 300);
    h.change(&mut doc, &id("b"), |s| s.y = 250);
    h.end(&doc);
    let after = doc.clone();

    h.undo(&mut doc);
    assert_eq!(doc, before);
    assert!(!h.can_undo());
    h.redo(&mut doc);
    assert_eq!(doc, after);
}
