//! A new linked shape (midpoint creation, insert-and-push, Tab) joins its source's zone, even
//! past the zone's edge.
mod common;

use std::time::Duration;

use willow_core::{Color, Document, Input, Link, Session, Shape, ShapeId, Side, Zone};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(name: &str, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: name.into(),
        color: Color::Neutral,
        x: -50,
        y: -50,
        // Tight around a and b: anything created below or beside lands outside.
        width: 300,
        height: 220,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

/// a (selected) with b linked below it, c loose and far off; zones as given.
fn session(zones: Vec<Zone>) -> Session {
    let doc = Document {
        shapes: [("a", 0, 0), ("b", 0, 100), ("c", 3000, 0)]
            .map(|(s, x, y)| Shape::new(id(s), x, y))
            .to_vec(),
        links: vec![Link(id("a"), id("b"))],
        zones,
    };
    let mut s = common::session_of(doc);
    s.select([id("a")]);
    s
}

fn new_id(s: &Session) -> ShapeId {
    s.editing().expect("editing the new shape").id.clone()
}

#[test]
fn a_top_level_zone_member_creates_into_its_zone_even_past_the_edge() {
    let mut s = session(vec![zone("z", &["a", "b"])]);
    s.create_linked(&id("a"), Side::Right);
    let n = new_id(&s);
    let z = &s.document().zones[0];
    assert_eq!(z.members, [id("a"), id("b"), n.clone()]);
    let shape = s.document().shapes.iter().find(|x| x.id == n).unwrap();
    assert!(shape.x > z.x + z.width as i32, "lands past the right edge");
}

#[test]
fn a_nested_zone_member_creates_into_the_nested_zone() {
    let mut s = session(vec![zone("p", &["n", "b"]), zone("n", &["a"])]);
    s.create_linked(&id("a"), Side::Left);
    let n = new_id(&s);
    let zones = &s.document().zones;
    assert_eq!(zones[1].members, [id("a"), n.clone()]);
    assert!(!zones[0].members.contains(&n));
}

#[test]
fn a_source_in_no_zone_gives_a_loose_shape() {
    let mut s = session(vec![zone("z", &["b"])]);
    s.create_linked(&id("a"), Side::Right);
    assert_eq!(s.document().zones[0].members, [id("b")]);
}

#[test]
fn insert_and_push_joins_the_sources_zone() {
    // b is below a: creating at the bottom inserts between them and pushes b out of the zone.
    let mut s = session(vec![zone("z", &["a", "b"])]);
    s.create_linked(&id("a"), Side::Bottom);
    let n = new_id(&s);
    assert_eq!(s.document().zones[0].members, [id("a"), id("b"), n]);
}

#[test]
fn tab_then_an_arrow_joins_the_sources_zone() {
    let mut s = session(vec![zone("z", &["a"])]);
    s.handle(Input::Tab, T);
    s.handle(Input::PickSide(Side::Right), T);
    let n = new_id(&s);
    assert_eq!(s.document().zones[0].members, [id("a"), n]);
}

#[test]
fn undo_removes_the_shape_and_its_membership_in_one_step_and_the_save_shows_it() {
    let mut s = session(vec![zone("z", &["a", "b"])]);
    let before = s.document().clone();
    s.create_linked(&id("a"), Side::Right);
    let n = new_id(&s);
    s.handle(Input::Escape, T);
    let mut saved = Vec::new();
    s.save(|b| {
        saved = b.to_vec();
        Ok(())
    })
    .unwrap();
    let text = String::from_utf8(saved).unwrap();
    let block = (text.split("zone z").nth(1).unwrap().split("\n}"))
        .next()
        .unwrap();
    assert!(block.contains(&format!("{}:", n.0)), "{text}");
    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}
