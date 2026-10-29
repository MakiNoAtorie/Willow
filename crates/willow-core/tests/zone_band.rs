//! The title band: the strip along a zone's top edge that the mouse grabs to move the zone.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, Modifiers, Point, Session, Shape, ShapeId, Zone,
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

/// A left-button pointer down (0), move (1) or up (2) at canvas point `c`, with `mods`.
fn pointer_with(s: &mut Session, kind: u8, c: (f32, f32), mods: Modifiers) {
    let (pos, button) = (at(s, c.0, c.1), Button::Left);
    let input = match kind {
        0 => Input::PointerDown { pos, button, mods },
        1 => Input::PointerMove { pos, mods },
        _ => Input::PointerUp { pos, button, mods },
    };
    s.handle(input, T);
}

fn pointer_at(s: &mut Session, kind: u8, c: (f32, f32)) {
    pointer_with(s, kind, c, Modifiers::default());
}

/// The zone the draw query rings.
fn ring(s: &Session) -> Option<&str> {
    let frame = s.draw(Point::default(), at(s, 4000.0, 4000.0), T);
    frame.zone_ring.map(|z| z.0.as_str())
}

/// Hovers canvas point `c` and returns the ringed zone.
fn ring_at(s: &mut Session, c: (f32, f32)) -> Option<&str> {
    pointer_at(s, 1, c);
    ring(s)
}

fn set_zoom(s: &mut Session, zoom: f32) {
    let mut vp = s.viewport();
    vp.zoom = zoom;
    s.set_viewport(vp);
}

#[test]
fn the_band_is_a_constant_screen_height_strip_along_the_top_with_any_title() {
    let mut long = zone("p", 1000, 0, 400, 600, &[]);
    long.title = "A title far too long to fit in this zone, so it is hidden".into();
    let mut s = session(
        &[("a", 0, 0)],
        vec![long, zone("q", 2000, 0, 400, 600, &[])],
    );
    for (zoom, case) in [(1.0, "1x"), (0.5, "0.5x"), (0.25, "0.25x")] {
        set_zoom(&mut s, zoom);
        // 35 screen px down is in the band, 45 below it, on both the hidden and empty title.
        for z in [1000.0, 2000.0] {
            let name = if z == 1000.0 { "p" } else { "q" };
            assert_eq!(
                ring_at(&mut s, (z + 200.0, 35.0 / zoom)),
                Some(name),
                "{case}"
            );
            assert_eq!(ring_at(&mut s, (z + 200.0, 45.0 / zoom)), None, "{case}");
        }
    }
}

#[test]
fn a_shape_over_the_band_wins_and_the_band_drawn_on_top_wins() {
    let mut s = session(
        &[("a", 0, 0), ("b", 1100, 0)],
        vec![
            zone("p", 1000, 0, 800, 600, &["n"]),
            zone("n", 1400, 20, 300, 300, &[]),
            // q, a sibling of p drawn over it, covers p's band from 1700.
            zone("q", 1700, 0, 600, 600, &[]),
        ],
    );
    assert_eq!(ring_at(&mut s, (1050.0, 10.0)), Some("p"));
    assert_eq!(ring_at(&mut s, (1150.0, 10.0)), None, "the shape wins");
    assert_eq!(ring_at(&mut s, (1500.0, 30.0)), Some("n"), "nested wins");
    assert_eq!(ring_at(&mut s, (1500.0, 100.0)), None, "nested body");
    assert_eq!(
        ring_at(&mut s, (1750.0, 10.0)),
        Some("q"),
        "the top sibling"
    );
}

/// Each shape's and zone's top-left y, by ID, shapes first.
fn ys(s: &Session) -> Vec<(String, i32)> {
    let doc = s.document();
    (doc.shapes.iter().map(|s| (s.id.0.clone(), s.y)))
        .chain(doc.zones.iter().map(|z| (z.id.0.clone(), z.y)))
        .collect()
}

