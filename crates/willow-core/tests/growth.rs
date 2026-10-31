//! Growth: a shape growing (typing, resizing) is pushed clear by the spacing pass. Nothing is
//! saved: tests read drawn positions, and placed ones stay where the writer put them.
mod common;

use std::time::Duration;

use willow_core::{
    Color, Document, Input, Link, Point, ResizeTo, Session, Shape, ShapeId, Side, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty rose shapes `(id, x, y)` (210 x 48.5 each), linked as `links`.
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    let doc = Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    };
    common::session_of(doc)
}

fn shape<'a>(s: &'a Session, name: &str) -> &'a Shape {
    (s.document().shapes.iter().find(|x| x.id.0 == name)).expect(name)
}

fn at(s: &Session, name: &str) -> (i32, i32) {
    let r = s.drawn_rect(&id(name)).expect(name);
    (r.x as i32, r.y as i32)
}

fn placed(s: &Session, name: &str) -> (i32, i32) {
    let sh = shape(s, name);
    (sh.x, sh.y)
}

/// The bottom edge of `name` as drawn, in canvas units.
fn bottom(s: &Session, name: &str) -> f32 {
    s.drawn_rect(&id(name)).expect(name).bottom()
}

/// Selects `name` and types `text` into it (starting an editing session).
fn type_into(s: &mut Session, name: &str, text: &str) {
    s.select([id(name)]);
    s.handle(
        Input::Text {
            text: text.into(),
            replace: None,
        },
        T,
    );
}

#[test]
fn typing_growth_pushes_the_subtree_below_to_keep_the_creation_gap() {
    // a, b one gap below (48.5 + 48 = 96.5 -> 97), c below b; x hangs off b to the right.
    let mut s = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194), ("x", 258, 97)],
        &[("a", "b"), ("b", "c"), ("b", "x")],
    );
    type_into(&mut s, "a", &"word ".repeat(30));
    let grown = bottom(&s, "a");
    assert!(grown > 100.0, "{grown}");
    // b sits one creation gap (48) below a's new bottom, rounded up; its subtree moves along.
    let dy = (grown + 48.0).ceil() as i32 - 97;
    assert_eq!(at(&s, "b"), (0, 97 + dy));
    assert_eq!(at(&s, "c"), (0, 194 + dy));
    assert_eq!(at(&s, "x"), (258, 97 + dy));
    assert_eq!(at(&s, "a"), (0, 0));
    assert_eq!(placed(&s, "b"), (0, 97));
    assert_eq!(placed(&s, "x"), (258, 97));
}

#[test]
fn a_shape_growing_past_its_neighbors_placed_position_still_pushes_it_the_right_way() {
    // a's center passes b's placed one, and even b's placed bottom: b still goes below.
    let mut s = session(&[("a", 0, 0), ("b", 0, 97)], &[("a", "b")]);
    let r = ResizeTo {
        x: 0,
        width: 210,
        extra_height: 900,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!(at(&s, "a"), (0, 0));
    // 948.5 + 48, rounded up.
    assert_eq!(at(&s, "b"), (0, 997));
    // Sideways too: a's right edge passes all of r's placed rectangle; a's center stays.
    let mut s = session(&[("a", 0, 0), ("r", 258, 0)], &[("a", "r")]);
    let r = ResizeTo {
        x: -400,
        width: 1010,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!(at(&s, "r"), (658, 0));
}

#[test]
fn other_chains_crowded_below_move_whole_and_cascade_across_three_chains() {
    // a's chain is the largest; chain b1-b2 31.5 below a; c1 31.5 below b1; d off to the side.
    let mut s = session(
        &[
            ("a", 0, 0),
            ("k1", 3000, 0),
            ("k2", 3000, 97),
            ("b1", 0, 80),
            ("b2", 600, 300),
            ("c1", 0, 160),
            ("d", 900, 60),
        ],
        &[("a", "k1"), ("k1", "k2"), ("b1", "b2")],
    );
    type_into(&mut s, "a", &"word ".repeat(30));
    // b1 is pushed to the minimum gap (24) below a's new bottom, with all of its chain.
    let db = (bottom(&s, "a") + 24.0).ceil() as i32 - 80;
    assert!(db > 0);
    assert_eq!(at(&s, "b1"), (0, 80 + db));
    assert_eq!(at(&s, "b2"), (600, 300 + db));
    // b1 then crowds c1 (its bottom is 128.5 + db), which goes 24 below it.
    let dc = (128.5 + db as f32 + 24.0).ceil() as i32 - 160;
    assert_eq!(at(&s, "c1"), (0, 160 + dc));
    assert_eq!(at(&s, "d"), (900, 60));
}

#[test]
fn a_small_branch_that_grows_into_the_trunk_moves_itself() {
    // Its own chain: the three-shape trunk holds, and the note rises off it.
    let trunk = [("a", 0, 0), ("b", 0, 97), ("c", 0, 194)];
    let mut shapes = trunk.to_vec();
    shapes.push(("n", 0, -73));
    let mut s = session(&shapes, &[("a", "b"), ("b", "c")]);
    type_into(&mut s, "n", &"word ".repeat(30));
    assert_eq!(
        (at(&s, "a"), at(&s, "b"), at(&s, "c")),
        trunk.map(|t| (t.1, t.2)).into()
    );
    assert_eq!(bottom(&s, "n").ceil() + 24.0, 0.0);

    // In the trunk's own chain: x is walked after the whole trunk, so x moves, right 12.
    let mut s = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194), ("x", 258, 0)],
        &[("a", "b"), ("b", "c"), ("a", "x")],
    );
    let r = ResizeTo {
        x: 0,
        width: 246,
        extra_height: 0,
    };
    s.resize_shape(&id("c"), r);
    let r = ResizeTo {
        x: 258,
        width: 210,
        extra_height: 400,
    };
    s.resize_shape(&id("x"), r);
    assert_eq!(
        (at(&s, "a"), at(&s, "b"), at(&s, "c")),
        ((0, 0), (0, 97), (0, 194))
    );
    assert_eq!(at(&s, "x"), (270, 0));
}

