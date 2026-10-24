mod common;

use willow_core::{Color, Document, Shape, ShapeId};

#[test]
fn new_document_has_one_empty_neutral_shape_at_the_origin() {
    let doc = Document::new();

    assert_eq!(doc.shapes.len(), 1);
    let s = &doc.shapes[0];
    assert_eq!((s.x, s.y), (0, 0));
    assert_eq!(s.width, 210);
    assert_eq!(s.extra_height, 0);
    assert_eq!(s.color, Color::Neutral);
    assert!(s.text.is_empty());
}

#[test]
fn generated_ids_are_short_lowercase_letters_that_skip_ids_in_use() {
    let mut doc = Document::default();
    let mut ids = Vec::new();
    for _ in 0..30 {
        let id = doc.next_id();
        doc.shapes.push(Shape::new(id.clone(), 0, 0));
        ids.push(id.0);
    }
    assert_eq!(ids[..3], ["a", "b", "c"]);
    assert_eq!(ids[25..28], ["z", "aa", "ab"]);

    let taken = |id: &str| Shape::new(ShapeId(id.into()), 0, 0);
    let doc = Document {
        shapes: vec![taken("a"), taken("b"), taken("d")],
        ..Document::default()
    };
    assert_eq!(doc.next_id(), ShapeId("c".into()));
}

#[test]
fn an_empty_shape_is_one_paper_line_plus_padding_tall() {
    let session = common::session_of(Document::new());
    let mut s = Document::new().shapes.remove(0);
    // Literata 15 px at 1.5 = 22.5, plus 13 top and bottom.
    assert_eq!(session.shape_height(&s), 48.5);
    s.extra_height = 20;
    assert_eq!(session.shape_height(&s), 68.5);
}
