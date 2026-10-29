//! Zones resize from their four edges and four corners.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Cursor, Document, Input, Modifiers, Point, Session, Shape, ShapeId, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(name: &str, x: i32, y: i32, w: u32, h: u32, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width: w,
        height: h,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

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

/// Pointer down (0), move (1) or up (2) with the left button at canvas point `c`.
fn pointer(s: &mut Session, kind: u8, c: (f32, f32)) -> Cursor {
    let (pos, button, mods) = (at(s, c.0, c.1), Button::Left, Modifiers::default());
    let input = match kind {
        0 => Input::PointerDown { pos, button, mods },
        1 => Input::PointerMove { pos, mods },
        _ => Input::PointerUp { pos, button, mods },
    };
    s.handle(input, T).cursor
}

fn drag(s: &mut Session, from: (f32, f32), d: (f32, f32)) {
    pointer(s, 0, from);
    pointer(s, 1, (from.0 + d.0, from.1 + d.1));
    pointer(s, 2, (from.0 + d.0, from.1 + d.1));
}

/// Zone `p`'s x, y, width, height.
fn rect(s: &Session) -> (i32, i32, u32, u32) {
    let z = &s.document().zones[0];
    (z.x, z.y, z.width, z.height)
}

// Zone p is 1000,1000 400x300. Edge points sit clear of its title band and the corners.
fn lone() -> Session {
    session(&[], vec![zone("p", 1000, 1000, 400, 300, &[])])
}

#[test]
fn every_edge_and_corner_resizes_and_top_and_left_move_the_position() {
    // (grab point, drag, expected rect)
    let cases = [
        ((1000.0, 1150.0), (-30.0, 0.0), (970, 1000, 430, 300)),
        ((1400.0, 1150.0), (20.0, 0.0), (1000, 1000, 420, 300)),
        ((1200.0, 1000.0), (0.0, 25.0), (1000, 1025, 400, 275)),
        ((1200.0, 1300.0), (0.0, 40.0), (1000, 1000, 400, 340)),
        ((1000.0, 1000.0), (10.0, 15.0), (1010, 1015, 390, 285)),
        ((1400.0, 1000.0), (-10.0, -15.0), (1000, 985, 390, 315)),
        ((1000.0, 1300.0), (-10.0, 15.0), (990, 1000, 410, 315)),
        ((1400.0, 1300.0), (10.0, 15.0), (1000, 1000, 410, 315)),
    ];
    for (from, d, want) in cases {
        let mut s = lone();
        drag(&mut s, from, d);
        assert_eq!(rect(&s), want, "grab {from:?}");
    }
}

#[test]
fn zoomed_drags_resize_in_whole_canvas_units() {
    let mut s = lone();
    let mut vp = s.viewport();
    vp.zoom = 2.0;
    s.set_viewport(vp);
    // 20 screen px at 2x is 10 units.
    drag(&mut s, (1400.0, 1150.0), (10.0, 0.0));
    assert_eq!(rect(&s), (1000, 1000, 410, 300));
}

#[test]
fn the_minimum_size_holds_and_members_and_nested_zones_stay() {
    let mut s = session(
        &[("a", 1050, 1100), ("b", 1200, 1100)],
        vec![
            zone("p", 1000, 1000, 400, 300, &["a", "n"]),
            zone("n", 1150, 1050, 200, 200, &["b"]),
        ],
    );
    // Drag the bottom-right corner far past the top-left corner, then the top-left far past
    // the bottom-right: 2 * pad_x + font size = 47 each way, anchored on the far edge.
    drag(&mut s, (1400.0, 1300.0), (-900.0, -900.0));
    assert_eq!(rect(&s), (1000, 1000, 47, 47));
    drag(&mut s, (1000.0, 1000.0), (500.0, 500.0));
    assert_eq!(rect(&s), (1000, 1000, 47, 47));
    let doc = s.document();
    let moved: Vec<_> = doc.shapes.iter().map(|s| (s.x, s.y)).collect();
    assert_eq!(moved, [(1050, 1100), (1200, 1100)]);
    assert_eq!(doc.zones[0].members, [id("a"), id("n")]);
    assert_eq!(doc.zones[1].members, [id("b")]);
    assert_eq!((doc.zones[1].x, doc.zones[1].width), (1150, 200));
}

