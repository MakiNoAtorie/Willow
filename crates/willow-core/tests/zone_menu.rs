//! The zone menu: right-click on a title band for the zone's color, Dissolve and Delete.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, Link, MenuAction, MenuEntry, MenuKind, Modifiers, Point,
    Session, Shape, ShapeId, Zone,
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

/// Empty shapes at the given top-lefts (each is 210 wide), linked and zoned as given.
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)], zones: Vec<Zone>) -> Session {
    session_of(Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        zones,
    })
}

/// Right-clicks canvas point (x, y).
fn right_click(s: &mut Session, x: f32, y: f32) {
    let pos = s.viewport().to_screen(Point { x, y });
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

/// Opens the menu on the band of the zone whose top-left is (x, y) and chooses `action`.
fn choose_on_band(s: &mut Session, x: f32, y: f32, action: MenuAction) {
    right_click(s, x + 20.0, y + 10.0);
    assert_eq!(s.menu().map(|m| m.kind), Some(MenuKind::Zone));
    s.handle(Input::MenuChoose(action), T);
    assert!(s.menu().is_none());
}

fn zones(s: &Session) -> Vec<(&str, Vec<&str>)> {
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
fn right_click_on_the_band_opens_the_zone_menu_and_elsewhere_in_the_zone_the_empty_menu() {
    let mut z = zone("z", 0, 0, 800, 600, &["a"]);
    z.color = Color::Sky;
    let mut s = session(&[("a", 400, 300)], &[], vec![z]);

    right_click(&mut s, 20.0, 10.0);
    let menu = s.menu().expect("zone menu");
    assert_eq!(menu.kind, MenuKind::Zone);
    assert_eq!(menu.zone, Some(id("z")));
    let colors = (menu.entries.iter()).find(|e| matches!(e, MenuEntry::Colors { .. }));
    assert_eq!(
        colors,
        Some(&MenuEntry::Colors {
            current: Some(Color::Sky)
        })
    );
    let items: Vec<_> = (menu.entries.iter())
        .filter_map(|e| match e {
            MenuEntry::Item(i) => Some((i.action, i.label, i.destructive)),
            _ => None,
        })
        .collect();
    assert_eq!(
        items,
        [
            (MenuAction::Rename, "Rename", false),
            (MenuAction::Dissolve, "Dissolve", false),
            (MenuAction::DeleteZone, "Delete", true),
        ]
    );

    right_click(&mut s, 20.0, 300.0);
    assert_eq!(s.menu().map(|m| m.kind), Some(MenuKind::Empty));
}

#[test]
fn a_color_colors_only_the_zone_in_one_undo_step() {
    let mut s = session(
        &[("a", 100, 100)],
        &[],
        vec![zone("z", 0, 0, 800, 600, &["a"])],
    );
    let before = s.document().clone();

    choose_on_band(&mut s, 0.0, 0.0, MenuAction::Recolor(Color::Rose));
    assert_eq!(s.document().zones[0].color, Color::Rose);
    assert_eq!(s.document().shapes, before.shapes);

    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

/// Top-level zone p holds shape a and nested zone n, which holds b; c is loose.
fn nested() -> Session {
    session(
        &[("a", 600, 400), ("b", 200, 250), ("c", 1200, 0)],
        &[],
        vec![
            zone("p", 0, 0, 1000, 800, &["a", "n"]),
            zone("n", 100, 100, 400, 300, &["b"]),
        ],
    )
}

#[test]
fn dissolving_a_nested_zone_gives_its_shapes_to_its_parent_in_one_undo_step() {
    let mut s = nested();
    let before = s.document().clone();

    choose_on_band(&mut s, 100.0, 100.0, MenuAction::Dissolve);
    assert_eq!(zones(&s), [("p", vec!["a", "b"])]);
    assert_eq!(s.document().shapes, before.shapes);

    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn dissolving_a_top_level_zone_makes_its_nested_zones_top_level_and_its_shapes_loose() {
    let mut s = nested();
    let before = s.document().clone();

    choose_on_band(&mut s, 0.0, 0.0, MenuAction::Dissolve);
    assert_eq!(zones(&s), [("n", vec!["b"])]);
    assert_eq!(s.document().shapes, before.shapes);

    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn delete_removes_the_zone_its_nested_zones_and_every_member_reconnecting_across_the_edge() {
    // A trunk t-m1-m2-u crosses zone z: m1 in z, m2 in z's nested zone n. Member o lies
    // outside z's rectangle; x lies inside it but isn't a member.
    let mut s = session(
        &[
            ("t", 0, 0),
            ("m1", 0, 300),
            ("m2", 0, 550),
            ("u", 0, 1000),
            ("o", 1500, 300),
            ("x", 0, 700),
        ],
        &[("t", "m1"), ("m1", "m2"), ("m2", "u")],
        vec![
            zone("z", -50, 200, 400, 600, &["m1", "n", "o"]),
            zone("n", -30, 450, 360, 220, &["m2"]),
        ],
    );
    let before = s.document().clone();

    choose_on_band(&mut s, -50.0, 200.0, MenuAction::DeleteZone);
    let doc = s.document();
    let ids: Vec<_> = doc.shapes.iter().map(|s| s.id.0.as_str()).collect();
    assert_eq!(ids, ["t", "u", "x"]);
    assert_eq!(doc.links, [Link(id("t"), id("u"))]);
    assert!(doc.zones.is_empty());
    assert_eq!(
        (doc.shapes.iter().map(|s| (s.x, s.y)).collect::<Vec<_>>()),
        [(0, 0), (0, 1000), (0, 700)]
    );

    s.handle(Input::Undo, T);
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn a_zone_left_with_no_members_stays() {
    let mut s = session(
        &[("a", 100, 100)],
        &[],
        vec![
            zone("p", 0, 0, 1000, 800, &["n"]),
            zone("n", 50, 50, 400, 300, &["a"]),
        ],
    );
    s.select([id("a")]);
    s.handle(Input::DeleteShapes, T);
    assert_eq!(zones(&s), [("p", vec!["n"]), ("n", vec![])]);

    choose_on_band(&mut s, 50.0, 50.0, MenuAction::DeleteZone);
    assert_eq!(zones(&s), [("p", vec![])]);
}
