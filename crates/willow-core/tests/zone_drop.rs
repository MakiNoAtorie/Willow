//! Zone membership by drop: a dropped shape joins the zone drawn on top under its center, or
//! none; every placement picks the same zone; nothing else changes membership.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, Link, MenuAction, Modifiers, Point, Session, Shape, ShapeId,
    Side, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(id: &str, x: i32, y: i32, w: u32, h: u32, members: &[&str]) -> Zone {
    Zone {
        id: ShapeId(id.into()),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width: w,
        height: h,
        members: members.iter().map(|m| ShapeId((*m).into())).collect(),
    }
}

/// Empty shapes at the given top-lefts (each is 210 wide), in the given zones.
fn session(shapes: &[(&str, i32, i32)], zones: Vec<Zone>) -> Session {
    session_of(Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        zones,
        ..Document::default()
    })
}

fn at(s: &Session, x: f32, y: f32) -> Point {
    s.viewport().to_screen(Point { x, y })
}

/// A left-button pointer down (0), move (1) or up (2) at canvas point `c`.
fn pointer_at(s: &mut Session, kind: u8, c: (f32, f32)) {
    let (pos, button, mods) = (at(s, c.0, c.1), Button::Left, Modifiers::default());
    let input = match kind {
        0 => Input::PointerDown { pos, button, mods },
        1 => Input::PointerMove { pos, mods },
        _ => Input::PointerUp { pos, button, mods },
    };
    s.handle(input, T);
}

/// Presses at canvas point `from` and moves to canvas point `to`, not yet released.
fn drag_to(s: &mut Session, from: (f32, f32), to: (f32, f32)) {
    pointer_at(s, 0, from);
    pointer_at(s, 1, to);
}

/// Drags from canvas point `from` to canvas point `to` and releases.
fn drag(s: &mut Session, from: (f32, f32), to: (f32, f32)) {
    drag_to(s, from, to);
    pointer_at(s, 2, to);
}

/// Each zone's members, by ID.
fn members(s: &Session) -> Vec<(&str, Vec<&str>)> {
    (s.document().zones.iter())
        .map(|z| {
            (
                z.id.0.as_str(),
                z.members.iter().map(|m| m.0.as_str()).collect(),
            )
        })
        .collect()
}

#[test]
fn a_drop_joins_the_zone_under_the_shapes_center_and_leaves_its_old_one() {
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("p", 1000, 0, 600, 600, &[]),
            zone("q", 2000, 0, 600, 600, &[]),
        ],
    );
    // Pressed 10 in from a's top-left; a's center lands at (1300, 300).
    drag(&mut s, (10.0, 10.0), (1210.0, 287.0));
    assert_eq!(members(&s), [("p", vec!["a"]), ("q", vec![])]);
    drag(&mut s, (1210.0, 287.0), (2210.0, 287.0));
    assert_eq!(members(&s), [("p", vec![]), ("q", vec!["a"])]);
    drag(&mut s, (2210.0, 287.0), (10.0, 10.0));
    assert_eq!(members(&s), [("p", vec![]), ("q", vec![])]);
}

#[test]
fn a_nested_member_dropped_in_the_parent_outside_the_nested_zone_joins_the_parent() {
    let mut s = session(
        &[("a", 1100, 100)],
        vec![
            zone("p", 1000, 0, 1000, 1000, &["n"]),
            zone("n", 1050, 50, 400, 400, &["a"]),
        ],
    );
    drag(&mut s, (1110.0, 110.0), (1610.0, 610.0));
    assert_eq!(members(&s), [("p", vec!["n", "a"]), ("n", vec![])]);
    drag(&mut s, (1610.0, 610.0), (1110.0, 110.0));
    assert_eq!(members(&s), [("p", vec!["n"]), ("n", vec!["a"])]);
}