#[test]
fn dragging_the_band_moves_members_by_the_list_and_nested_zones_as_one_undo_step() {
    // `far` is a member outside p's rectangle; `c` only lies inside p.
    let mut s = session(
        &[
            ("a", 1050, 450),
            ("b", 1150, 150),
            ("far", 3000, 0),
            ("c", 1500, 400),
        ],
        vec![
            zone("p", 1000, 0, 800, 600, &["n", "a", "far"]),
            zone("n", 1100, 100, 300, 300, &["b"]),
        ],
    );
    let before = ys(&s);
    pointer_at(&mut s, 0, (1700.0, 10.0));
    pointer_at(&mut s, 1, (1700.0, 1010.0));
    let frame = s.draw(Point::default(), at(&s, 4000.0, 4000.0), T);
    let drawn: Vec<(String, i32)> = (frame.shapes.iter().map(|v| (v.shape.id.0.clone(), v.y)))
        .chain(frame.zones.iter().map(|v| (v.zone.id.0.clone(), v.zone.y)))
        .collect();
    let moved: Vec<(String, i32)> = [
        ("a", 1450),
        ("b", 1150),
        ("far", 1000),
        ("c", 400),
        ("p", 1000),
        ("n", 1100),
    ]
    .map(|(i, y)| (i.to_string(), y))
    .into();
    assert_eq!(drawn, moved, "the preview");
    assert_eq!(ys(&s), before, "the document waits for the drop");
    pointer_at(&mut s, 2, (1700.0, 1010.0));
    assert_eq!(ys(&s), moved);
    assert_eq!(s.document().zones[0].members, [id("n"), id("a"), id("far")]);
    s.handle(Input::Undo, T);
    assert_eq!(ys(&s), before);
}

/// Drags from canvas point `from` to canvas point `to` and releases.
fn drag(s: &mut Session, from: (f32, f32), to: (f32, f32)) {
    pointer_at(s, 0, from);
    pointer_at(s, 1, to);
    pointer_at(s, 2, to);
}

/// Each zone's members, by ID, in draw order.
fn members(s: &Session) -> Vec<(String, Vec<String>)> {
    (s.document().zones.iter())
        .map(|z| {
            (
                z.id.0.clone(),
                z.members.iter().map(|m| m.0.clone()).collect(),
            )
        })
        .collect()
}

fn expect(zones: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    (zones.iter())
        .map(|(z, m)| (z.to_string(), m.iter().map(|m| m.to_string()).collect()))
        .collect()
}

#[test]
fn a_dropped_zone_nests_in_the_top_level_zone_under_its_center_or_none() {
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("p", 1000, 0, 1000, 1000, &[]),
            zone("z", 0, 2000, 400, 400, &[]),
        ],
    );
    // z's center (200, 2200) lands at (1500, 500).
    drag(&mut s, (100.0, 2010.0), (1400.0, 310.0));
    assert_eq!(members(&s), expect(&[("p", &["z"]), ("z", &[])]));
    drag(&mut s, (1400.0, 310.0), (1400.0, 2310.0));
    assert_eq!(members(&s), expect(&[("p", &[]), ("z", &[])]));
}

#[test]
fn nested_zones_are_never_targets_and_a_zone_holding_one_never_nests() {
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("p", 1000, 0, 1000, 1000, &["m"]),
            zone("m", 1200, 200, 400, 400, &[]),
            zone("z", 0, 2000, 200, 200, &[]),
            zone("h", 3000, 0, 400, 400, &["k"]),
            zone("k", 3100, 100, 100, 100, &[]),
        ],
    );
    // z's center lands on m's: it joins m's parent.
    drag(&mut s, (50.0, 2010.0), (1350.0, 310.0));
    let nested = expect(&[
        ("p", &["m", "z"]),
        ("m", &[]),
        ("z", &[]),
        ("h", &["k"]),
        ("k", &[]),
    ]);
    assert_eq!(members(&s), nested);
    // h holds k: dropped on p's center it just moves.
    drag(&mut s, (3300.0, 10.0), (1600.0, 310.0));
    assert_eq!(members(&s), nested);
    assert_eq!(s.document().zones[3].x, 1300);
    // m dragged out of p becomes top-level.
    drag(&mut s, (1220.0, 210.0), (1220.0, 2210.0));
    let p: Vec<_> = members(&s).into_iter().filter(|(z, _)| z == "p").collect();
    assert_eq!(p, expect(&[("p", &["z"])]));
}

/// The zones' IDs in draw order.
fn order(s: &Session) -> Vec<&str> {
    s.document().zones.iter().map(|z| z.id.0.as_str()).collect()
}

