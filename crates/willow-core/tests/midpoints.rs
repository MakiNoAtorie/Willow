//! Midpoints: hovering a shape shows them, clicking one creates a linked shape in editing.
mod common;

use std::time::Duration;

use willow_core::{Button, Color, Cursor, Document, Input, Link, Modifiers, Point, Session};

const T: Duration = Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

// The new document's shape is 210 x 48.5 at screen x 395..605, y 376..424.5 at zoom 1.
const TOP: Point = Point { x: 500.0, y: 376.0 };
const BOTTOM: Point = Point { x: 500.0, y: 424.5 };
const LEFT: Point = Point {
    x: 395.0,
    y: 400.25,
};
const RIGHT: Point = Point {
    x: 605.0,
    y: 400.25,
};

/// The new document's shape, painted rose.
fn session() -> Session {
    let mut doc = Document::new();
    doc.shapes[0].color = Color::Rose;
    common::session_of(doc)
}

fn input(s: &mut Session, i: Input) -> willow_core::Response {
    s.handle(i, T)
}

fn hover(s: &mut Session, pos: Point) -> willow_core::Response {
    let mods = Modifiers::default();
    input(s, Input::PointerMove { pos, mods })
}

fn click(s: &mut Session, pos: Point) {
    let (button, mods) = (Button::Left, Modifiers::default());
    hover(s, pos);
    input(s, Input::PointerDown { pos, button, mods });
    input(s, Input::PointerUp { pos, button, mods });
}

#[test]
fn each_side_creates_a_linked_shape_one_gap_away_in_the_chain_color_in_editing() {
    // An empty shape is 48.5 tall; the creation gap is 48.
    for (mid, x, y) in [
        (BOTTOM, 0, 97),
        (TOP, 0, -97),
        (RIGHT, 258, 0),
        (LEFT, -258, 0),
    ] {
        let mut s = session();
        click(&mut s, mid);
        let doc = s.document();
        assert_eq!(doc.shapes.len(), 2, "{mid:?}");
        let (a, new) = (&doc.shapes[0], &doc.shapes[1]);
        assert_eq!((new.x, new.y, new.width), (x, y, 210), "{mid:?}");
        assert_eq!(new.color, Color::Rose);
        assert!(new.text.is_empty());
        assert_eq!(doc.links, [Link(a.id.clone(), new.id.clone())]);
        assert_eq!(s.editing().map(|e| &e.id), Some(&new.id));
        assert_eq!(s.selection(), std::slice::from_ref(&new.id));
    }
}

#[test]
fn a_side_already_linked_inserts_the_new_shape() {
    let mut s = session();
    click(&mut s, BOTTOM);
    input(&mut s, Input::Escape);
    click(&mut s, BOTTOM);
    let doc = s.document();
    assert_eq!(doc.shapes.len(), 3);
    let [a, b, c] = [0, 1, 2].map(|i| doc.shapes[i].id.clone());
    assert_eq!(doc.links, [Link(a, c.clone()), Link(c, b)]);
}

#[test]
fn one_undo_removes_the_new_shape_and_its_text() {
    let before = session().document().clone();
    let typed = |s: &mut Session| {
        click(s, RIGHT);
        let text = "Hi there".to_string();
        input(
            s,
            Input::Text {
                text,
                replace: None,
            },
        );
    };
    // Undo while still editing the new shape.
    let mut s = session();
    typed(&mut s);
    input(&mut s, Input::Undo);
    assert_eq!(*s.document(), before);
    assert!(s.editing().is_none());
    assert!(!s.can_undo());
    // Redo brings back the shape, link and text in one step.
    input(&mut s, Input::Redo);
    assert_eq!(s.document().shapes[1].text.plain(), "Hi there");
    assert_eq!(s.document().links.len(), 1);
    // Undo after editing ended.
    let mut s = session();
    typed(&mut s);
    input(&mut s, Input::Escape);
    input(&mut s, Input::Undo);
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
}

#[test]
fn a_new_shape_left_empty_is_kept_and_undoes_in_one_step() {
    let mut s = session();
    click(&mut s, BOTTOM);
    input(&mut s, Input::Escape);
    assert_eq!(s.document().shapes.len(), 2);
    input(&mut s, Input::Undo);
    assert_eq!(s.document().shapes.len(), 1);
    assert!(s.document().links.is_empty());
}

#[test]
fn hovering_shows_four_midpoints_at_constant_screen_size() {
    let mut s = session();
    assert!(hover(&mut s, pt(500.0, 400.0)).redraw);
    let mid = |s: &Session| s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T).midpoints;
    let shown: Vec<Point> = (mid(&s).iter())
        .map(|&p| s.viewport().to_screen(p))
        .collect();
    for p in [TOP, RIGHT, BOTTOM, LEFT] {
        assert!(shown.contains(&p), "{p:?} in {shown:?}");
    }
    // Off the shape, beyond the hit radius: gone.
    assert!(hover(&mut s, pt(500.0, 300.0)).redraw);
    assert!(mid(&s).is_empty());
    // Zoomed out, the hit radius stays 10 screen px: 9 px below the bottom still creates.
    let mut vp = s.viewport();
    vp.zoom = 0.5;
    s.set_viewport(vp);
    let bottom = s.viewport().to_screen(pt(105.0, 48.5));
    click(&mut s, pt(bottom.x, bottom.y + 9.0));
    assert_eq!(s.document().shapes.len(), 2);
}