#[test]
fn growth_saves_no_moves_and_folds_nothing_into_the_undo_step() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97), ("e", 0, 170)], &[("a", "b")]);
    let before = s.document().clone();
    let drawn_before = (at(&s, "a"), at(&s, "b"), at(&s, "e"));
    type_into(&mut s, "a", &"word ".repeat(30));
    s.handle(Input::Escape, T);
    let grown = (at(&s, "a"), at(&s, "b"), at(&s, "e"));
    assert_ne!(grown, drawn_before);
    assert_eq!((placed(&s, "b"), placed(&s, "e")), ((0, 97), (0, 170)));
    // Only a's text differs from the start.
    let mut typed = s.document().clone();
    typed.shapes[0].text = before.shapes[0].text.clone();
    assert_eq!(typed, before);

    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
    assert_eq!((at(&s, "a"), at(&s, "b"), at(&s, "e")), drawn_before);
    s.handle(Input::Redo, T);
    assert_eq!((at(&s, "a"), at(&s, "b"), at(&s, "e")), grown);
}

#[test]
fn shrinking_settles_pushed_shapes_back_and_a_drag_pushes_nothing_it_no_longer_reaches() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97), ("e", 300, 0)], &[("a", "b")]);
    type_into(&mut s, "a", &"word ".repeat(30));
    assert!(at(&s, "b").1 > 97);
    // Deleting the text shrinks a; b comes back.
    s.handle(Input::SelectAll, T);
    s.handle(Input::Delete(willow_core::Motion::Left), T);
    s.handle(Input::Escape, T);
    assert!(bottom(&s, "a") < 50.0);
    assert_eq!(at(&s, "b"), (0, 97));
    // A narrower resize moves nothing either.
    let r = ResizeTo {
        x: 20,
        width: 170,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!((at(&s, "b"), at(&s, "e")), ((0, 97), (300, 0)));
}

#[test]
fn shapes_a_deleted_shape_pushed_settle_back_and_placed_positions_stay() {
    let mut s = session(
        &[("a", 0, 0), ("b", 0, 97), ("c", 0, 194), ("n", 300, 0)],
        &[("a", "b"), ("b", "c")],
    );
    type_into(&mut s, "b", &"word ".repeat(30));
    s.handle(Input::Escape, T);
    let pushed = at(&s, "c");
    assert!(pushed.1 > 194);
    s.handle(Input::DeleteShapes, T);
    // a and c are linked now, and 194 is well clear of a.
    assert_eq!(at(&s, "c"), (0, 194));
    assert_eq!(placed(&s, "c"), (0, 194));
    s.handle(Input::Undo, T);
    assert_eq!(at(&s, "c"), pushed);
}