#[test]
fn a_dropped_zone_draws_above_its_siblings_and_always_above_its_parent() {
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("z", 0, 2000, 200, 200, &[]),
            zone("p", 1000, 0, 1000, 1000, &["n1", "n2"]),
            zone("n1", 1100, 100, 300, 300, &[]),
            zone("n2", 1500, 100, 300, 300, &[]),
            zone("q", 3000, 0, 500, 500, &[]),
        ],
    );
    // z, drawn below p, nests in it: it draws above p and p's other nested zones.
    drag(&mut s, (50.0, 2010.0), (1450.0, 610.0));
    assert_eq!(order(&s), ["p", "n1", "n2", "z", "q"]);
    // A nested zone rises among its siblings only.
    drag(&mut s, (1150.0, 110.0), (1160.0, 110.0));
    assert_eq!(order(&s), ["p", "n2", "z", "n1", "q"]);
    // A top-level zone rises above its siblings, carrying its nested zones.
    drag(&mut s, (1050.0, 10.0), (1060.0, 10.0));
    assert_eq!(order(&s), ["q", "p", "n2", "z", "n1"]);
}

#[test]
fn while_a_zone_is_dragged_the_target_rings_only_when_nesting_would_change() {
    let mut s = session(
        &[("a", 0, 0)],
        vec![
            zone("p", 1000, 0, 1000, 1000, &[]),
            zone("z", 0, 2000, 200, 200, &[]),
            zone("h", 3000, 0, 400, 400, &["k"]),
            zone("k", 3100, 100, 100, 100, &[]),
        ],
    );
    pointer_at(&mut s, 0, (50.0, 2010.0));
    assert_eq!(ring(&s), None, "a press shows no ring");
    pointer_at(&mut s, 1, (1450.0, 610.0));
    assert_eq!(ring(&s), Some("p"));
    pointer_at(&mut s, 1, (60.0, 2010.0));
    assert_eq!(ring(&s), None, "staying loose");
    pointer_at(&mut s, 1, (1450.0, 610.0));
    pointer_at(&mut s, 2, (1450.0, 610.0));
    pointer_at(&mut s, 0, (1450.0, 610.0));
    pointer_at(&mut s, 1, (1460.0, 610.0));
    assert_eq!(ring(&s), None, "staying in p");
    pointer_at(&mut s, 2, (1460.0, 610.0));
    // h holds k: it never nests, so p never rings.
    pointer_at(&mut s, 0, (3300.0, 10.0));
    pointer_at(&mut s, 1, (1600.0, 310.0));
    assert_eq!(ring(&s), None, "a zone holding a nested zone");
}

#[test]
fn a_band_press_clears_the_selection_and_shift_passes_through_to_the_marquee() {
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let mut s = session(&[("a", 0, 0)], vec![zone("p", 1000, 0, 800, 600, &[])]);
    let p = s.document().zones[0].clone();
    s.select([id("a")]);
    pointer_at(&mut s, 0, (1500.0, 10.0));
    assert!(s.selection().is_empty(), "a press clears the selection");
    pointer_at(&mut s, 2, (1500.0, 10.0));

    s.select([id("a")]);
    pointer_with(&mut s, 0, (1500.0, 10.0), shift);
    pointer_with(&mut s, 1, (1700.0, 300.0), shift);
    assert!(s.marquee().is_some(), "Shift+drag draws a marquee");
    pointer_with(&mut s, 2, (1700.0, 300.0), shift);
    assert_eq!(s.selection(), [id("a")]);
    assert_eq!(s.document().zones[0], p, "the zone stays");

    pointer_with(&mut s, 0, (1500.0, 10.0), shift);
    pointer_with(&mut s, 2, (1500.0, 10.0), shift);
    assert_eq!(s.selection(), [id("a")], "Shift+click does nothing");
    assert_eq!(s.document().zones[0], p);
}

#[test]
fn the_ring_is_never_thinner_on_screen_than_the_theme_width() {
    let mut s = session(&[("a", 0, 0)], vec![zone("p", 1000, 0, 400, 600, &[])]);
    let ring = s.theme().zone.ring;
    for zoom in [0.1, 0.5, 1.0, 2.0] {
        set_zoom(&mut s, zoom);
        let frame = s.draw(Point::default(), at(&s, 4000.0, 4000.0), T);
        // Canvas units: the theme's width on screen zoomed out, scaling with the zone zoomed in.
        let screen = frame.zone_ring_width * zoom;
        assert!(
            (screen - ring * zoom.max(1.0)).abs() < 1e-4,
            "{zoom}x: {screen}"
        );
    }
}
