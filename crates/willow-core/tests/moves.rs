//! Moves land where released (ADR 0006): a chain a move touches takes its drawn positions as
//! placed, then the movement is added. Everything here starts from a pushed position, which
//! the note's offset proves: a read of the placed position lands elsewhere.
mod common;

use std::time::Duration;

use willow_core::{
    Button, Color, Document, Input, Link, Modifiers, Point, Session, Shape, ShapeId, Side,
    Viewport, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Shapes `(id, x, y)` linked as `links`, canvas (0, 0) at the screen's corner.
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    let doc = Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    };
    let mut s = common::session_of(doc);
    s.set_viewport(Viewport {
        origin: Point::default(),
        zoom: 1.0,
        size: Point {
            x: 2000.0,
            y: 2000.0,
        },
    });
    s
}

/// The trunk t-u-v, which holds still, and the note `n` placed on it and so pushed.
fn pushed_note() -> Session {
    let s = session(
        &[("t", 0, 0), ("u", 0, 97), ("v", 0, 194), ("n", 100, 60)],
        &[("t", "u"), ("u", "v")],
    );
    assert_ne!(drawn(&s, "n"), (100, 60), "the note is pushed");
    s
}

/// The trunk t-u-v and the smaller chain x-y, placed on it and so pushed.
fn pushed_chain() -> Session {
    let s = session(
        &[
            ("t", 0, 0),
            ("u", 0, 97),
            ("v", 0, 194),
            ("x", 100, 60),
            ("y", 100, 157),
        ],
        &[("t", "u"), ("u", "v"), ("x", "y")],
    );
    assert_ne!(drawn(&s, "y"), (100, 157), "the chain is pushed");
    s
}

fn drawn(s: &Session, name: &str) -> (i32, i32) {
    let r = s.drawn_rect(&id(name)).expect(name);
    (r.x as i32, r.y as i32)
}

fn placed(s: &Session, name: &str) -> (i32, i32) {
    let sh = s.document().shapes.iter().find(|x| x.id.0 == name);
    sh.map(|x| (x.x, x.y)).expect(name)
}

fn plus((x, y): (i32, i32), (dx, dy): (i32, i32)) -> (i32, i32) {
    (x + dx, y + dy)
}

/// Presses `name` near its top-left as drawn and moves the pointer by (dx, dy).
fn drag_by(s: &mut Session, name: &str, (dx, dy): (i32, i32)) -> Point {
    let (button, mods) = (Button::Left, Modifiers::default());
    let (x, y) = drawn(s, name);
    let pos = Point {
        x: x as f32 + 20.0,
        y: y as f32 + 20.0,
    };
    s.handle(Input::PointerDown { pos, button, mods }, T);
    let pos = Point {
        x: pos.x + dx as f32,
        y: pos.y + dy as f32,
    };
    s.handle(Input::PointerMove { pos, mods }, T);
    pos
}