#[test]
fn a_typing_session_undoes_in_one_step_and_zones_keep_their_members_and_their_rectangle() {
    let mut doc = session(&[("a", 0, 0), ("b", 0, 97), ("e", 0, 170)], &[("a", "b")])
        .document()
        .clone();
    doc.zones.push(Zone {
        id: id("z"),
        title: "Z".into(),
        color: Color::Sky,
        x: -20,
        y: 80,
        width: 300,
        height: 150,
        members: vec![id("b"), id("e")],
    });
    let mut s = common::session_of(doc.clone());
    for word in ["word "; 30] {
        type_into(&mut s, "a", word);
    }
    // b (own chain) and then e (another chain) are drawn out of the zone's rectangle.
    assert!(at(&s, "b").1 > 230 && at(&s, "e").1 > at(&s, "b").1);
    let z = &s.document().zones[0];
    assert_eq!(z.members, [id("b"), id("e")]);
    assert_eq!((z.x, z.y, z.width, z.height), (-20, 80, 300, 150));
    s.handle(Input::Escape, T);
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), doc);
    assert!(!s.can_undo());
}

#[test]
fn a_midpoint_created_shape_landing_on_another_chain_is_cleared_and_saves_nothing() {
    // The new shape goes at (0, 97), 48.5 tall; g (chain g-g2) sits under that spot.
    let mut s = session(
        &[("a", 0, 0), ("g", 0, 120), ("g2", 400, 500), ("h", 300, 97)],
        &[("g", "g2")],
    );
    s.create_linked(&id("a"), Side::Bottom);
    // 24 below the new shape's bottom (145.5): 169.5, rounded up.
    assert_eq!((at(&s, "g"), at(&s, "g2")), ((0, 170), (400, 550)));
    assert_eq!((placed(&s, "g"), placed(&s, "g2")), ((0, 120), (400, 500)));
    assert_eq!(at(&s, "h"), (300, 97));
    s.handle(Input::Escape, T);
    s.handle(Input::Undo, T);
    assert_eq!((at(&s, "g"), at(&s, "g2")), ((0, 120), (400, 500)));
}

#[test]
fn a_new_shape_placed_on_a_chain_saves_nothing_and_is_cleared_itself() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97)], &[("a", "b")]);
    s.select([id("a")]);
    s.handle(Input::NewBeside, T);
    s.handle(Input::Escape, T);
    let c = s.selection()[0].clone();
    assert_eq!((placed(&s, "a"), placed(&s, "b")), ((0, 0), (0, 97)));
    assert_eq!(at(&s, &c.0), placed(&s, &c.0));
}

#[test]
fn widening_pushes_left_and_right_subtrees_outward_and_saves_only_the_shape() {
    // a with l (and its child ll) on the left and r on the right, each one gap away; a is
    // the topmost, so the walk starts there.
    let mut s = session(
        &[("a", 0, 0), ("l", -258, 1), ("ll", -516, 1), ("r", 258, 1)],
        &[("a", "l"), ("l", "ll"), ("a", "r")],
    );
    let before = s.document().clone();
    // 40 units wider on each side: edges at -40 and 250.
    let r = ResizeTo {
        x: -40,
        width: 290,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!(at(&s, "r"), (298, 1));
    assert_eq!(at(&s, "l"), (-298, 1));
    assert_eq!(at(&s, "ll"), (-556, 1));
    assert_eq!(placed(&s, "l"), (-258, 1));
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
    assert_eq!(at(&s, "r"), (258, 1));
}

#[test]
fn widening_a_shape_past_the_top_of_a_level_row_moves_it_and_what_lies_beyond_it() {
    // On a level row the walk starts at the leftmost shape: l stays and a gives way to it.
    let mut s = session(
        &[("l", -258, 0), ("a", 0, 0), ("r", 258, 0)],
        &[("a", "l"), ("a", "r")],
    );
    let r = ResizeTo {
        x: -40,
        width: 290,
        extra_height: 0,
    };
    s.resize_shape(&id("a"), r);
    assert_eq!(at(&s, "l"), (-258, 0));
    assert_eq!(at(&s, "a"), (0, 0));
    assert_eq!(at(&s, "r"), (338, 0));
}

#[test]
fn bold_that_rewraps_taller_pushes_like_typing() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97)], &[("a", "b")]);
    // Four words fit one line (172 of 178); bold (11 a letter) needs 188, so two lines.
    type_into(&mut s, "a", "word word word word");
    assert_eq!(at(&s, "b"), (0, 97));
    s.handle(Input::SelectAll, T);
    s.handle(Input::ToggleBold, T);
    assert_eq!(bottom(&s, "a"), 71.0);
    // One creation gap (48) below a's new bottom.
    assert_eq!(at(&s, "b"), (0, 119));
    assert_eq!(placed(&s, "b"), (0, 97));
}