#[test]
fn no_midpoints_on_the_shape_being_edited() {
    let mut s = session();
    click(&mut s, pt(500.0, 400.0));
    click(&mut s, pt(500.0, 400.0));
    assert!(s.editing().is_some());
    hover(&mut s, BOTTOM);
    assert!(
        s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T)
            .midpoints
            .is_empty()
    );
    click(&mut s, BOTTOM);
    assert_eq!(s.document().shapes.len(), 1);
}

#[test]
fn an_open_menu_or_picker_hides_the_midpoints() {
    let mid = |s: &Session| s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T).midpoints;
    let mut s = session();
    let (pos, button, mods) = (pt(500.0, 400.0), Button::Right, Modifiers::default());
    hover(&mut s, pos);
    assert_eq!(mid(&s).len(), 4);

    input(&mut s, Input::PointerDown { pos, button, mods });
    input(&mut s, Input::PointerUp { pos, button, mods });
    assert!(s.menu().is_some());
    assert!(mid(&s).is_empty(), "the context menu");
    input(&mut s, Input::Escape);
    assert_eq!(mid(&s).len(), 4);

    input(&mut s, Input::OpenColorPicker);
    assert!(mid(&s).is_empty(), "the color picker");
    input(&mut s, Input::Escape);

    input(&mut s, Input::Tab);
    assert!(mid(&s).is_empty(), "the Tab picker");
    input(&mut s, Input::Escape);
    assert_eq!(mid(&s).len(), 4);
}

#[test]
fn a_midpoint_hit_beats_the_resize_edge_under_it() {
    let mut s = session();
    // On the bottom edge at its midpoint: no resize cursor, and a click creates.
    assert_eq!(hover(&mut s, BOTTOM).cursor, Cursor::Arrow);
    assert_eq!(hover(&mut s, RIGHT).cursor, Cursor::Arrow);
    // Further along the edge, past the 10 px hit radius, the edge still resizes.
    assert_eq!(hover(&mut s, pt(RIGHT.x, 380.0)).cursor, Cursor::ResizeEw);
    click(&mut s, RIGHT);
    assert_eq!(s.document().shapes.len(), 2);
    assert_eq!(s.document().shapes[0].width, 210);
}

#[test]
fn dragging_off_a_midpoint_creates_nothing() {
    let mut s = session();
    let (button, mods) = (Button::Left, Modifiers::default());
    hover(&mut s, BOTTOM);
    input(
        &mut s,
        Input::PointerDown {
            pos: BOTTOM,
            button,
            mods,
        },
    );
    let pos = pt(500.0, 600.0);
    input(&mut s, Input::PointerMove { pos, mods });
    input(&mut s, Input::PointerUp { pos, button, mods });
    assert_eq!(s.document().shapes.len(), 1);
}

#[test]
fn the_create_animation_rises_scales_and_draws_the_connector_then_stops() {
    let mut s = session();
    click(&mut s, BOTTOM);
    let frame = |s: &Session, ms| {
        let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), Duration::from_millis(ms));
        let a = f.shapes[1].appear;
        let c = f.connectors[0];
        (a, c.to.y - c.from.y)
    };
    // Start: hidden, 8 units up, at 96%, connector not yet drawn.
    let (a, len) = frame(&s, 0);
    let a = a.expect("appearing");
    assert_eq!((a.opacity, a.dy, a.scale), (0.0, -8.0, 0.96));
    assert_eq!(len, 0.0);
    assert!(s.animating(Duration::from_millis(100)));
    // Done at 160 ms: drawn in full: 48.5 units of connector, 48.5 to 97.
    let (a, len) = frame(&s, 160);
    assert!(a.is_none());
    assert_eq!(len, 48.5);
    assert!(!s.animating(Duration::from_millis(160)));
    // Midpoints hide on the source until the pointer moves again.
    assert!(
        s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), T)
            .midpoints
            .is_empty()
    );
}

#[test]
fn a_linked_shape_is_centered_on_its_side_so_the_connector_runs_straight() {
    // A source taller and wider than a new shape: the new one centers on the side's midpoint.
    for side in [
        willow_core::Side::Right,
        willow_core::Side::Left,
        willow_core::Side::Bottom,
        willow_core::Side::Top,
    ] {
        let mut doc = Document::new();
        (doc.shapes[0].width, doc.shapes[0].extra_height) = (330, 120);
        let mut s = common::session_of(doc);
        let id = s.document().shapes[0].id.clone();
        s.select([id]);
        input(&mut s, Input::Tab);
        input(&mut s, Input::PickSide(side));
        let (a, b) = (&s.document().shapes[0], &s.document().shapes[1]);
        let (ah, bh) = (s.shape_height(a), s.shape_height(b));
        let center =
            |x: i32, y: i32, w: u32, h: f32| (x as f32 + w as f32 / 2.0, y as f32 + h / 2.0);
        let (ca, cb) = (center(a.x, a.y, a.width, ah), center(b.x, b.y, b.width, bh));
        match side {
            willow_core::Side::Right | willow_core::Side::Left => {
                assert!((ca.1 - cb.1).abs() <= 0.5, "{side:?}: {ca:?} vs {cb:?}")
            }
            _ => assert!((ca.0 - cb.0).abs() <= 0.5, "{side:?}: {ca:?} vs {cb:?}"),
        }
    }
}
