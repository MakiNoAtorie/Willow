//! Every read of what the writer sees goes through the drawn rectangle. These set a shape's
//! offset by hand: a read of the placed position would pass most other tests.

use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

use super::{DrawnRect, Session, Viewport};
use crate::{
    Button, Color, Cursor, Document, Input, Link, Measure, Modifiers, PAPER, Paragraph, Point,
    Shape, ShapeId, Side, Theme, Zone,
};

/// Ten units per cluster.
struct Tens;

impl Measure for Tens {
    fn advances(&self, _: &Paragraph, clusters: &[Range<usize>], _: &Theme) -> Vec<f32> {
        vec![10.0; clusters.len()]
    }
}

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Shapes `a`, `b`, … at the given top-lefts, with `links`, the screen at canvas (0, 0).
fn session(at: &[(i32, i32)], links: &[(&str, &str)]) -> Session {
    let mut doc = Document::default();
    for &(x, y) in at {
        doc.shapes.push(Shape::new(doc.next_id(), x, y));
    }
    doc.links = (links.iter()).map(|(a, b)| Link(id(a), id(b))).collect();
    let mut s = Session::new(doc, &PAPER, Box::new(Tens));
    s.resize(1000.0, 800.0);
    s.set_viewport(Viewport {
        origin: pt(0.0, 0.0),
        zoom: 1.0,
        size: pt(1000.0, 800.0),
    });
    s
}

/// Draws `who` off its placed position by (dx, dy).
fn shift(s: &mut Session, who: &str, dx: i32, dy: i32) {
    s.drawn_offsets.insert(id(who), (dx, dy));
}

#[test]
fn the_drawn_rectangle_is_the_placed_one_and_follows_the_offset() {
    let mut s = session(&[(40, 60)], &[]);
    let h = s.shape_height(&s.document().shapes[0]);
    let r = s.drawn_rect(&id("a")).expect("a shape");
    assert_eq!((r.x, r.y, r.width, r.height), (40.0, 60.0, 210.0, h));
    assert_eq!(r.center(), pt(145.0, 60.0 + h / 2.0));
    assert_eq!((r.right(), r.bottom()), (250.0, 60.0 + h));

    shift(&mut s, "a", 300, -20);
    let r = s.drawn_rect(&id("a")).expect("a shape");
    assert_eq!((r.x, r.y), (340.0, 40.0));
    let a = &s.document().shapes[0];
    assert_eq!((a.x, a.y), (40, 60));

    assert!(s.drawn_rect(&id("zz")).is_none());
}

const T: Duration = Duration::ZERO;
const SCREEN: (Point, Point) = (
    Point { x: 0.0, y: 0.0 },
    Point {
        x: 1000.0,
        y: 800.0,
    },
);

fn press(s: &mut Session, p: Point) {
    s.handle(
        Input::PointerDown {
            pos: p,
            button: Button::Left,
            mods: Modifiers::default(),
        },
        T,
    );
}

fn pointer(s: &mut Session, p: Point) -> Cursor {
    let mods = Modifiers::default();
    s.handle(Input::PointerMove { pos: p, mods }, T).cursor
}

fn release(s: &mut Session, p: Point) {
    s.handle(
        Input::PointerUp {
            pos: p,
            button: Button::Left,
            mods: Modifiers::default(),
        },
        T,
    );
}

fn rect(s: &Session, who: &str) -> DrawnRect {
    s.drawn_rect(&id(who)).expect("a shape")
}

fn placed(s: &Session, who: &str) -> (i32, i32) {
    let found = s.document().shapes.iter().find(|x| x.id == id(who));
    found.map(|x| (x.x, x.y)).expect("a shape")
}

fn painted(s: &Session, who: &str) -> Option<(i32, i32)> {
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    let v = f.shapes.iter().find(|v| v.shape.id == id(who));
    v.map(|v| (v.x, v.y))
}

#[test]
fn painting_reads_drawn_positions_and_culls_on_them() {
    let mut s = session(&[(0, 0), (5000, 0), (10, 10)], &[]);
    shift(&mut s, "a", 5000, 0);
    shift(&mut s, "b", -4900, 40);
    shift(&mut s, "c", 300, 40);
    assert_eq!(painted(&s, "a"), None);
    assert_eq!(painted(&s, "b"), Some((100, 40)));
    assert_eq!(painted(&s, "c"), Some((310, 50)));
}

