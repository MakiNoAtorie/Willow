mod common;

use common::{doc_with, session_of};
use willow_core::{Color, Document, Point, Rgba, Session, Shape, ShapeId, Side, Viewport, Zone};

const T: std::time::Duration = std::time::Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// The screen shows canvas (0, 0) at its top-left at zoom `z`.
fn look_at(s: &mut Session, z: f32) {
    s.set_viewport(Viewport {
        origin: pt(0.0, 0.0),
        zoom: z,
        size: pt(1000.0, 800.0),
    });
}

const SCREEN: (Point, Point) = (
    Point { x: 0.0, y: 0.0 },
    Point {
        x: 1000.0,
        y: 800.0,
    },
);

#[test]
fn text_fades_and_fills_deepen_between_75_and_50_percent() {
    let mut s = session_of(doc_with("Hello"));
    // (zoom, text opacity, deepening toward the picker color, shadows, grid)
    for (z, opacity, deepen, shadows, grid) in [
        (1.0, 1.0, 0.0, true, true),
        (0.75, 1.0, 0.0, true, true),
        (0.62, 0.48, 0.156, true, true),
        (0.5, 0.0, 0.3, true, true),
        (0.27, 0.0, 0.3, false, false),
    ] {
        look_at(&mut s, z);
        let f = s.draw(SCREEN.0, SCREEN.1, T);
        assert!(
            (f.text_opacity - opacity).abs() < 1e-4,
            "{z}: {}",
            f.text_opacity
        );
        assert!((f.deepen - deepen).abs() < 1e-4, "{z}: {}", f.deepen);
        assert_eq!((f.shadows, f.grid), (shadows, grid), "{z}");
    }
}

#[test]
fn a_deepened_fill_mixes_the_pastel_toward_the_picker_color() {
    let mut doc = doc_with("Hello");
    doc.shapes[0].color = Color::Rose;
    let mut s = session_of(doc);
    look_at(&mut s, 1.0);
    // Paper rose pastel #f3d8d3.
    assert_eq!(
        s.draw(SCREEN.0, SCREEN.1, T).shapes[0].fill,
        Rgba(0xf3d8d3ff)
    );
    look_at(&mut s, 0.27);
    // 70% #f3d8d3 + 30% picker #d9776c = (235.2, 186.9, 180.1).
    assert_eq!(
        s.draw(SCREEN.0, SCREEN.1, T).shapes[0].fill,
        Rgba(0xebbbb4ff)
    );
}

/// Shapes reading "Hello" (48.5 tall with the fake measure) at the given top-lefts.
fn shapes_at(tops: &[(i32, i32)]) -> Session {
    let mut doc = doc_with("Hello");
    let shape = doc.shapes.pop().expect("one shape");
    for (i, &(x, y)) in tops.iter().enumerate() {
        let mut s = shape.clone();
        (s.id.0, s.x, s.y) = (format!("s{i}"), x, y);
        doc.shapes.push(s);
    }
    session_of(doc)
}

fn drawn(s: &Session, from: Point, to: Point) -> Vec<String> {
    let f = s.draw(from, to, T);
    f.shapes.iter().map(|v| v.shape.id.0.clone()).collect()
}

#[test]
fn the_draw_query_returns_only_shapes_intersecting_the_rectangle() {
    let mut s = shapes_at(&[
        (0, 0),     // s0: inside
        (2000, 0),  // s1: right of it
        (0, -100),  // s2: above it, bottom at -51.5
        (0, -40),   // s3: top above it, bottom inside
        (-300, 0),  // s4: left of it, right edge at -90
        (0, 900),   // s5: below it
        (990, 790), // s6: overlaps the bottom-right corner
    ]);
    look_at(&mut s, 1.0);
    assert_eq!(drawn(&s, SCREEN.0, SCREEN.1), ["s0", "s3", "s6"]);
    // At 200% screen (0, 0)..(100, 100) is canvas (0, 0)..(50, 50).
    look_at(&mut s, 2.0);
    assert_eq!(drawn(&s, pt(0.0, 0.0), pt(100.0, 100.0)), ["s0", "s3"]);
}

#[test]
fn a_shape_being_resized_draws_at_its_previewed_size() {
    use willow_core::{Button, Input, Modifiers};
    let (mut s, mods, t) = (
        session_of(doc_with("Hello")),
        Modifiers::default(),
        std::time::Duration::ZERO,
    );
    // The centered new shape's right edge is at screen x 605; drag it 20 px right.
    let (from, to) = (pt(605.0, 400.0), pt(625.0, 400.0));
    let button = Button::Left;
    s.handle(
        Input::PointerDown {
            pos: from,
            button,
            mods,
        },
        t,
    );
    s.handle(Input::PointerMove { pos: to, mods }, t);
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    assert_eq!((f.shapes[0].x, f.shapes[0].shape.width), (-20, 250));
    assert_eq!(s.document().shapes[0].width, 210);
}

#[test]
fn the_edited_shape_carries_its_ring_selection_and_caret_and_low_zoom_drops_text_marks() {
    use willow_core::{Button, Input, Modifiers};
    let (mut s, mods) = (shapes_at(&[(0, 0), (100, 20)]), Modifiers::default());
    let ms = std::time::Duration::from_millis;
    look_at(&mut s, 1.0);
    // Select and edit s0 by clicking it twice where s1 doesn't cover it.
    let (pos, button) = (pt(10.0, 10.0), Button::Left);
    for _ in 0..2 {
        s.handle(Input::PointerDown { pos, button, mods }, ms(0));
        s.handle(Input::PointerUp { pos, button, mods }, ms(0));
    }
    s.handle(Input::SelectAll, ms(0));

    let f = s.draw(SCREEN.0, SCREEN.1, ms(500));
    let (a, b) = (&f.shapes[0], &f.shapes[1]);
    assert_eq!((a.ring, b.ring), (1.0, 0.0));
    assert_eq!(a.selection.len(), 1);
    assert!(a.caret.is_some() && b.caret.is_none() && b.selection.is_empty());

    // The caret is in its blink's hidden half at 530 ms.
    assert!(
        s.draw(SCREEN.0, SCREEN.1, ms(600)).shapes[0]
            .caret
            .is_none()
    );

    look_at(&mut s, 0.4);
    let f = s.draw(SCREEN.0, SCREEN.1, ms(500));
    assert!(f.shapes[0].caret.is_none() && f.shapes[0].selection.is_empty());
    assert_eq!(f.shapes[0].ring, 1.0);
}