fn release(s: &mut Session, pos: Point) {
    let (button, mods) = (Button::Left, Modifiers::default());
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

#[test]
fn a_note_dragged_out_of_a_pushed_position_lands_where_released() {
    let mut s = pushed_note();
    let was = drawn(&s, "n");
    let at = drag_by(&mut s, "n", (400, 500));
    release(&mut s, at);
    assert_eq!(drawn(&s, "n"), plus(was, (400, 500)));
    assert_eq!(placed(&s, "n"), plus(was, (400, 500)));
}

#[test]
fn a_whole_chain_dragged_out_of_a_pushed_position_lands_where_released_keeping_its_layout() {
    let mut s = pushed_chain();
    let (x, y) = (drawn(&s, "x"), drawn(&s, "y"));
    s.select([id("x"), id("y")]);
    let at = drag_by(&mut s, "x", (500, 300));
    release(&mut s, at);
    assert_eq!(drawn(&s, "x"), plus(x, (500, 300)));
    assert_eq!(drawn(&s, "y"), plus(y, (500, 300)));
}

#[test]
fn dragging_one_shape_leaves_the_rest_of_its_chain_placed_where_it_was_drawn() {
    let mut s = pushed_chain();
    let y = drawn(&s, "y");
    let at = drag_by(&mut s, "x", (600, 0));
    release(&mut s, at);
    assert_eq!(drawn(&s, "y"), y, "the rest stays put");
    assert_eq!(placed(&s, "y"), y, "and is now placed there");
}

#[test]
fn the_move_undoes_as_one_step_and_a_touched_chain_stops_settling_back() {
    let mut s = pushed_chain();
    let (x, y) = (drawn(&s, "x"), drawn(&s, "y"));
    let at = drag_by(&mut s, "x", (600, 0));
    release(&mut s, at);
    s.handle(Input::Undo, T);
    assert_eq!((placed(&s, "x"), placed(&s, "y")), ((100, 60), (100, 157)));
    assert_eq!((drawn(&s, "x"), drawn(&s, "y")), (x, y));
    assert!(!s.can_undo());
    s.handle(Input::Redo, T);
    assert_eq!(placed(&s, "y"), y);
    assert_eq!(drawn(&s, "x"), plus(x, (600, 0)));
    // Take the trunk away: nothing pushes y any more, and it stays.
    s.select([id("t"), id("u"), id("v")]);
    s.handle(Input::DeleteShapes, T);
    assert_eq!(drawn(&s, "y"), y);
}

#[test]
fn a_drag_released_where_it_began_touches_nothing() {
    let mut s = pushed_note();
    let at = drag_by(&mut s, "n", (40, 0));
    let (button, mods) = (Button::Left, Modifiers::default());
    let pos = Point {
        x: at.x - 40.0,
        y: at.y,
    };
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    assert_eq!(placed(&s, "n"), (100, 60));
    assert!(!s.can_undo());
}

#[test]
fn a_nudge_moves_from_where_the_shape_is_drawn() {
    let mut s = pushed_chain();
    let (x, y) = (drawn(&s, "x"), drawn(&s, "y"));
    let step = s.theme().nudge;
    s.select([id("x")]);
    s.handle(Input::Nudge(Side::Right), T);
    assert_eq!(drawn(&s, "x"), plus(x, (step, 0)));
    assert_eq!(placed(&s, "y"), y);
    s.handle(Input::Undo, T);
    assert_eq!(placed(&s, "y"), (100, 157));
}

#[test]
fn move_shape_goes_by_the_same_path() {
    let mut s = pushed_chain();
    let y = drawn(&s, "y");
    s.move_shape(&id("x"), 900, -60);
    assert_eq!(drawn(&s, "x"), (900, -60));
    assert_eq!(placed(&s, "y"), y);
}

/// `s`'s document changed by `f`, as if opened that way, in the same view.
fn reopened(s: Session, f: impl FnOnce(&mut Document)) -> Session {
    let mut doc = s.document().clone();
    f(&mut doc);
    let mut new = common::session_of(doc);
    new.set_viewport(s.viewport());
    new
}

/// An empty zone at (x, y) holding `members`.
fn zone(x: i32, y: i32, width: u32, height: u32, members: Vec<ShapeId>) -> Zone {
    Zone {
        id: id("z"),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width,
        height,
        members,
    }
}

#[test]
fn a_zone_move_carries_its_shapes_from_where_they_are_drawn() {
    // The zone holds only the note, though the trunk stands in it too.
    let big = zone(-500, -300, 1000, 800, vec![id("n")]);
    let mut s = reopened(pushed_note(), |d| d.zones.push(big));
    let was = drawn(&s, "n");
    let (button, mods) = (Button::Left, Modifiers::default());
    let pos = s.viewport().to_screen(Point {
        x: -400.0,
        y: -285.0,
    });
    s.handle(Input::PointerDown { pos, button, mods }, T);
    let pos = Point {
        x: pos.x + 1200.0,
        y: pos.y + 500.0,
    };
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    assert_eq!(s.document().zones[0].x, 700);
    assert_eq!(drawn(&s, "n"), plus(was, (1200, 500)));
    assert_eq!(placed(&s, "t"), (0, 0));
}

/// How far `name` is drawn off its rest position just after a change at `T`, slides only.
fn slide(s: &Session, name: &str) -> (f32, f32) {
    let (from, to) = (
        Point {
            x: -3000.0,
            y: -3000.0,
        },
        Point {
            x: 3000.0,
            y: 3000.0,
        },
    );
    let f = s.draw(from, to, T);
    let v = f.shapes.iter().find(|v| v.shape.id.0 == name).expect(name);
    (v.slide.x, v.slide.y)
}

#[test]
fn a_touched_chain_takes_its_new_placed_positions_without_sliding_either_way() {
    let mut s = pushed_chain();
    let at = drag_by(&mut s, "x", (600, 0));
    release(&mut s, at);
    assert_eq!(slide(&s, "x"), (0.0, 0.0), "the drop lands as released");
    assert_eq!(slide(&s, "y"), (0.0, 0.0), "the rest is where it was drawn");
    s.handle(Input::Undo, T);
    assert_eq!(
        slide(&s, "y"),
        (0.0, 0.0),
        "drawn the same before and after"
    );
    s.handle(Input::Redo, T);
    assert_eq!(slide(&s, "y"), (0.0, 0.0));
}

#[test]
fn undoing_and_redoing_a_move_slides_the_moved_shape_where_a_new_move_lands_at_once() {
    let mut s = session(&[("n", 0, 0)], &[]);
    let at = drag_by(&mut s, "n", (600, 0));
    release(&mut s, at);
    assert_eq!(slide(&s, "n"), (0.0, 0.0), "the drop lands as released");
    s.handle(Input::Undo, T);
    assert_eq!((drawn(&s, "n"), slide(&s, "n")), ((0, 0), (600.0, 0.0)));
    s.handle(Input::Redo, Duration::from_millis(300));
    assert_eq!((drawn(&s, "n"), slide(&s, "n")), ((600, 0), (-600.0, 0.0)));
}

#[test]
fn nothing_moves_during_a_drag_and_on_release_the_rest_slides_clear() {
    // x-y is larger than the note m it is dropped on, so m gives way.
    let mut s = session(&[("x", 0, 0), ("y", 0, 97), ("m", 600, 0)], &[("x", "y")]);
    s.select([id("x"), id("y")]);
    let at = drag_by(&mut s, "x", (590, 0));
    assert_eq!(drawn(&s, "m"), (600, 0), "the drag passes over it");
    release(&mut s, at);
    assert_eq!(drawn(&s, "x"), (590, 0), "the drop lands exactly");
    assert_eq!(slide(&s, "x"), (0.0, 0.0));
    let m = drawn(&s, "m");
    assert_ne!(m, (600, 0), "m is pushed clear");
    assert_eq!(placed(&s, "m"), (600, 0), "and its push isn't saved");
    let from = (600.0 - m.0 as f32, -m.1 as f32);
    assert_eq!(slide(&s, "m"), from, "it slides from where it was");
}

#[test]
fn a_shape_dropped_on_a_larger_chain_slides_off_it_from_where_it_was_released() {
    let mut s = pushed_note();
    let (x, y) = drawn(&s, "n");
    // Back onto the trunk from wherever the note was pushed.
    let at = drag_by(&mut s, "n", (100 - x, 110 - y));
    release(&mut s, at);
    assert_eq!(placed(&s, "n"), (100, 110));
    let rest = drawn(&s, "n");
    assert_ne!(rest, (100, 110), "pushed off the trunk");
    let (dx, dy) = slide(&s, "n");
    assert_eq!((rest.0 as f32 + dx, rest.1 as f32 + dy), (100.0, 110.0));
}

#[test]
fn a_drop_is_judged_for_its_zone_where_it_was_released() {
    let s = pushed_note();
    let (x, y) = drawn(&s, "n");
    // Holds the release, but not the placed position plus the drag, 133 lower.
    let under = zone(x + 500, y + 500, 400, 140, Vec::new());
    let mut s = reopened(s, |d| d.zones.push(under));
    let at = drag_by(&mut s, "n", (600, 600));
    release(&mut s, at);
    assert_eq!(s.document().zones[0].members, [id("n")]);
    assert_eq!(drawn(&s, "n"), (x + 600, y + 600));
}

/// a, then b and c below it; a is 300 taller than when they were placed, which pushes them
/// inside their own chain.
fn grown_chain() -> Session {
    let links = [("a", "b"), ("b", "c")];
    let s = session(&[("a", 0, 0), ("b", 0, 97), ("c", 0, 194)], &links);
    let s = reopened(s, |d| d.shapes[0].extra_height = 300);
    assert_ne!(drawn(&s, "b"), (0, 97), "b is pushed");
    s
}

/// The vertical gap from `upper`'s drawn bottom to `lower`'s drawn top.
fn gap(s: &Session, upper: &ShapeId, lower: &ShapeId) -> f32 {
    let (u, l) = (s.drawn_rect(upper), s.drawn_rect(lower));
    l.expect("lower").y - u.expect("upper").bottom()
}

#[test]
fn a_new_shape_lands_one_creation_gap_from_where_a_pushed_source_is_drawn() {
    let creation = |s: &Session| s.theme().creation_gap;
    // Pushed inside its chain.
    let mut s = grown_chain();
    s.create_linked(&id("c"), Side::Bottom);
    let new = s.selection()[0].clone();
    let d = gap(&s, &id("c"), &new);
    assert!((d - creation(&s)).abs() <= 1.0, "{d}");
    assert_eq!(drawn(&s, "c").0, s.drawn_rect(&new).expect("new").x as i32);
    // Pushed by a larger chain.
    let mut s = pushed_note();
    s.create_linked(&id("n"), Side::Bottom);
    let new = s.selection()[0].clone();
    let d = gap(&s, &id("n"), &new);
    assert!((d - creation(&s)).abs() <= 1.0, "{d}");
    // From the side.
    let mut s = pushed_note();
    s.create_linked(&id("n"), Side::Right);
    let new = s.selection()[0].clone();
    let (n, r) = (
        s.drawn_rect(&id("n")).expect("n"),
        s.drawn_rect(&new).expect("new"),
    );
    assert!((r.x - n.right() - creation(&s)).abs() <= 1.0);
    assert_eq!(r.center().y, n.center().y);
}

#[test]
fn insert_and_push_from_a_pushed_shape_keeps_the_order_and_the_gaps() {
    let mut s = grown_chain();
    let lower = gap(&s, &id("b"), &id("c"));
    s.create_linked(&id("a"), Side::Bottom);
    let new = s.selection()[0].clone();
    let links = &s.document().links;
    assert!(links.contains(&Link(id("a"), new.clone())), "{links:?}");
    assert!(links.contains(&Link(new.clone(), id("b"))), "{links:?}");
    let creation = s.theme().creation_gap;
    let (above, below) = (gap(&s, &id("a"), &new), gap(&s, &new, &id("b")));
    assert!((above - creation).abs() <= 1.0, "{above}");
    assert!((below - creation).abs() <= 1.0, "{below}");
    assert_eq!(gap(&s, &id("b"), &id("c")), lower);
}

#[test]
fn an_insert_in_a_pushed_chain_slides_the_subtree_from_where_it_was_drawn() {
    let mut s = pushed_chain();
    let was = drawn(&s, "y");
    s.create_linked(&id("x"), Side::Bottom);
    let (x, y) = (drawn(&s, "y"), slide(&s, "y"));
    assert_ne!(x, was, "y moved");
    assert_eq!(
        (x.0 as f32 + y.0, x.1 as f32 + y.1),
        (was.0 as f32, was.1 as f32)
    );
}