#[test]
fn across_overlapping_siblings_the_zone_drawn_on_top_wins() {
    // q, the larger, draws over p where they overlap.
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("p", 1000, 0, 400, 400, &[]),
            zone("q", 1200, 0, 800, 800, &[]),
        ],
    );
    // a's center lands at (1300, 200), inside both.
    drag(&mut s, (10.0, 10.0), (1205.0, 187.0));
    assert_eq!(members(&s), [("p", vec![]), ("q", vec!["a"])]);
    // Inside p only.
    drag(&mut s, (1205.0, 187.0), (1005.0, 187.0));
    assert_eq!(members(&s), [("p", vec!["a"]), ("q", vec![])]);
}

#[test]
fn a_multi_selection_drop_judges_each_shape_by_its_own_center_and_one_undo_reverts_it() {
    let mut s = session(
        &[("a", 0, 0), ("b", 300, 0)],
        vec![
            zone("p", 1000, 0, 300, 300, &[]),
            zone("q", 250, 0, 400, 100, &["b"]),
        ],
    );
    s.select([id("a"), id("b")]);
    let before = s.document().clone();
    // a's center lands at (1105, 23.5) in p, b's at (1405, 23.5) in no zone.
    drag(&mut s, (10.0, 10.0), (1010.0, 10.0));
    assert_eq!(members(&s), [("p", vec!["a"]), ("q", vec![])]);
    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn one_undo_reverts_a_single_drop_and_its_membership() {
    let mut s = session(&[("a", 0, 0)], vec![zone("p", 1000, 0, 600, 600, &[])]);
    let before = s.document().clone();
    drag(&mut s, (10.0, 10.0), (1210.0, 287.0));
    assert_eq!(members(&s), [("p", vec!["a"])]);
    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
    s.handle(Input::Redo, T);
    assert_eq!(members(&s), [("p", vec!["a"])]);
    let a = &s.document().shapes[0];
    assert_eq!((a.x, a.y), (1200, 277));
}

/// Shape a at the origin; siblings p and q overlap, q (the larger) drawn on top; r is a
/// shrunk parent its nested zone n sticks out of.
fn placements() -> Vec<Zone> {
    vec![
        zone("p", 1000, 0, 400, 400, &[]),
        zone("q", 1200, 0, 800, 800, &[]),
        zone("r", 3000, 0, 200, 200, &["n"]),
        zone("n", 3100, 50, 600, 600, &[]),
    ]
}

/// The zone shape `shape` is a member of.
fn zone_of<'a>(s: &'a Session, shape: &ShapeId) -> Option<&'a str> {
    (s.document().zones.iter())
        .find(|z| z.members.contains(shape))
        .map(|z| z.id.0.as_str())
}

