//! Growth pushes: a shape growing (typing, resizing) or being created makes room, pushing its
//! own linked subtree to the creation gap and other chains to the minimum gap.
mod common;

use std::time::Duration;

use willow_core::{Color, Document, Input, Link, ResizeTo, Session, Shape, ShapeId, Side, Zone};

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

fn shape<'a>(s: &'a Session, name: &str) -> &'a Shape {
    (s.document().shapes.iter().find(|x| x.id.0 == name)).expect(name)
}

fn at(s: &Session, name: &str) -> (i32, i32) {
    let sh = shape(s, name);
    (sh.x, sh.y)
}

/// The bottom edge of `name`, in canvas units.
fn bottom(s: &Session, name: &str) -> f32 {
    let sh = shape(s, name);
    sh.y as f32 + s.shape_height(sh)
}

/// Selects `name` and types `text` into it (starting an editing session).
fn type_into(s: &mut Session, name: &str, text: &str) {
    s.select([id(name)]);
    s.handle(
        Input::Text {
            text: text.into(),
            replace: None,
        },
        T,
    );
}

#[test]
fn typing_growth_pushes_the_subtree_below_to_keep_the_creation_gap() {
    // a, b one gap below (48.5 + 48 = 96.5 -> 97), c below b; x hangs off b to the right.
    let mut s = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194), ("x", 258, 97)],
        &[("a", "b"), ("b", "c"), ("b", "x")],
    );
    type_into(&mut s, "a", &"word ".repeat(30));
    let grown = bottom(&s, "a");
    assert!(grown > 100.0, "{grown}");
    // b sits one creation gap (48) below a's new bottom, rounded up; its subtree moves along.
    let dy = (grown + 48.0).ceil() as i32 - 97;
    assert_eq!(at(&s, "b"), (0, 97 + dy));
    assert_eq!(at(&s, "c"), (0, 194 + dy));
    assert_eq!(at(&s, "x"), (258, 97 + dy));
    assert_eq!(at(&s, "a"), (0, 0));
}

#[test]
fn other_chains_crowded_below_move_whole_and_cascade_across_three_chains() {
    // a (own chain); chain b1-b2 31.5 below a; chain c1 31.5 below b1; d off to the side.
    let mut s = session(
        &[
            ("a", 0, 0),
            ("b1", 0, 80),
            ("b2", 600, 300),
            ("c1", 0, 160),
            ("d", 900, 60),
        ],
        &[("b1", "b2")],
    );
    type_into(&mut s, "a", &"word ".repeat(30));
    // b1 is pushed to the minimum gap (24) below a's new bottom, with all of its chain.
    let db = (bottom(&s, "a") + 24.0).ceil() as i32 - 80;
    assert!(db > 0);
    assert_eq!(at(&s, "b1"), (0, 80 + db));
    assert_eq!(at(&s, "b2"), (600, 300 + db));
    // b1 then crowds c1 (its bottom was 128.5), which goes 24 below it.
    let dc = (128.5 + db as f32 + 24.0).ceil() as i32 - 160;
    assert_eq!(at(&s, "c1"), (0, 160 + dc));
    assert_eq!(at(&s, "d"), (900, 60));
}

#[test]
fn already_close_shapes_stay_until_a_growth_brings_them_closer() {
    // e (another chain) 11.5 below a, closer than the minimum gap; f overlaps a's right end.
    let mut s = session(&[("a", 0, 0), ("e", 0, 60), ("f", 200, 20)], &[]);
    // Typing that doesn't add a line, and widening (a's bottom stays), leave them be.
    type_into(&mut s, "a", "word");
    s.handle(Input::StopEditing, T);
    let r = ResizeTo {
        x: -20,
        width: 250,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!((at(&s, "e"), at(&s, "f")), ((0, 60), (200, 20)));
    // Growing a's bottom into e pushes it to the gap; f, overlapped already, stays.
    type_into(&mut s, "a", &" word".repeat(30));
    let grown = bottom(&s, "a");
    assert_eq!(at(&s, "e"), (0, (grown + 24.0).ceil() as i32));
    assert_eq!(at(&s, "f"), (200, 20));
}

#[test]
fn shrinking_and_drags_move_nothing() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97), ("e", 300, 0)], &[("a", "b")]);
    type_into(&mut s, "a", &"word ".repeat(30));
    let placed = (at(&s, "b"), at(&s, "e"));
    // Deleting the text shrinks a; nothing comes back.
    s.handle(Input::SelectAll, T);
    s.handle(Input::Delete(willow_core::Motion::Left), T);
    s.handle(Input::StopEditing, T);
    assert!(bottom(&s, "a") < 50.0);
    // A narrower resize and a drag next to e push nothing either.
    let r = ResizeTo {
        x: 20,
        width: 170,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    s.move_shape(&id("a"), 100, 10);
    assert_eq!((at(&s, "b"), at(&s, "e")), placed);
}

#[test]
fn one_undo_reverts_a_typing_session_and_its_pushes_and_zones_keep_their_members() {
    let mut doc = session(&[("a", 0, 0), ("b", 0, 97), ("e", 0, 170)], &[("a", "b")])
        .document()
        .clone();
    doc.zones.push(Zone {
        id: id("z"),
        title: "Z".into(),
        color: Color::Sky,
        x: -20,
        y: 80,
        width: 300,
        height: 150,
        members: vec![id("b"), id("e")],
    });
    let mut s = common::session_of(doc.clone());
    for word in ["word "; 30] {
        type_into(&mut s, "a", word);
    }
    // b (own chain) and then e (another chain) were pushed out of the zone's rectangle.
    assert!(at(&s, "b").1 > 230 && at(&s, "e").1 > at(&s, "b").1);
    assert_eq!(s.document().zones[0].members, [id("b"), id("e")]);
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), doc);
    assert!(!s.can_undo());
}

#[test]
fn a_midpoint_created_shape_landing_on_another_chain_pushes_that_chain_clear() {
    // The new shape goes at (0, 97), 48.5 tall; g (chain g-g2) sits under that spot.
    let mut s = session(
        &[("a", 0, 0), ("g", 0, 120), ("g2", 400, 500), ("h", 300, 97)],
        &[("g", "g2")],
    );
    s.create_linked(&id("a"), Side::Bottom);
    // 24 below the new shape's bottom (145.5): 169.5, rounded up.
    assert_eq!((at(&s, "g"), at(&s, "g2")), ((0, 170), (400, 550)));
    assert_eq!(at(&s, "h"), (300, 97));
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!((at(&s, "g"), at(&s, "g2")), ((0, 120), (400, 500)));
}

#[test]
fn widening_pushes_left_and_right_subtrees_outward_in_one_undo_step() {
    // a with l (and its child ll) on the left and r on the right, each one gap away.
    let mut s = session(
        &[("a", 0, 0), ("l", -258, 0), ("ll", -516, 0), ("r", 258, 0)],
        &[("a", "l"), ("l", "ll"), ("a", "r")],
    );
    let before = s.document().clone();
    // 40 units wider on each side: edges at -40 and 250.
    let r = ResizeTo {
        x: -40,
        width: 290,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!(at(&s, "r"), (298, 0));
    assert_eq!(at(&s, "l"), (-298, 0));
    assert_eq!(at(&s, "ll"), (-556, 0));
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
}