#[test]
fn connectors_and_their_sides_read_drawn_positions() {
    let mut s = session(&[(0, 0), (0, 200)], &[("a", "b")]);
    shift(&mut s, "b", 600, 0);
    let (a, b) = (rect(&s, "a"), rect(&s, "b"));
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    let c = f.connectors[0];
    assert_eq!((c.from_side, c.to_side), (Side::Right, Side::Left));
    assert_eq!(c.from, pt(a.right(), a.center().y));
    assert_eq!(c.to, pt(b.x, b.center().y));
}

#[test]
fn connectors_cull_on_drawn_positions() {
    let mut s = session(&[(5000, 0), (5000, 200)], &[("a", "b")]);
    shift(&mut s, "a", -5000, 0);
    shift(&mut s, "b", -5000, 0);
    assert_eq!(s.draw(SCREEN.0, SCREEN.1, T).connectors.len(), 1);
}

#[test]
fn a_press_hits_the_shape_where_it_is_drawn() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let c = rect(&s, "a").center();
    press(&mut s, pt(c.x, c.y));
    release(&mut s, pt(c.x, c.y));
    assert_eq!(s.selection(), [id("a")]);

    s.select([]);
    press(&mut s, pt(50.0, 10.0));
    release(&mut s, pt(50.0, 10.0));
    assert!(s.selection().is_empty());
}

#[test]
fn resize_edges_are_where_the_shape_is_drawn() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let r = rect(&s, "a");
    // Near a corner: a side's midpoint wins over its edge.
    assert_eq!(pointer(&mut s, pt(r.right(), r.y + 2.0)), Cursor::ResizeEw);
    assert_eq!(pointer(&mut s, pt(210.0, 2.0)), Cursor::Arrow);
}

#[test]
fn a_resize_previews_from_where_the_shape_is_drawn() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let r = rect(&s, "a");
    press(&mut s, pt(r.right(), r.y + 2.0));
    pointer(&mut s, pt(r.right() + 20.0, r.y + 2.0));
    assert_eq!(painted(&s, "a"), Some((280, 100)));
    release(&mut s, pt(r.right() + 20.0, r.y + 2.0));
    assert_eq!(placed(&s, "a"), (-20, 0));
}

#[test]
fn midpoints_show_and_hit_where_the_shape_is_drawn() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let r = rect(&s, "a");
    let bottom = pt(r.center().x, r.bottom());
    pointer(&mut s, r.center());
    let shown = s.draw(SCREEN.0, SCREEN.1, T).midpoints;
    assert!(shown.contains(&bottom), "{shown:?}");

    press(&mut s, bottom);
    pointer(&mut s, pt(bottom.x, bottom.y + 50.0));
    let band = s
        .draw(SCREEN.0, SCREEN.1, T)
        .link_drag
        .expect("a link drag");
    assert_eq!(band.from, bottom);
}

#[test]
fn the_pointer_finds_a_connector_where_it_is_drawn() {
    let mut s = session(&[(0, 0), (0, 200)], &[("a", "b")]);
    shift(&mut s, "a", 0, 300);
    shift(&mut s, "b", 600, 100);
    let h = rect(&s, "a").height;
    pointer(&mut s, pt(405.0, 300.0 + h / 2.0));
    assert!(s.draw(SCREEN.0, SCREEN.1, T).connectors[0].hovered);
    pointer(&mut s, pt(105.0, (h + 200.0) / 2.0));
    assert!(!s.draw(SCREEN.0, SCREEN.1, T).connectors[0].hovered);
}

#[test]
fn the_marquee_touches_drawn_rectangles() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let shift_held = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let mark = |s: &mut Session, from: Point, to: Point| {
        let (pos, button, mods) = (from, Button::Left, shift_held);
        s.handle(Input::PointerDown { pos, button, mods }, T);
        s.handle(Input::PointerMove { pos: to, mods }, T);
        s.handle(
            Input::PointerUp {
                pos: to,
                button,
                mods,
            },
            T,
        );
    };
    mark(&mut s, pt(600.0, 10.0), pt(800.0, 90.0));
    assert!(s.selection().is_empty(), "a stays clear of the marquee");
    mark(&mut s, pt(0.0, 300.0), pt(100.0, 500.0));
    assert!(s.selection().is_empty(), "nothing at a's placed position");
    mark(&mut s, pt(520.0, 120.0), pt(480.0, 300.0));
    assert_eq!(s.selection(), [id("a")]);
}

