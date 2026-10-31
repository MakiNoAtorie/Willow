//! The spacing pass: chains that crowd each other are drawn apart, and the document keeps its
//! placed positions. Empty shapes here are 210 x 48.5; the minimum gap is 24 units.
mod common;

use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use willow_core::{
    Button, Document, Input, Link, Measure, Modifiers, Motion, PAPER, Paragraph, Point, Session,
    Shape, ShapeId, Theme, Viewport, generate, load,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn doc(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Document {
    Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape::new(id(s), x, y))
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    }
}

/// A session over `shapes`, the screen at canvas (0, 0).
fn session(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Session {
    let mut s = common::session_of(doc(shapes, links));
    s.set_viewport(Viewport {
        origin: Point::default(),
        zoom: 1.0,
        size: Point {
            x: 1000.0,
            y: 800.0,
        },
    });
    s
}

fn drawn(s: &Session, name: &str) -> (f32, f32) {
    let r = s.drawn_rect(&id(name)).expect(name);
    (r.x, r.y)
}

fn placed(s: &Session, name: &str) -> (i32, i32) {
    let sh = s.document().shapes.iter().find(|x| x.id.0 == name);
    sh.map(|x| (x.x, x.y)).expect(name)
}

/// Drags from canvas point `from` to canvas point `to` and releases.
fn drag(s: &mut Session, from: (f32, f32), to: (f32, f32)) {
    let (button, mods) = (Button::Left, Modifiers::default());
    let p = |(x, y)| Point { x, y };
    let pos = p(from);
    s.handle(Input::PointerDown { pos, button, mods }, T);
    let pos = p(to);
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

#[test]
fn a_note_dropped_on_the_trunk_is_drawn_clear_and_keeps_its_placed_position() {
    let mut s = session(&[("a", 0, 0), ("b", 0, 97), ("n", 300, 400)], &[("a", "b")]);
    drag(&mut s, (310.0, 410.0), (20.0, 15.0));
    assert_eq!(placed(&s, "n"), (10, 5));
    // Up 78 is shorter than right 224, left 244 and down 165 (below b, not just below a).
    assert_eq!(drawn(&s, "n"), (10.0, -73.0));
    assert_eq!(drawn(&s, "a"), (0.0, 0.0));
    assert_eq!(drawn(&s, "b"), (0.0, 97.0));
}

#[test]
fn the_larger_chain_holds_still_even_below_and_after_the_smaller() {
    let s = session(&[("n", 0, 0), ("a", 0, 30), ("b", 0, 127)], &[("a", "b")]);
    assert_eq!(drawn(&s, "a"), (0.0, 30.0));
    assert_eq!(drawn(&s, "n"), (0.0, -43.0));
}

#[test]
fn of_equal_chains_the_highest_top_shape_holds_then_the_leftmost_then_the_lower_id() {
    // Whatever their document order.
    let s = session(&[("a", 0, 30), ("b", 0, 0)], &[]);
    assert_eq!(drawn(&s, "b"), (0.0, 0.0), "highest");
    assert_eq!(drawn(&s, "a"), (0.0, 73.0));
    // The chains are compared by their top shapes, not by the shape at the far end.
    let s = session(
        &[("a", 0, 30), ("b", -900, 500), ("c", 0, 0), ("d", 900, 900)],
        &[("a", "b"), ("c", "d")],
    );
    assert_eq!(drawn(&s, "c"), (0.0, 0.0), "highest top shape");
    assert_eq!(drawn(&s, "a"), (0.0, 73.0));
    let s = session(&[("a", 10, 0), ("b", 0, 0)], &[]);
    assert_eq!(drawn(&s, "b"), (0.0, 0.0), "leftmost");
    assert_eq!(drawn(&s, "a"), (10.0, 73.0));
    let s = session(&[("b", 0, 0), ("a", 0, 0)], &[]);
    assert_eq!(drawn(&s, "a"), (0.0, 0.0), "lower ID");
    assert_eq!(drawn(&s, "b"), (0.0, 73.0));
    let s = session(&[("aa", 0, 0), ("b", 0, 0)], &[]);
    assert_eq!(drawn(&s, "b"), (0.0, 0.0), "shorter IDs come first");
    assert_eq!(drawn(&s, "aa"), (0.0, 73.0));
}

#[test]
fn ties_for_the_shortest_move_go_down_right_left_up() {
    // Holders `h`-`k`, the note `n` crowding `h` only. Empty shapes are 210 x 48.5, so with
    // the 24 gap `n` clears `h` by 234 - |dx| sideways or 73 - |dy| upright.
    let tie = |n: (i32, i32), tall: u32| {
        let mut d = doc(
            &[("h", 0, 0), ("k", 3000, 3000), ("n", n.0, n.1)],
            &[("h", "k")],
        );
        d.shapes[0].extra_height = tall;
        d.shapes[2].extra_height = tall;
        let s = common::session_of(d);
        let (x, y) = drawn(&s, "n");
        (x as i32 - n.0, y as i32 - n.1)
    };
    // Down, right and up are all 73.
    assert_eq!(tie((161, 0), 0), (0, 73), "down before right");
    // Both 848.5 tall: right and left are 234, down and up 873.
    assert_eq!(tie((0, 0), 800), (234, 0), "right before left");
    // Left and up are 233; right is 235 and down far more.
    assert_eq!(tie((-1, -640), 800), (-233, 0), "left before up");
}

#[test]
fn a_pushed_chain_pushes_what_it_then_crowds() {
    let s = session(
        &[
            ("a", 0, 0),
            ("b", 0, 97),
            ("c", 0, 194),
            ("d", 150, 97),
            ("e", 150, 500),
            ("n", 400, 97),
            ("far", 3000, 3000),
        ],
        &[("a", "b"), ("b", "c"), ("d", "e")],
    );
    // d's chain goes right 84 as one block, which brings it within the gap of n: right 68.
    assert_eq!(drawn(&s, "c"), (0.0, 194.0));
    assert_eq!(drawn(&s, "d"), (234.0, 97.0));
    assert_eq!(drawn(&s, "e"), (234.0, 500.0));
    assert_eq!(drawn(&s, "n"), (468.0, 97.0));
    assert_eq!(drawn(&s, "far"), (3000.0, 3000.0));
}

#[test]
fn a_block_may_sit_in_a_hole_in_a_larger_chain() {
    // Only shapes count, not a chain's bounding box.
    let s = session(&[("a", 0, 0), ("b", 0, 300), ("n", 0, 150)], &[("a", "b")]);
    assert_eq!(drawn(&s, "n"), (0.0, 150.0));
}

#[test]
fn exactly_the_gap_is_not_crowding_and_a_move_lands_on_whole_units() {
    // 48.5 tall + 24 gap is 72.5: 73 is clear, and 72 goes on to the next whole unit.
    let clear = session(&[("a", 0, 0), ("b", 0, 73)], &[]);
    assert_eq!(drawn(&clear, "b"), (0.0, 73.0));
    let close = session(&[("a", 0, 0), ("b", 0, 72)], &[]);
    assert_eq!(drawn(&close, "b"), (0.0, 73.0));
}

#[test]
fn shapes_too_big_for_the_index_still_hold_and_still_move() {
    // The sizes a file may ask for.
    const HUGE: u32 = 500_000_000;
    let mut d = doc(&[("a", 0, 0), ("n", 10, 10)], &[]);
    (d.shapes[0].width, d.shapes[0].extra_height) = (HUGE, HUGE);
    let s = common::session_of(d);
    assert_eq!(drawn(&s, "a"), (0.0, 0.0));
    assert_eq!(drawn(&s, "n"), (10.0, -73.0));
    let mut d = doc(
        &[("a", 0, 0), ("k", 900, 900), ("n", 10, 10)],
        &[("a", "k")],
    );
    (d.shapes[2].width, d.shapes[2].extra_height) = (HUGE, HUGE);
    let s = common::session_of(d);
    assert_eq!(drawn(&s, "a"), (0.0, 0.0));
    assert_eq!(crowding(&s), NOBODY);
}

#[test]
fn a_pile_of_shapes_on_one_spot_ends_uncrowded() {
    let names: Vec<String> = (0..60).map(|i| format!("s{i}")).collect();
    let shapes: Vec<_> = names.iter().map(|n| (n.as_str(), 40, 40)).collect();
    assert_eq!(crowding(&session(&shapes, &[])), NOBODY);
}

#[test]
fn undo_redo_and_delete_rerun_the_pass_and_placed_positions_stay() {
    let mut s = session(&[("h", 0, 0), ("n", 0, 30)], &[]);
    assert_eq!(drawn(&s, "n"), (0.0, 73.0));
    s.select([id("h")]);
    s.handle(Input::DeleteShapes, T);
    assert_eq!(drawn(&s, "n"), (0.0, 30.0), "settles back");
    s.handle(Input::Undo, T);
    assert_eq!(drawn(&s, "n"), (0.0, 73.0));
    s.handle(Input::Redo, T);
    assert_eq!(drawn(&s, "n"), (0.0, 30.0));
    assert_eq!(placed(&s, "n"), (0, 30));
}

#[test]
fn text_growing_and_shrinking_redoes_the_pass_keystroke_by_keystroke() {
    // n sits on a: the pass moves it, and nothing is saved.
    let mut s = session(&[("a", 0, 0), ("n", 0, 30)], &[]);
    let key = |s: &mut Session, text: &str| {
        let (text, replace) = (text.to_string(), None);
        s.handle(Input::Text { text, replace }, T);
    };
    s.select([id("a")]);
    key(&mut s, "x");
    let one_line = drawn(&s, "n");
    assert_eq!(one_line.1, 73.0);
    for _ in 0..25 {
        key(&mut s, "x");
    }
    let tall = s.drawn_rect(&id("a")).expect("a").height;
    assert!(tall > 48.5, "{tall}");
    assert_eq!(drawn(&s, "n"), (0.0, (tall + 24.0).ceil()));
    assert_eq!(placed(&s, "n"), (0, 30));
    for _ in 0..25 {
        s.handle(Input::Delete(Motion::Left), T);
    }
    assert_eq!(drawn(&s, "n"), one_line);
}

#[test]
fn a_new_link_changes_which_chain_is_larger() {
    let mut s = session(&[("a", 0, 30), ("b", 0, 0), ("k", 3000, 3000)], &[]);
    assert_eq!(drawn(&s, "a"), (0.0, 73.0));
    s.link(&id("a"), &id("k"));
    assert_eq!(drawn(&s, "a"), (0.0, 30.0));
    assert_eq!(drawn(&s, "b"), (0.0, -43.0));
}

#[test]
fn text_applied_from_the_code_panel_runs_the_pass_and_the_text_keeps_placed_positions() {
    let mut s = common::session_of(doc(&[("a", 0, 0), ("b", 900, 0)], &[]));
    s.handle(Input::TogglePanel, T);
    settle(&mut s);
    let t = Duration::from_secs(1);
    let at = s.panel().text().find("900,0").expect("in the layout");
    let (text, replace) = ("0,30".into(), Some(at..at + 5));
    s.handle(Input::Text { text, replace }, t);
    let due = t + Duration::from_secs(1);
    let job = s.take_text_check(due).expect("a check is due");
    s.text_checked(job.run());
    settle(&mut s);
    assert_eq!(placed(&s, "b"), (0, 30));
    assert_eq!(drawn(&s, "b"), (0.0, 73.0));
    assert!(s.panel().text().contains("b 0,30 w210"));
}

#[test]
fn linked_shapes_closer_than_the_creation_gap_are_drawn_at_it_along_their_link() {
    // 48.5 tall + 48 gap is 96.5: the next whole unit is 97.
    let s = session(&[("a", 0, 0), ("b", 0, 60)], &[("a", "b")]);
    assert_eq!(drawn(&s, "b"), (0.0, 97.0));
    // Sideways, the far shape goes along the link too; 210 wide + 48 gap is 258.
    let s = session(&[("a", 0, 0), ("b", 230, 10)], &[("a", "b")]);
    assert_eq!(drawn(&s, "b"), (258.0, 10.0));
    // Exactly the gap is not crowding.
    let s = session(&[("a", 0, 0), ("b", 0, 97)], &[("a", "b")]);
    assert_eq!(drawn(&s, "b"), (0.0, 97.0));
}

/// `s`'s shape `name` made `extra` units taller.
fn taller(d: &mut Document, name: &str, extra: u32) {
    let sh = d.shapes.iter_mut().find(|s| s.id.0 == name);
    sh.expect(name).extra_height = extra;
}

#[test]
fn of_two_crowding_shapes_in_a_chain_the_later_in_the_walk_moves_with_its_subtree() {
    // a's links: b below, walked first; x on the right, then y below x. x reaches down beside
    // b, 12 away where b is wide: the trunk stays, and x goes right, the shortest way out.
    let mut d = doc(
        &[("a", 0, 0), ("b", 0, 97), ("x", 258, 30), ("y", 258, 500)],
        &[("a", "b"), ("a", "x"), ("x", "y")],
    );
    taller(&mut d, "x", 200);
    d.shapes[1].width = 246;
    let s = common::session_of(d);
    assert_eq!(drawn(&s, "a"), (0.0, 0.0));
    assert_eq!(drawn(&s, "b"), (0.0, 97.0));
    assert_eq!(drawn(&s, "x"), (270.0, 30.0));
    assert_eq!(drawn(&s, "y"), (270.0, 500.0));
    assert_eq!(placed(&s, "x"), (258, 30));
}

#[test]
fn the_shortest_way_out_inside_a_chain_keeps_the_creation_gap_to_the_linked_shape() {
    // a's links: x below, walked first; b on the right, then c below b. c lands on x, which
    // is wide. Up 53 clears x but goes inside the creation gap to b, so down 93 wins.
    let mut d = doc(
        &[("a", 0, 0), ("x", 0, 250), ("b", 258, 0), ("c", 258, 230)],
        &[("a", "x"), ("a", "b"), ("b", "c")],
    );
    taller(&mut d, "b", 100);
    d.shapes[1].width = 600;
    let s = common::session_of(d);
    assert_eq!(drawn(&s, "x"), (0.0, 250.0));
    assert_eq!(drawn(&s, "b"), (258.0, 0.0));
    assert_eq!(drawn(&s, "c"), (258.0, 323.0));
}

/// What the app does after every input: runs a due regeneration and hands the text back.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        s.regenerated(job.run());
    }
}

fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake))
}

fn saved_bytes(s: &mut Session) -> Vec<u8> {
    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    written
}

#[test]
fn a_crowded_file_opens_saved_drawn_clear_and_saves_the_same_bytes() {
    let mut bytes = Vec::new();
    let mut made = session(&[("a", 0, 0), ("b", 0, 30)], &[]);
    made.save(|b| {
        bytes = b.to_vec();
        Ok(())
    })
    .unwrap();
    let mut s = open(&bytes);
    assert_eq!(drawn(&s, "b"), (0.0, 73.0));
    assert!(s.is_saved());
    assert_eq!(s.recovery_wake(T), None);
    assert_eq!(saved_bytes(&mut s), bytes);
}

#[test]
fn a_file_with_linked_shapes_closer_than_the_gap_opens_saved_with_them_drawn_at_it() {
    let bytes =
        b"willow 1\n\na: \"\"\nb: \"\" below a\n\n@layout a 0,0 w210\n@layout b 0,60 w210\n";
    let mut s = open(bytes);
    assert_eq!(drawn(&s, "b"), (0.0, 97.0));
    assert_eq!(placed(&s, "b"), (0, 60));
    assert!(s.is_saved());
    assert_eq!(s.recovery_wake(T), None);
    assert_eq!(saved_bytes(&mut s), bytes);
}

