//! Text alignment: Justified, Left and Centered move words within a line, nothing else.

mod common;

use common::{Fake, doc_with, session_of, text};
use willow_core::{Align, Caret, Layout, PAPER, Point, layout};

/// Content width of a default 210-wide Paper shape.
const W: f32 = 178.0;
/// Words are 40 wide, spaces 4: the first line is 172 of ink, the last 84.
const S: &str = "aaaa bbbb cccc dddd eeee ffff";

fn lay(a: Align) -> Layout {
    layout(&text(S), W, &PAPER, a, &Fake)
}

#[test]
fn left_keeps_every_gap_at_its_natural_width() {
    let l = lay(Align::Left);
    assert_eq!(l.lines[0].clusters[0].x, 0.0);
    assert_eq!(l.lines[0].clusters[5].x, 44.0);
}

#[test]
fn centered_puts_equal_space_either_side_of_the_ink() {
    let l = lay(Align::Centered);
    // (178 - 172) / 2 and (178 - 84) / 2; the space hanging at the first line's end is not ink.
    assert_eq!(l.lines[0].clusters[0].x, 3.0);
    assert_eq!(l.lines[1].clusters[0].x, 47.0);
    assert_eq!(l.lines[1].clusters[5].x, 47.0 + 44.0);
}

#[test]
fn line_breaks_and_height_are_the_same_in_all_three() {
    let ranges = |l: &Layout| l.lines.iter().map(|x| x.range.clone()).collect::<Vec<_>>();
    let j = lay(Align::Justified);
    for a in [Align::Left, Align::Centered] {
        let l = lay(a);
        assert_eq!((ranges(&l), l.height), (ranges(&j), j.height), "{a:?}");
    }
}

#[test]
fn carets_and_clicks_follow_the_words_when_centered() {
    let l = lay(Align::Centered);
    // Before "ffff" on the last line: it starts at 47 and "eeee " is 44 wide.
    let c = Caret {
        offset: 25,
        upstream: false,
    };
    assert_eq!(l.caret_point(c), Point { x: 91.0, y: 22.5 });
    assert_eq!(l.caret_at(Point { x: 92.0, y: 30.0 }), c);
    // Left of the first word is the line's start.
    assert_eq!(l.caret_at(Point { x: 0.0, y: 30.0 }).offset, 20);
    // A selection starts where its words do.
    assert_eq!(l.selection_rects(25..29, 22.5)[0].0.x, 91.0);
}

#[test]
fn an_empty_paragraph_puts_the_caret_in_the_middle_when_centered() {
    let l = layout(&text(""), W, &PAPER, Align::Centered, &Fake);
    assert_eq!(l.caret_point(Caret::default()).x, 89.0);
}

#[test]
fn switching_keeps_heights_the_document_saved_and_adds_no_undo() {
    let mut s = session_of(doc_with(S));
    let shape = s.document().shapes[0].clone();
    let (height, before) = (s.shape_height(&shape), s.layout(&shape));
    assert_eq!(s.align(), Align::Justified);

    s.set_align(Align::Centered);

    assert_eq!(s.align(), Align::Centered);
    assert_ne!(s.layout(&shape), before);
    assert_eq!(s.shape_height(&shape), height);
    assert!(s.is_saved());
    assert!(!s.can_undo());
}