#[test]
fn the_tab_picker_hangs_from_the_drawn_rectangle() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    s.select([id("a")]);
    s.handle(Input::Tab, T);
    let r = rect(&s, "a");
    let picker = s.draw(SCREEN.0, SCREEN.1, T).tab_picker;
    assert!(picker.contains(&(Side::Top, pt(r.center().x, r.y))));
}

#[test]
fn the_color_picker_opens_over_the_drawn_rectangle() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    s.select([id("a")]);
    s.handle(Input::OpenColorPicker, T);
    let m = s.menu().expect("the picker");
    assert_eq!(m.at, pt(300.0, 100.0));
}

#[test]
fn the_caret_and_text_selection_sit_in_the_drawn_rectangle() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    s.select([id("a")]);
    s.handle(Input::StartEditing, T);
    let (text, replace) = ("abc".to_string(), None);
    s.handle(Input::Text { text, replace }, T);
    let (px, py) = (PAPER.shape.pad_x, PAPER.shape.pad_y);
    let at = s.caret(T).expect("editing").pos;
    assert_eq!(at, pt(300.0 + px + 30.0, 100.0 + py));
    assert_eq!(s.text_point(0), Some(pt(300.0 + px, 100.0 + py)));

    s.set_text_selection(0..2);
    let rects = s.selection_rects();
    assert_eq!(rects[0].0, pt(300.0 + px, 100.0 + py));

    // A press in the drawn text puts the caret under it.
    let c = pt(300.0 + px + 11.0, 100.0 + py + 2.0);
    press(&mut s, c);
    release(&mut s, c);
    assert_eq!(s.caret(T).expect("editing").pos.x, 300.0 + px + 10.0);
}

#[test]
fn a_drag_previews_from_where_the_shape_is_drawn_and_moves_by_what_the_pointer_did() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let c = rect(&s, "a").center();
    press(&mut s, c);
    pointer(&mut s, pt(c.x + 30.0, c.y + 10.0));
    assert_eq!(painted(&s, "a"), Some((330, 110)));
    release(&mut s, pt(c.x + 30.0, c.y + 10.0));
    assert_eq!(placed(&s, "a"), (30, 10));
}

#[test]
fn a_group_drag_previews_every_shape_from_where_it_is_drawn() {
    let mut s = session(&[(0, 0), (0, 300)], &[]);
    shift(&mut s, "a", 300, 100);
    shift(&mut s, "b", 300, -100);
    s.select([id("a"), id("b")]);
    let c = rect(&s, "a").center();
    press(&mut s, c);
    pointer(&mut s, pt(c.x + 30.0, c.y + 10.0));
    assert_eq!(s.group_drag(), Some((30, 10)));
    assert_eq!(painted(&s, "a"), Some((330, 110)));
    assert_eq!(painted(&s, "b"), Some((330, 210)));
    release(&mut s, pt(c.x + 30.0, c.y + 10.0));
    assert_eq!((placed(&s, "a"), placed(&s, "b")), ((30, 10), (30, 310)));
}

/// Adds `zone` at (x, y), 300 square, holding `members`.
fn add_zone(s: &mut Session, zone: &str, x: i32, y: i32, members: &[&str]) {
    s.doc.zones.push(Zone {
        id: id(zone),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width: 300,
        height: 300,
        members: members.iter().map(|m| id(m)).collect(),
    });
}

#[test]
fn a_zone_drag_previews_its_members_from_where_they_are_drawn() {
    let mut s = session(&[(100, 100)], &[]);
    add_zone(&mut s, "z", 0, 0, &["a"]);
    shift(&mut s, "a", 300, 100);
    press(&mut s, pt(150.0, 10.0));
    pointer(&mut s, pt(180.0, 20.0));
    assert_eq!(painted(&s, "a"), Some((430, 210)));
}