#[test]
fn a_resize_is_one_undo_step_previewed_until_release() {
    let mut s = lone();
    pointer(&mut s, 0, (1400.0, 1150.0));
    pointer(&mut s, 1, (1450.0, 1150.0));
    assert_eq!(
        rect(&s),
        (1000, 1000, 400, 300),
        "the document holds the old size"
    );
    let frame = s.draw(Point::default(), at(&s, 4000.0, 4000.0), T);
    assert_eq!(
        frame.zones[0].zone.width, 450,
        "the draw query shows the new size"
    );
    pointer(&mut s, 1, (1460.0, 1150.0));
    pointer(&mut s, 2, (1460.0, 1150.0));
    assert_eq!(rect(&s), (1000, 1000, 460, 300));
    s.handle(Input::Undo, T);
    assert_eq!(rect(&s), (1000, 1000, 400, 300));
    s.handle(Input::Redo, T);
    assert_eq!(rect(&s), (1000, 1000, 460, 300));
}

#[test]
fn hovering_each_edge_and_corner_gives_its_resize_cursor_and_the_edge_wins_the_band() {
    let mut s = lone();
    let cases = [
        ((1000.0, 1150.0), Cursor::ResizeEw),
        ((1400.0, 1150.0), Cursor::ResizeEw),
        ((1200.0, 1000.0), Cursor::ResizeNs),
        ((1200.0, 1300.0), Cursor::ResizeNs),
        ((1000.0, 1000.0), Cursor::ResizeNwse),
        ((1400.0, 1300.0), Cursor::ResizeNwse),
        ((1400.0, 1000.0), Cursor::ResizeNesw),
        ((1000.0, 1300.0), Cursor::ResizeNesw),
        ((1200.0, 1150.0), Cursor::Arrow),
        ((1200.0, 1010.0), Cursor::Arrow),
    ];
    for (c, want) in cases {
        assert_eq!(pointer(&mut s, 1, c), want, "{c:?}");
    }
    // On the top edge the band does not ring; one step below it does.
    pointer(&mut s, 1, (1200.0, 1000.0));
    let ring = |s: &Session| {
        s.draw(Point::default(), at(s, 4000.0, 4000.0), T)
            .zone_ring
            .is_some()
    };
    assert!(!ring(&s));
    pointer(&mut s, 1, (1200.0, 1010.0));
    assert!(ring(&s));
    // Pressing the top edge resizes instead of moving: the drag changes the height.
    drag(&mut s, (1200.0, 1000.0), (0.0, 20.0));
    assert_eq!(rect(&s), (1000, 1020, 400, 280));
}

#[test]
fn the_cursor_holds_during_a_resize_and_a_press_clears_the_selection() {
    let mut s = session(
        &[("a", 100, 100)],
        vec![zone("p", 1000, 1000, 400, 300, &[])],
    );
    s.select([id("a")]);
    assert_eq!(pointer(&mut s, 0, (1400.0, 1300.0)), Cursor::ResizeNwse);
    assert!(s.selection().is_empty());
    assert_eq!(pointer(&mut s, 1, (1500.0, 1500.0)), Cursor::ResizeNwse);
}

#[test]
fn a_shape_over_a_zone_edge_wins() {
    // Shape a (210 wide) covers p's left edge at y 1120.
    let mut s = session(
        &[("a", 950, 1100)],
        vec![zone("p", 1000, 1000, 400, 300, &[])],
    );
    assert_eq!(pointer(&mut s, 1, (1000.0, 1120.0)), Cursor::Arrow);
    drag(&mut s, (1000.0, 1120.0), (-40.0, 0.0));
    assert_eq!(rect(&s), (1000, 1000, 400, 300));
}