#[test]
fn a_shape_swapped_for_another_in_the_same_place_is_pushed_under_its_new_id() {
    let file = |b: &str| {
        let lay = format!("@layout a 0,0 w210\n@layout {b} 0,30 w210\n");
        format!("willow 1\n\na: \"\"\n\n{b}: \"\"\n\n{lay}").into_bytes()
    };
    let mut s = open(&file("b"));
    assert_eq!(drawn(&s, "b"), (0.0, 73.0));
    s.disk_changed(Some(&file("c")));
    assert_eq!(drawn(&s, "c"), (0.0, 73.0));
}

#[test]
fn a_push_never_marks_the_document_unsaved_or_adds_an_undo_step() {
    let s = session(&[("a", 0, 0), ("b", 0, 30)], &[]);
    assert_eq!(drawn(&s, "b"), (0.0, 73.0));
    assert!(s.is_saved());
    assert!(!s.can_undo());
}

/// Counts paragraphs measured.
struct Counting(Rc<Cell<usize>>);

impl Measure for Counting {
    fn advances(&self, p: &Paragraph, clusters: &[Range<usize>], t: &Theme) -> Vec<f32> {
        self.0.set(self.0.get() + 1);
        common::Fake.advances(p, clusters, t)
    }
}

#[test]
fn opening_measures_every_shape_before_anything_reads_it() {
    let bytes =
        b"willow 1\n\na: \"One\"\n\nb: \"Two\"\n\n@layout a 0,0 w210\n@layout b 0,300 w210\n";
    let count = Rc::new(Cell::new(0));
    let measure = Box::new(Counting(count.clone()));
    let s = Session::open(load(bytes).unwrap(), bytes.to_vec(), &PAPER, measure);
    let measured = count.get();
    assert!(measured >= 2, "{measured}");
    assert_eq!(drawn(&s, "b"), (0.0, 300.0));
    assert_eq!(count.get(), measured);
}