fn right_click(s: &mut Session, c: (f32, f32)) {
    let (pos, button, mods) = (at(s, c.0, c.1), Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

/// The zone each placement at canvas point `c` puts its shape in: a drop centered there,
/// the empty-space New shape, Ctrl/Cmd+Shift+N centered there, and a paste there.
fn placed_in(c: (f32, f32)) -> [Option<String>; 4] {
    let new = |f: &dyn Fn(&mut Session)| {
        let mut s = session(&[("a", 0, 0)], placements());
        f(&mut s);
        let last = s.document().shapes.last().unwrap().id.clone();
        zone_of(&s, &last).map(String::from)
    };
    [
        new(&|s| drag(s, (105.0, 23.5), c)),
        new(&|s| {
            right_click(s, c);
            s.handle(Input::MenuChoose(MenuAction::NewShape), T);
        }),
        new(&|s| {
            s.select([]);
            let mut vp = s.viewport();
            vp.origin = Point {
                x: c.0 - vp.size.x / 2.0,
                y: c.1 - vp.size.y / 2.0,
            };
            s.set_viewport(vp);
            s.handle(Input::NewBeside, T);
        }),
        new(&|s| {
            right_click(s, c);
            s.handle(Input::Paste("x".into()), T);
        }),
    ]
}

#[test]
fn text_growth_pushes_and_insert_and_push_never_change_membership_or_zone_sizes() {
    // Chain a-b-c down the left; p holds b, q (empty) lies where the pushes take c.
    let zones = vec![
        zone("p", -20, 80, 300, 100, &["b"]),
        zone("q", -20, 300, 300, 300, &[]),
    ];
    let mut doc = session(&[("a", 0, 0), ("b", 0, 97), ("c", 0, 194)], zones)
        .document()
        .clone();
    doc.links = vec![Link(id("a"), id("b")), Link(id("b"), id("c"))];
    let mut s = session_of(doc);
    let before = s.document().zones.clone();
    // Insert-and-push below a, then type lines into the new shape so it grows and pushes.
    s.create_linked(&id("a"), Side::Bottom);
    for _ in 0..4 {
        let text = "x".to_string();
        s.handle(
            Input::Text {
                text,
                replace: None,
            },
            T,
        );
        s.handle(Input::NewLine, T);
    }
    s.handle(Input::Escape, T);
    let c = s.drawn_rect(&id("c")).unwrap();
    assert!(c.y > 300.0 && c.y < 550.0, "c pushed into q, at {}", c.y);
    assert_eq!(s.document().zones, before);
}

#[test]
fn new_shape_new_beside_and_paste_pick_the_zone_a_drop_there_would_join() {
    for (c, want) in [
        ((1300.0, 200.0), Some("q")),
        ((1100.0, 200.0), Some("p")),
        ((3150.0, 100.0), Some("n")),
        ((3500.0, 400.0), Some("n")),
        ((500.0, 500.0), None),
    ] {
        let want = want.map(String::from);
        assert_eq!(placed_in(c), [(); 4].map(|_| want.clone()), "at {c:?}");
    }
}

/// The zone the draw query rings.
fn ring(s: &Session) -> Option<&str> {
    let frame = s.draw(Point::default(), at(s, 3000.0, 3000.0), T);
    frame.zone_ring.map(|z| z.0.as_str())
}

#[test]
fn while_dragging_the_draw_query_rings_the_target_only_when_membership_would_change() {
    let mut s = session(
        &[("a", 1100, 100)],
        vec![
            zone("p", 1000, 0, 600, 600, &["a"]),
            zone("q", 2000, 0, 600, 600, &[]),
        ],
    );
    assert_eq!(ring(&s), None);
    // Within p: no change, no ring.
    drag_to(&mut s, (1110.0, 110.0), (1210.0, 210.0));
    assert_eq!(ring(&s), None);
    // Over q: q rings.
    pointer_at(&mut s, 1, (2110.0, 110.0));
    assert_eq!(ring(&s), Some("q"));
    // Out of every zone: nothing to join, no ring.
    pointer_at(&mut s, 1, (110.0, 110.0));
    assert_eq!(ring(&s), None);
    pointer_at(&mut s, 2, (110.0, 110.0));
    assert_eq!(ring(&s), None);
}

#[test]
fn a_multi_selection_rings_the_pressed_shapes_target_else_the_first_change_in_story_order() {
    let mut s = session(
        &[("a", 0, 0), ("b", 300, 0), ("c", 600, 0)],
        vec![
            zone("r", 750, 0, 250, 300, &[]),
            zone("p", 1000, 0, 300, 300, &[]),
            zone("q", 1600, 0, 300, 300, &[]),
        ],
    );
    // Selected in an order other than story order (a, b, c).
    s.select([id("c"), id("a"), id("b")]);
    // Pressing b, 1000 right: b lands in no zone, a in p, c in q; a comes first in story order.
    drag_to(&mut s, (310.0, 10.0), (1310.0, 10.0));
    assert_eq!(ring(&s), Some("p"));
    // 700 right: a lands in r, b in p; the pressed shape's target wins.
    pointer_at(&mut s, 1, (1010.0, 10.0));
    assert_eq!(ring(&s), Some("p"));
    // Back where they were: no membership changes, no ring.
    pointer_at(&mut s, 1, (310.0, 10.0));
    assert_eq!(ring(&s), None);
}