#[test]
fn a_drop_target_is_judged_from_where_the_shape_is_drawn() {
    let mut s = session(&[(0, 0)], &[]);
    add_zone(&mut s, "z", 400, 0, &[]);
    shift(&mut s, "a", 450, 100);
    let c = rect(&s, "a").center();
    press(&mut s, c);
    pointer(&mut s, pt(c.x + 5.0, c.y));
    assert_eq!(
        s.draw(SCREEN.0, SCREEN.1, T).zone_ring,
        Some(&id("z")),
        "the drawn center is in the zone, the placed one isn't"
    );
    release(&mut s, pt(c.x + 5.0, c.y));
    assert_eq!(s.document().parent_zone(&id("a")), Some(&id("z")));
}

#[test]
fn arrow_keys_go_by_drawn_centers() {
    let mut s = session(&[(0, 0), (0, 300)], &[]);
    shift(&mut s, "b", 600, -300);
    s.select([id("a")]);
    s.handle(Input::Navigate(Side::Bottom), T);
    assert_eq!(s.selection(), [id("a")], "b is drawn to the right");
    s.handle(Input::Navigate(Side::Right), T);
    assert_eq!(s.selection(), [id("b")]);
}

#[test]
fn keeping_a_shape_in_view_reads_its_drawn_rectangle() {
    let mut s = session(&[(0, 0), (0, 200)], &[]);
    shift(&mut s, "b", 3000, 0);
    s.select([id("a")]);
    s.handle(Input::Navigate(Side::Right), T);
    s.tick(Duration::from_secs(5));
    let (r, vp) = (rect(&s, "b"), s.viewport());
    assert!(
        vp.origin.x <= r.x && r.right() <= vp.origin.x + vp.size.x,
        "{vp:?}"
    );
}

#[test]
fn enter_with_nothing_selected_takes_the_nearest_drawn_center() {
    let mut s = session(&[(0, 0), (3000, 3000)], &[]);
    shift(&mut s, "b", -2600, -2600);
    s.handle(Input::Enter, T);
    assert_eq!(s.selection(), [id("b")]);
}

#[test]
fn the_first_view_centers_the_first_shape_where_it_is_drawn() {
    let doc = Document {
        shapes: vec![Shape::new(id("a"), 0, 0)],
        ..Document::default()
    };
    let mut s = Session::new(doc, &PAPER, Box::new(Tens));
    shift(&mut s, "a", 300, 100);
    s.resize(1000.0, 800.0);
    let (r, vp) = (rect(&s, "a"), s.viewport());
    assert_eq!(vp.origin.x, 300.0 - 395.0);
    assert_eq!(vp.origin.y, 100.0 - ((800.0 - r.height) / 2.0).round());
}

#[test]
fn deleting_reconnects_by_the_sides_the_writer_sees() {
    // Placed in a column, b's neighbors are above and below it; c is drawn to its right.
    let mut s = session(&[(0, 0), (0, 200), (0, 400)], &[("a", "b"), ("b", "c")]);
    shift(&mut s, "c", 800, -200);
    s.delete(&[id("b")]);
    assert!(s.document().links.is_empty(), "{:?}", s.document().links);

    // Placed in a row, drawn in a column, they reconnect.
    let mut s = session(&[(0, 0), (300, 0), (600, 0)], &[("a", "b"), ("b", "c")]);
    shift(&mut s, "a", 300, -300);
    shift(&mut s, "c", -300, 300);
    s.delete(&[id("b")]);
    assert_eq!(s.document().links, [Link(id("a"), id("c"))]);
}

#[test]
fn deleting_picks_the_hub_by_drawn_distance() {
    // Two neighbors each side, so the hub is above. Placed, c is the nearer; drawn, a is.
    let links = [("a", "b"), ("c", "b"), ("b", "d"), ("b", "e")];
    let at = [(0, 0), (0, 500), (0, 300), (0, 700), (0, 900)];
    let mut s = session(&at, &links);
    shift(&mut s, "a", 0, 380);
    s.delete(&[id("b")]);
    let links = &s.document().links;
    assert!(links.contains(&Link(id("a"), id("d"))), "{links:?}");
    assert!(links.contains(&Link(id("a"), id("e"))), "{links:?}");
}