#[test]
fn every_selected_shape_draws_the_selection_ring() {
    let mut s = shapes_at(&[(0, 0), (300, 0), (600, 0)]);
    look_at(&mut s, 1.0);
    let rings = |s: &Session| -> Vec<f32> {
        let f = s.draw(SCREEN.0, SCREEN.1, std::time::Duration::from_secs(5));
        f.shapes.iter().map(|v| v.ring).collect()
    };
    let sid = |n: &str| ShapeId(n.into());

    s.select([sid("s0"), sid("s2")]);
    assert_eq!(s.selection(), [sid("s0"), sid("s2")]);
    assert_eq!(rings(&s), [1.0, 0.0, 1.0]);
    s.select([sid("s1")]);
    assert_eq!(rings(&s), [0.0, 1.0, 0.0]);
    s.select([]);
    assert!(s.selection().is_empty());
}

fn zone(id: &str, x: i32, y: i32, color: Color, members: &[&str]) -> Zone {
    Zone {
        id: ShapeId(id.into()),
        title: format!("Zone {id}"),
        color,
        x,
        y,
        width: 600,
        height: 400,
        members: members.iter().map(|m| ShapeId((*m).into())).collect(),
    }
}

/// Empty shapes a (0, 0), b (300, 100) and c (0, 200), 48.5 tall; a-b and c-a linked; zone y
/// (sky) holds a, b and zone z (sage), which holds c.
fn linked_and_zoned() -> Session {
    let mut doc = Document::default();
    for (i, x, y) in [("a", 0, 0), ("b", 300, 100), ("c", 0, 200)] {
        doc.shapes.push(Shape::new(ShapeId(i.into()), x, y));
    }
    let id = |i: &str| ShapeId(i.into());
    assert!(doc.link(&id("a"), &id("b")) && doc.link(&id("c"), &id("a")));
    doc.zones
        .push(zone("y", -40, -40, Color::Sky, &["a", "b", "z"]));
    doc.zones.push(zone("z", -20, 180, Color::Sage, &["c"]));
    session_of(doc)
}

#[test]
fn connectors_join_facing_side_midpoints() {
    let mut s = linked_and_zoned();
    look_at(&mut s, 1.0);
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    let ends: Vec<_> = (f.connectors.iter())
        .map(|c| ((c.from, c.from_side), (c.to, c.to_side)))
        .collect();
    assert_eq!(
        ends,
        [
            // a-b: centers 300 apart across, 100 down: horizontal.
            (
                (pt(210.0, 24.25), Side::Right),
                (pt(300.0, 124.25), Side::Left)
            ),
            // c-a: c is straight below a.
            (
                (pt(105.0, 200.0), Side::Top),
                (pt(105.0, 48.5), Side::Bottom)
            ),
        ]
    );
    assert_eq!(f.connector_width, 2.0);
    // Never thinner than one screen pixel.
    look_at(&mut s, 0.25);
    assert_eq!(s.draw(SCREEN.0, SCREEN.1, T).connector_width, 4.0);
}

#[test]
fn zones_draw_parents_first_with_pastel_fills_and_titles_readable_when_zoomed_out() {
    let mut s = linked_and_zoned();
    look_at(&mut s, 1.0);
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    let z: Vec<_> = (f.zones.iter())
        .map(|v| (v.zone.id.0.as_str(), v.nested, v.fill, v.edge))
        .collect();
    // Sky #d3e0ea 45% into canvas #f6f3ec, edge 35% toward picker #5d8fbf; nested sage
    // #d9e5cf 70% into the canvas, edge 35% toward #7ea468.
    assert_eq!(
        z,
        [
            ("y", false, Rgba(0xe6eaebff), Rgba(0xaac4dbff)),
            ("z", true, Rgba(0xe2e9d8ff), Rgba(0xb9ceabff)),
        ]
    );
    let y = &f.zones[0];
    assert_eq!((y.title_at, y.title_size), (pt(-24.0, -28.0), 13.0));
    // At 50% the title stays 12 screen px and its inset 16, 12 screen px.
    look_at(&mut s, 0.5);
    let y = &s.draw(SCREEN.0, SCREEN.1, T).zones[0];
    assert_eq!((y.title_at, y.title_size), (pt(-8.0, -16.0), 24.0));
    // Off screen, nothing.
    let f = s.draw(pt(2000.0, 2000.0), pt(2100.0, 2100.0), T);
    assert!(f.zones.is_empty() && f.connectors.is_empty());
}

#[test]
fn a_generated_document_draws_its_connectors_and_zones() {
    let mut s = session_of(willow_core::generate(1000, 1));
    look_at(&mut s, 0.25);
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    assert!(f.connectors.len() > 20);
    assert!(f.zones.iter().any(|z| z.nested) && f.zones.iter().any(|z| !z.nested));
    let opposite = |a: Side| match a {
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    };
    assert!((f.connectors.iter()).all(|c| c.to_side == opposite(c.from_side)));
}
