//! Create zone: the menu item's availability, the zone it makes, and its single undo step.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, MenuAction, MenuEntry, Modifiers, PAPER, Point, Session, Shape,
    ShapeId, Zone, load,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(name: &str, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: name.into(),
        color: Color::Neutral,
        x: -40,
        y: -40,
        width: 600,
        height: 600,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

/// Shapes a, b, c, d side by side 300 apart; zones as given.
fn session(zones: Vec<Zone>) -> Session {
    let shapes = ["a", "b", "c", "d"]
        .iter()
        .enumerate()
        .map(|(i, n)| Shape::new(id(n), 300 * i as i32, 0))
        .collect();
    session_of(Document {
        shapes,
        links: vec![],
        zones,
    })
}

/// Right-clicks shape `n` and returns whether the open menu offers Create zone.
fn offered(s: &mut Session, n: &str) -> bool {
    let sh = s.document().shapes.iter().find(|x| x.id == id(n)).unwrap();
    let c = Point {
        x: sh.x as f32 + 5.0,
        y: sh.y as f32 + 5.0,
    };
    let pos = s.viewport().to_screen(c);
    let (mods, button) = (Modifiers::default(), Button::Right);
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.menu().is_some_and(|m| {
        m.entries
            .iter()
            .any(|e| matches!(e, MenuEntry::Item(i) if i.action == MenuAction::CreateZone))
    })
}

fn create(s: &mut Session, shapes: &[&str]) {
    s.select(shapes.iter().map(|n| id(n)));
    assert!(offered(s, shapes[0]));
    s.handle(Input::MenuChoose(MenuAction::CreateZone), T);
}

#[test]
fn the_item_shows_on_both_menus_for_loose_shapes() {
    let mut s = session(vec![]);
    s.select([id("a")]);
    assert!(offered(&mut s, "a"), "shape menu");
    s.select([id("a"), id("b")]);
    assert!(offered(&mut s, "a"), "multi menu");
}

#[test]
fn the_item_shows_for_shapes_sharing_a_top_level_zone() {
    let mut s = session(vec![zone("z", &["a", "b"])]);
    s.select([id("a"), id("b")]);
    assert!(offered(&mut s, "a"));
}

#[test]
fn the_item_is_hidden_across_parents_and_inside_nested_zones() {
    let mut s = session(vec![zone("p", &["a", "n"]), zone("n", &["b"])]);
    s.select([id("a"), id("c")]);
    assert!(!offered(&mut s, "a"), "a in a zone, c loose");
    s.select([id("b")]);
    assert!(!offered(&mut s, "b"), "inside a nested zone");
    let mut s = session(vec![zone("x", &["a"]), zone("y", &["b"])]);
    s.select([id("a"), id("b")]);
    assert!(!offered(&mut s, "a"), "two zones");
}

#[test]
fn choosing_it_when_not_allowed_does_nothing() {
    let mut s = session(vec![zone("x", &["a"])]);
    s.select([id("a"), id("c")]);
    s.handle(Input::MenuChoose(MenuAction::CreateZone), T);
    assert_eq!(s.document().zones.len(), 1);
}

#[test]
fn a_new_top_level_zone_wraps_exactly_the_selection_with_padding() {
    let mut s = session(vec![]);
    let h = s.shape_height(&s.document().shapes[2].clone());
    create(&mut s, &["a", "c"]);
    let doc = s.document();
    let z = &doc.zones[0];
    let pad = PAPER.zone.padding;
    assert_eq!(z.members, [id("a"), id("c")], "b between them stays out");
    assert_eq!((z.x, z.y), (-(pad as i32), -(pad as i32)));
    assert_eq!(z.width, (600.0 + 210.0 + 2.0 * pad).ceil() as u32);
    assert_eq!(z.height, (h + 2.0 * pad).ceil() as u32);
    assert_eq!((z.title.as_str(), z.color), ("", Color::Neutral));
    assert!(s.menu().is_none());
}

#[test]
fn a_nested_zone_joins_its_parent_and_draws_above_overlapping_siblings() {
    let mut s = session(vec![zone("p", &["a", "b"]), zone("q", &["c"])]);
    create(&mut s, &["a"]);
    let zones = &s.document().zones;
    assert_eq!(zones.len(), 3);
    let new = &zones[2];
    assert_eq!(new.members, [id("a")]);
    assert!(!new.id.0.is_empty() && zones[..2].iter().all(|z| z.id != new.id));
    assert_eq!(zones[0].members, [id("b"), new.id.clone()]);
}

#[test]
fn creation_is_one_undo_step_and_the_saved_text_round_trips() {
    let mut s = session(vec![]);
    create(&mut s, &["a", "b"]);
    let mut bytes = Vec::new();
    s.save(|b| {
        bytes = b.to_vec();
        Ok(())
    })
    .unwrap();
    let loaded = load(&bytes).unwrap();
    assert_eq!(loaded.document.zones, s.document().zones);
    s.handle(Input::Undo, T);
    assert!(s.document().zones.is_empty());
    s.handle(Input::Redo, T);
    assert_eq!(s.document().zones.len(), 1);
}