/// How far `name` is drawn off its rest position at `ms`, slides only.
fn slide(s: &Session, name: &str, ms: u64) -> (f32, f32) {
    let to = Point {
        x: 1000.0,
        y: 800.0,
    };
    let f = s.draw(Point::default(), to, Duration::from_millis(ms));
    let v = f.shapes.iter().find(|v| v.shape.id.0 == name).expect(name);
    (v.slide.x, v.slide.y)
}

/// A session with `b` below `a`, the screen at canvas (0, 0).
fn pair() -> Session {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97)], &[("a", "b")]);
    s.set_viewport(willow_core::Viewport {
        origin: Point::default(),
        zoom: 1.0,
        size: Point {
            x: 1000.0,
            y: 800.0,
        },
    });
    s
}

#[test]
fn pushed_shapes_slide_from_where_they_were_drawn_to_where_the_pass_puts_them() {
    let mut s = pair();
    assert_eq!(slide(&s, "b", 0), (0.0, 0.0));
    type_into(&mut s, "a", "word word word word word");
    assert_eq!(at(&s, "b"), (0, 119));
    assert_eq!(slide(&s, "b", 0), (0.0, -22.0));
    let mid = slide(&s, "b", 110).1;
    assert!(-22.0 < mid && mid < 0.0, "{mid}");
    assert!(s.animating(Duration::from_millis(200)));
    assert_eq!(slide(&s, "b", 220), (0.0, 0.0));
    assert!(!s.animating(Duration::from_millis(220)));
    assert_eq!(slide(&s, "a", 0), (0.0, 0.0));
}

#[test]
fn undo_and_shrinking_slide_too_and_a_change_during_a_slide_retargets_it() {
    let mut s = pair();
    type_into(&mut s, "a", "word word word word word");
    s.handle(Input::Escape, T);
    // Half way through, Undo sends b home: it slides from where it is drawn right then.
    let half = Duration::from_millis(110);
    let left = slide(&s, "b", 110).1;
    s.handle(Input::Undo, half);
    assert_eq!(at(&s, "b"), (0, 97));
    assert_eq!(slide(&s, "b", 110), (0.0, left + 22.0));
    assert!(s.animating(half + Duration::from_millis(219)));
    assert!(!s.animating(half + Duration::from_millis(220)));
}

#[test]
fn a_reload_is_instant() {
    let file = |y: i32| {
        let lay = format!("@layout a 0,0 w210\n@layout b {y},30 w210\n");
        format!("willow 1\n\na: \"\"\n\nb: \"\"\n\n{lay}").into_bytes()
    };
    let bytes = file(0);
    let load = willow_core::load(&bytes).unwrap();
    let mut s = Session::open(load, bytes, &willow_core::PAPER, Box::new(common::Fake));
    assert!(!s.animating(T), "a load is instant");
    assert_eq!(at(&s, "b"), (0, 73));
    // Clear of a now: its push is gone, and nothing slides.
    s.disk_changed(Some(&file(1000)));
    assert_eq!(at(&s, "b"), (1000, 30));
    assert!(!s.animating(T), "a reload is instant");
}

#[test]
fn the_view_stays_put_when_the_shape_being_typed_in_moves() {
    // Typing grows n, smaller than a's chain and above it: n is what moves, down below a.
    let mut s = session(&[("a", 0, 0), ("k", 3000, 0), ("n", 0, -73)], &[("a", "k")]);
    let view = s.viewport();
    type_into(&mut s, "n", &"word ".repeat(30));
    assert_eq!(at(&s, "n"), (0, 73));
    assert_eq!(placed(&s, "n"), (0, -73));
    let t = Duration::from_secs(1);
    s.tick(t);
    assert_eq!(s.viewport(), view);
    assert!(!s.animating(t));
}
