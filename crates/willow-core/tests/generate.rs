use std::collections::HashSet;
use willow_core::{Color, generate};

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