/// Every drawn rectangle, by ID.
fn picture(s: &Session) -> HashMap<String, (f32, f32, f32, f32)> {
    let rect = |sh: &Shape| {
        let r = s.drawn_rect(&sh.id).expect("a shape");
        (sh.id.0.clone(), (r.x, r.y, r.width, r.height))
    };
    s.document().shapes.iter().map(rect).collect()
}

const NOBODY: Vec<(String, String)> = Vec::new();

/// The pairs of shapes closer than their gap: the creation gap if linked, else the minimum.
fn crowding(s: &Session) -> Vec<(String, String)> {
    let d = s.document();
    let t = s.theme();
    let linked = |a: &ShapeId, b: &ShapeId| {
        (d.links.iter()).any(|Link(p, q)| (p == a && q == b) || (p == b && q == a))
    };
    let rects: Vec<_> = (d.shapes.iter())
        .map(|a| (a, s.drawn_rect(&a.id).expect("a shape")))
        .collect();
    let mut sorted = rects.clone();
    sorted.sort_by(|(_, p), (_, q)| p.x.total_cmp(&q.x));
    // Rectangles come back as floats: a hair under the gap is rounding.
    let wide = t.creation_gap.max(t.min_gap);
    let mut pairs = Vec::new();
    for (i, (a, p)) in sorted.iter().enumerate() {
        for (b, q) in &sorted[i + 1..] {
            if q.x >= p.right() + wide {
                break;
            }
            let gap = if linked(&a.id, &b.id) {
                t.creation_gap
            } else {
                t.min_gap
            } - 0.01;
            if p.x - gap < q.right()
                && q.x < p.right() + gap
                && p.y - gap < q.bottom()
                && q.y < p.bottom() + gap
            {
                pairs.push((a.id.0.clone(), b.id.0.clone()));
            }
        }
    }
    pairs
}

