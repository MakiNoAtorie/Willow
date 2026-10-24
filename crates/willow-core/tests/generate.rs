use std::collections::{HashMap, HashSet};
use willow_core::{Color, Document, Link, ShapeId, generate};

#[test]
fn the_same_seed_generates_the_same_document() {
    assert_eq!(generate(500, 7), generate(500, 7));
    assert_ne!(generate(500, 7), generate(500, 8));
}

#[test]
fn a_generated_document_has_n_shapes_in_all_seven_colors_with_bold_and_italic() {
    let doc = generate(1000, 1);
    assert_eq!(doc.shapes.len(), 1000);
    let colors: HashSet<Color> = doc.shapes.iter().map(|s| s.color).collect();
    assert_eq!(colors.len(), 7);
    let runs = || doc.shapes.iter().flat_map(|s| &s.text.paragraphs[0].runs);
    assert!(runs().any(|r| r.bold) && runs().any(|r| r.italic));
    assert!(runs().any(|r| !r.bold && !r.italic));
    let ids: HashSet<_> = doc.shapes.iter().map(|s| &s.id).collect();
    assert_eq!(ids.len(), 1000);
}

#[test]
fn generated_chains_are_one_colored_trees_some_linked_sideways() {
    let doc = generate(1000, 2);
    let shape = |id: &ShapeId| doc.shapes.iter().find(|s| &s.id == id).expect("link end");
    assert!(doc.links.len() > 500);
    for Link(a, b) in &doc.links {
        assert_eq!(shape(a).color, shape(b).color, "{a:?}-{b:?}");
    }
    assert!((doc.links.iter()).any(|Link(a, b)| shape(a).x != shape(b).x));
    // Relinking every link from scratch never closes a cycle.
    let mut fresh = Document {
        shapes: doc.shapes.clone(),
        ..Document::default()
    };
    for Link(a, b) in &doc.links {
        assert!(fresh.link(a, b), "{a:?}-{b:?}");
    }
}

#[test]
fn generated_zones_nest_one_level_and_chains_span_them() {
    let doc = generate(1000, 2);
    let shape_ids: HashSet<_> = doc.shapes.iter().map(|s| &s.id).collect();
    let zone_ids: HashSet<_> = doc.zones.iter().map(|z| &z.id).collect();
    assert_eq!(zone_ids.len(), doc.zones.len());
    assert!(zone_ids.is_disjoint(&shape_ids), "one ID namespace");
    // The innermost zone of every member; nobody is listed twice.
    let mut parent = HashMap::new();
    for z in &doc.zones {
        for m in &z.members {
            assert!(shape_ids.contains(m) || zone_ids.contains(m), "{m:?}");
            assert!(parent.insert(m, &z.id).is_none(), "{m:?} listed twice");
        }
    }
    let nested: Vec<_> = (doc.zones.iter())
        .filter(|z| parent.contains_key(&z.id))
        .collect();
    assert!(!nested.is_empty());
    for z in nested {
        assert!(z.members.iter().all(|m| shape_ids.contains(m)), "too deep");
    }
    let spans = |Link(a, b): &&Link| parent.contains_key(a) && parent.get(a) != parent.get(b);
    assert!(doc.links.iter().filter(spans).count() > 10);
}

#[test]
fn shapes_in_one_column_keep_their_distance() {
    // Shapes sharing a column (same x) keep a gap of at least 120 units between tops.
    let doc = generate(2000, 3);
    let mut tops: Vec<(i32, i32)> = doc.shapes.iter().map(|s| (s.x, s.y)).collect();
    tops.sort();
    for w in tops.windows(2) {
        if w[0].0 == w[1].0 {
            assert!(w[1].1 - w[0].1 >= 120, "{:?}", w);
        }
    }
}