#[test]
fn the_clipboard_text_carries_drawn_positions() {
    let mut s = session(&[(0, 0), (0, 200)], &[("a", "b")]);
    shift(&mut s, "b", 600, 0);
    s.select([id("a"), id("b")]);
    let text = s.clipboard_text().expect("a selection");
    assert!(text.contains("@layout b 600,200 w210"), "{text}");
    assert!(text.contains("right of a"), "{text}");
    // The document itself keeps its placed position.
    assert_eq!(placed(&s, "b"), (0, 200));

    // Paste keeps what was copied: the copies stand apart as the originals are drawn.
    s.handle(Input::Paste(text), T);
    let copies = s.selection().to_vec();
    let at: Vec<_> = (copies.iter()).map(|c| placed(&s, &c.0)).collect();
    assert_eq!((at[1].0 - at[0].0, at[1].1 - at[0].1), (600, 200));
}

#[test]
fn duplicate_copies_beside_the_drawn_bounding_box() {
    let mut s = session(&[(0, 0), (0, 200)], &[]);
    shift(&mut s, "b", 600, 40);
    s.select([id("a"), id("b")]);
    s.handle(Input::Duplicate, T);
    let dx = 810 + PAPER.creation_gap.round() as i32;
    let copies = s.selection().to_vec();
    assert_eq!(placed(&s, &copies[0].0), (dx, 0));
    assert_eq!(placed(&s, &copies[1].0), (600 + dx, 240));
}

#[test]
fn create_zone_wraps_drawn_rectangles() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    s.select([id("a")]);
    s.create_zone();
    let (z, pad) = (&s.document().zones[0], PAPER.zone.padding);
    assert_eq!((z.x, z.y), ((300.0 - pad) as i32, (100.0 - pad) as i32));
}

#[test]
fn a_new_shape_beside_the_chain_goes_by_what_the_chain_shows() {
    let mut s = session(&[(0, 0), (0, 200)], &[("a", "b")]);
    shift(&mut s, "a", 300, 50);
    shift(&mut s, "b", 300, 0);
    s.select([id("a")]);
    s.handle(Input::NewBeside, T);
    let new = &s.document().shapes[2];
    let right = 300 + 210 + PAPER.creation_gap.round() as i32;
    assert_eq!((new.x, new.y), (right, 50));
}

#[test]
fn a_midpoint_insert_finds_the_neighbor_on_the_side_the_writer_sees() {
    // b is placed below a but drawn to its right: a new shape below a isn't an insert.
    let mut s = session(&[(0, 0), (0, 300)], &[("a", "b")]);
    shift(&mut s, "b", 800, -300);
    s.create_linked(&id("a"), Side::Bottom);
    let links = &s.document().links;
    assert!(links.contains(&Link(id("a"), id("b"))), "{links:?}");
}

#[test]
fn a_linked_shape_is_placed_from_the_sources_placed_position() {
    let mut s = session(&[(0, 0)], &[]);
    shift(&mut s, "a", 300, 100);
    let h = rect(&s, "a").height;
    s.create_linked(&id("a"), Side::Bottom);
    let gap = PAPER.creation_gap;
    assert_eq!(placed(&s, "b"), (0, (h + gap).round() as i32));
}

#[test]
fn a_direction_word_is_compared_with_the_side_the_document_shows() {
    let mut s = session(&[(0, 0), (0, 300)], &[("a", "b")]);
    shift(&mut s, "a", 0, 1000);
    shift(&mut s, "b", 800, 700);
    let mut new = s.document().clone();
    let at: HashMap<_, _> = (s.document().shapes.iter().enumerate())
        .map(|(i, x)| (&x.id, i))
        .collect();
    s.replace_by_word(&mut new, &[None, Some((0, Side::Right))], &at);
    let b = &new.shapes[1];
    assert_eq!((b.x, b.y), (0, 300), "b already shows right of a");
}

#[test]
fn a_click_where_shapes_overlap_hits_the_later_one() {
    // Nothing overlaps at rest any more; a slide or a hand-set offset can.
    let mut s = session(&[(0, 0), (900, 500)], &[]);
    shift(&mut s, "b", -800, -480);
    press(&mut s, pt(150.0, 30.0));
    release(&mut s, pt(150.0, 30.0));
    assert_eq!(s.selection(), [id("b")]);
}