/// A generated document squeezed to a third of its size: chains land on each other.
fn squeezed(seed: u64) -> Document {
    let mut doc = generate(300, seed);
    for s in &mut doc.shapes {
        (s.x, s.y) = (s.x / 3, s.y / 3);
    }
    doc
}

#[test]
fn generated_documents_end_uncrowded_with_placed_positions_kept() {
    for seed in 1..=4 {
        let doc = squeezed(seed);
        let s = common::session_of(doc.clone());
        assert_eq!(crowding(&s), NOBODY, "seed {seed}");
        assert_eq!(s.document(), &doc);
        assert!(s.is_saved());
        let drawn_off = |sh: &&Shape| {
            let r = s.drawn_rect(&sh.id).expect("a shape");
            (r.x, r.y) != (sh.x as f32, sh.y as f32)
        };
        assert!(doc.shapes.iter().any(|sh| drawn_off(&sh)), "seed {seed}");
    }
}

#[test]
fn growth_in_a_generated_document_leaves_nothing_crowded_saves_no_moves_and_undoes_cleanly() {
    for seed in 1..=4 {
        let doc = generate(300, seed);
        let mut s = common::session_of(doc.clone());
        assert_eq!(crowding(&s), NOBODY, "seed {seed}");
        let start = picture(&s);
        let names = [0, 40, 100, 299].map(|i| doc.shapes[i].id.clone());
        for name in &names {
            s.select([name.clone()]);
            let (text, replace) = ("word ".repeat(40), None);
            s.handle(Input::Text { text, replace }, T);
            s.handle(Input::Escape, T);
            assert_eq!(crowding(&s), NOBODY, "seed {seed}, {name:?}");
        }
        assert_ne!(picture(&s), start, "seed {seed}");
        let same_place = |(a, b): (&Shape, &Shape)| (a.x, a.y) == (b.x, b.y);
        assert!(s.document().shapes.iter().zip(&doc.shapes).all(same_place));
        for _ in names {
            s.handle(Input::Undo, T);
        }
        assert_eq!(s.document(), &doc, "seed {seed}");
        assert_eq!(picture(&s), start, "seed {seed}");
    }
}

#[test]
fn two_sessions_on_one_document_agree_whatever_its_order() {
    for seed in 1..=4 {
        let doc = squeezed(seed);
        let mut turned = doc.clone();
        turned.shapes.reverse();
        turned.links.reverse();
        let a = picture(&common::session_of(doc.clone()));
        assert_eq!(a, picture(&common::session_of(doc)), "seed {seed}");
        assert_eq!(a, picture(&common::session_of(turned)), "seed {seed}");
    }
}

#[test]
fn a_save_and_load_gives_the_same_picture_and_bytes() {
    for seed in 1..=4 {
        let mut s = common::session_of(squeezed(seed));
        let bytes = saved_bytes(&mut s);
        let mut again = open(&bytes);
        assert_eq!(picture(&s), picture(&again), "seed {seed}");
        assert_eq!(saved_bytes(&mut again), bytes, "seed {seed}");
    }
}
