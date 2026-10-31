//! Keyboard move and resize: Ctrl/Cmd+Arrows move the selection 24 units; with Shift they
//! resize the one selected shape by 24.
mod common;

use std::time::Duration;

use willow_core::{Color, Document, Input, Link, Session, Shape, ShapeId, Side, Zone};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty rose shapes `(id, x, y)` (210 x 48.5 each), linked as `links`.
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    common::session_of(doc(shapes, links))
}

fn doc(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Document {
    Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    }
}

fn shape<'a>(s: &'a Session, name: &str) -> &'a Shape {
    (s.document().shapes.iter().find(|x| x.id.0 == name)).expect(name)
}

fn at(s: &Session, name: &str) -> (i32, i32) {
    let sh = shape(s, name);
    (sh.x, sh.y)
}

#[test]
fn ctrl_arrows_move_every_selected_shape_24_pushing_nothing_and_keeping_zones() {
    // a and b selected inside zone z; c, another chain, sits where a moves to.
    let mut d = doc(&[("a", 0, 0), ("b", 0, 97), ("c", 30, 0)], &[("a", "b")]);
    d.zones.push(Zone {
        id: id("z"),
        title: "Z".into(),
        color: Color::Sky,
        x: -20,
        y: -20,
        width: 260,
        height: 180,
        members: vec![id("a"), id("b")],
    });
    let mut s = common::session_of(d);
    s.handle(Input::Nudge(Side::Right), T);
    assert_eq!(at(&s, "a"), (0, 0), "nothing selected: nothing moves");
    s.select([id("a"), id("b")]);
    s.handle(Input::Nudge(Side::Right), T);
    assert_eq!((at(&s, "a"), at(&s, "b")), ((24, 0), (24, 97)));
    for _ in 0..20 {
        s.handle(Input::Nudge(Side::Bottom), T);
    }
    s.handle(Input::Nudge(Side::Left), T);
    s.handle(Input::Nudge(Side::Top), T);
    assert_eq!((at(&s, "a"), at(&s, "b")), ((0, 456), (0, 553)));
    assert_eq!(at(&s, "c"), (30, 0));
    assert_eq!(s.document().zones[0].members, [id("a"), id("b")]);
}

/// The first shape's x, width and extra height.
fn size(s: &Session) -> (i32, u32, u32) {
    let sh = &s.document().shapes[0];
    (sh.x, sh.width, sh.extra_height)
}

#[test]
fn ctrl_shift_arrows_resize_by_24_symmetrically_and_never_below_the_text() {
    // Widest cluster is a 15 CJK: 2 * 16 + 15 = 47 is the narrowest; whole units stop at 48.
    let mut s = common::session_of(common::doc_with("ab\u{3042}"));
    let first = s.document().shapes[0].id.clone();
    s.select([first]);
    s.handle(Input::NudgeSize(Side::Right), T);
    assert_eq!(size(&s), (-12, 234, 0));
    s.handle(Input::NudgeSize(Side::Left), T);
    assert_eq!(size(&s), (0, 210, 0));
    for _ in 0..10 {
        s.handle(Input::NudgeSize(Side::Left), T);
    }
    assert_eq!(size(&s), (81, 48, 0));
    s.handle(Input::NudgeSize(Side::Bottom), T);
    s.handle(Input::NudgeSize(Side::Bottom), T);
    assert_eq!(size(&s), (81, 48, 48));
    for _ in 0..3 {
        s.handle(Input::NudgeSize(Side::Top), T);
    }
    assert_eq!(size(&s), (81, 48, 0));
}

#[test]
fn ctrl_shift_arrows_do_nothing_with_a_multi_selection_or_none() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97)], &[]);
    let before = s.document().clone();
    s.handle(Input::NudgeSize(Side::Right), T);
    s.select([id("a"), id("b")]);
    s.handle(Input::NudgeSize(Side::Right), T);
    s.handle(Input::NudgeSize(Side::Bottom), T);
    assert_eq!(*s.document(), before);
}

#[test]
fn widening_and_growing_push_as_the_mouse_resize_does() {
    // l (and its child ll) one gap left of a, r one gap right, b one gap below; a is topmost.
    let mut s = session(
        &[
            ("a", 0, 0),
            ("l", -258, 1),
            ("ll", -516, 1),
            ("r", 258, 1),
            ("b", 0, 97),
        ],
        &[("a", "l"), ("l", "ll"), ("a", "r"), ("a", "b")],
    );
    let drawn = |s: &Session, name: &str| {
        let r = s.drawn_rect(&id(name)).expect(name);
        (r.x as i32, r.y as i32)
    };
    s.select([id("a")]);
    s.handle(Input::NudgeSize(Side::Right), T);
    // a's edges at -12 and 222: each side's subtree keeps the 48 creation gap.
    let sides = (drawn(&s, "l"), drawn(&s, "ll"), drawn(&s, "r"));
    assert_eq!(sides, ((-270, 1), (-528, 1), (270, 1)));
    s.handle(Input::NudgeSize(Side::Bottom), T);
    // a's bottom at 72.5: b goes to 120.5, rounded up.
    assert_eq!(drawn(&s, "b"), (0, 121));
    assert_eq!(at(&s, "b"), (0, 97));
    // Narrowing and shrinking settle them back.
    s.handle(Input::NudgeSize(Side::Left), T);
    s.handle(Input::NudgeSize(Side::Top), T);
    let all = (drawn(&s, "l"), drawn(&s, "r"), drawn(&s, "b"));
    assert_eq!(all, ((-258, 1), (258, 1), (0, 97)));
}

#[test]
fn each_press_is_its_own_undo_step() {
    let mut s = session(&[("a", 0, 0), ("r", 258, 0)], &[("a", "r")]);
    let start = s.document().clone();
    s.select([id("a")]);
    s.handle(Input::Nudge(Side::Bottom), T);
    let moved = s.document().clone();
    s.handle(Input::NudgeSize(Side::Right), T);
    let widened = s.document().clone();
    s.handle(Input::NudgeSize(Side::Right), T);
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), widened);
    s.handle(Input::Undo, T);
    assert_eq!(
        *s.document(),
        moved,
        "the widening and its push undo together"
    );
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), start);
    assert!(!s.can_undo());
}

#[test]
fn never_fires_while_editing() {
    let mut s = session(&[("a", 0, 0)], &[]);
    s.select([id("a")]);
    s.handle(Input::StartEditing, T);
    let before = s.document().clone();
    for side in [Side::Right, Side::Bottom] {
        s.handle(Input::Nudge(side), T);
        s.handle(Input::NudgeSize(side), T);
    }
    assert_eq!(*s.document(), before);
    assert!(s.editing().is_some());
}
