//! Deleting shapes: neighbors on opposite sides reconnect through a hub, nothing moves, and
//! the whole delete is one undo step.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{Color, Document, Input, Link, Session, Shape, ShapeId};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Empty shapes at their top-left corners (all the same size), and links, all one color.
fn doc(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Document {
    Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    }
}

/// The document's links as sorted pairs, each pair sorted.
fn links(s: &Session) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = (s.document().links.iter())
        .map(|Link(a, b)| {
            let (a, b) = (a.0.clone(), b.0.clone());
            if a < b { (a, b) } else { (b, a) }
        })
        .collect();
    out.sort();
    out
}

fn pairs(ps: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = (ps.iter()).map(|&(a, b)| (a.into(), b.into())).collect();
    out.sort();
    out
}

fn delete(s: &mut Session, ids: &[&str]) {
    s.select(ids.iter().map(|&i| id(i)));
    s.handle(Input::DeleteShapes, T);
}

fn ids(s: &Session) -> Vec<&str> {
    s.document()
        .shapes
        .iter()
        .map(|s| s.id.0.as_str())
        .collect()
}

#[test]
fn deleting_the_middle_of_a_trunk_links_above_to_below() {
    let mut s = session_of(doc(
        &[("a", 0, 0), ("b", 0, 200), ("c", 0, 400)],
        &[("a", "b"), ("b", "c")],
    ));
    delete(&mut s, &["b"]);
    assert_eq!(ids(&s), ["a", "c"]);
    assert_eq!(links(&s), pairs(&[("a", "c")]));
    assert!(s.selection().is_empty());
}

#[test]
fn deleting_the_middle_of_a_left_right_pair_links_left_to_right() {
    let mut s = session_of(doc(
        &[("a", 0, 0), ("b", 300, 0), ("c", 600, 0)],
        &[("a", "b"), ("b", "c")],
    ));
    delete(&mut s, &["b"]);
    assert_eq!(links(&s), pairs(&[("a", "c")]));
}

#[test]
fn the_hub_is_the_nearest_neighbor_on_the_side_with_fewer() {
    // Around d: two above (t nearer), three below, one left with no right.
    let mut s = session_of(doc(
        &[
            ("d", 1000, 1000),
            ("t", 1000, 800),
            ("u", 800, 700),
            ("b", 1000, 1200),
            ("c", 1300, 1300),
            ("e", 700, 1400),
            ("l", 600, 1000),
        ],
        &[
            ("d", "t"),
            ("u", "d"),
            ("d", "b"),
            ("c", "d"),
            ("d", "e"),
            ("l", "d"),
        ],
    ));
    delete(&mut s, &["d"]);
    assert_eq!(links(&s), pairs(&[("b", "t"), ("c", "t"), ("e", "t")]));
}

#[test]
fn a_tie_in_count_goes_above_and_a_tie_in_distance_to_the_earlier_id() {
    // Two above, equally near; two below. "z" comes before "aa".
    let mut s = session_of(doc(
        &[
            ("d", 0, 0),
            ("aa", 100, -300),
            ("z", -100, -300),
            ("b", 0, 300),
            ("c", 50, 500),
        ],
        &[("d", "aa"), ("d", "z"), ("d", "b"), ("d", "c")],
    ));
    delete(&mut s, &["d"]);
    assert_eq!(links(&s), pairs(&[("b", "z"), ("c", "z")]));
}

#[test]
fn a_run_of_deleted_trunk_shapes_closes_into_one_link() {
    let mut s = session_of(doc(
        &[
            ("a", 0, 0),
            ("b", 0, 200),
            ("c", 0, 400),
            ("d", 0, 600),
            ("e", 0, 800),
        ],
        &[("a", "b"), ("b", "c"), ("c", "d"), ("d", "e")],
    ));
    // Selected out of story order.
    delete(&mut s, &["d", "b", "c"]);
    assert_eq!(ids(&s), ["a", "e"]);
    assert_eq!(links(&s), pairs(&[("a", "e")]));
}

#[test]
fn shapes_from_several_chains_reconnect_independently() {
    let mut d = doc(
        &[
            ("a", 0, 0),
            ("b", 0, 200),
            ("c", 0, 400),
            ("x", 1000, 0),
            ("y", 1300, 0),
            ("z", 1600, 0),
        ],
        &[("a", "b"), ("b", "c"), ("x", "y"), ("y", "z")],
    );
    for sh in &mut d.shapes[3..] {
        sh.color = Color::Sky;
    }
    let mut s = session_of(d);
    delete(&mut s, &["y", "b"]);
    assert_eq!(links(&s), pairs(&[("a", "c"), ("x", "z")]));
    let colors: Vec<Color> = s.document().shapes.iter().map(|s| s.color).collect();
    assert_eq!(colors, [Color::Rose, Color::Rose, Color::Sky, Color::Sky]);
}

#[test]
fn nothing_moves_and_one_undo_restores_shapes_links_and_colors() {
    let before = doc(
        &[
            ("a", 0, 0),
            ("b", 40, 200),
            ("c", -30, 450),
            ("d", 300, 210),
            ("e", 600, 0),
        ],
        &[("a", "b"), ("b", "c"), ("d", "b"), ("e", "d")],
    );
    let mut s = session_of(before.clone());
    delete(&mut s, &["b", "e"]);
    let at: Vec<(&str, i32, i32)> = (s.document().shapes.iter())
        .map(|sh| (sh.id.0.as_str(), sh.x, sh.y))
        .collect();
    assert_eq!(at, [("a", 0, 0), ("c", -30, 450), ("d", 300, 210)]);
    s.handle(Input::Undo, T);
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
}

#[test]
fn delete_and_backspace_do_nothing_to_shapes_while_editing() {
    let mut d = doc(&[("a", 0, 0), ("b", 0, 200)], &[("a", "b")]);
    d.shapes[0].text = common::text("abc");
    let mut s = session_of(d);
    s.select([id("a")]);
    s.handle(Input::StartEditing, T);
    s.handle(Input::Delete(willow_core::Motion::Left), T);
    s.handle(Input::Delete(willow_core::Motion::Right), T);
    s.handle(Input::DeleteShapes, T);
    assert_eq!(ids(&s), ["a", "b"]);
    assert_eq!(links(&s), pairs(&[("a", "b")]));
    assert_eq!(s.editing().map(|e| e.id.clone()), Some(id("a")));
}

/// xorshift64: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// 2 to 14 shapes on a coarse grid (so centers tie and diagonals occur), random colors,
/// each shape after the first linked to an earlier one three times in four through
/// `Document::link` (which keeps chains one color).
fn generated(r: &mut Rng) -> Document {
    let mut doc = Document::default();
    for _ in 0..2 + r.below(13) {
        let (x, y) = (r.below(7) as i32 * 250 - 750, r.below(7) as i32 * 180);
        let color = Color::ALL[r.below(7) as usize];
        let id = doc.next_id();
        doc.shapes.push(Shape {
            color,
            ..Shape::new(id, x, y)
        });
        let i = doc.shapes.len() - 1;
        if i > 0 && r.below(4) > 0 {
            let (a, b) = (
                doc.shapes[r.below(i as u64) as usize].id.clone(),
                id_at(&doc, i),
            );
            assert!(doc.link(&a, &b));
        }
    }
    doc
}

fn id_at(doc: &Document, i: usize) -> ShapeId {
    doc.shapes[i].id.clone()
}

#[test]
fn deleting_random_sets_keeps_chains_one_color_trees_joined_only_across_deleted_shapes() {
    use std::collections::{HashMap, HashSet};
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..2000 {
        let before = generated(&mut r);
        let gone: HashSet<ShapeId> = (before.shapes.iter())
            .filter(|_| r.below(3) == 0)
            .map(|s| s.id.clone())
            .collect();
        let mut s = session_of(before.clone());
        s.select(gone.iter().cloned());
        s.handle(Input::DeleteShapes, T);
        let after = s.document();

        // The rest stay, unmoved.
        let kept: Vec<&Shape> = (before.shapes.iter())
            .filter(|s| !gone.contains(&s.id))
            .collect();
        assert_eq!(after.shapes.iter().collect::<Vec<_>>(), kept, "case {case}");

        // A forest: every link joins two shapes not yet connected. Linked shapes share a color.
        let color: HashMap<&ShapeId, Color> =
            after.shapes.iter().map(|s| (&s.id, s.color)).collect();
        let mut root: HashMap<&ShapeId, &ShapeId> =
            after.shapes.iter().map(|s| (&s.id, &s.id)).collect();
        fn find<'a>(root: &HashMap<&'a ShapeId, &'a ShapeId>, mut x: &'a ShapeId) -> &'a ShapeId {
            while root[x] != x {
                x = root[x];
            }
            x
        }
        for Link(a, b) in &after.links {
            assert_eq!(color[a], color[b], "case {case}: one color");
            let (ra, rb) = (find(&root, a), find(&root, b));
            assert_ne!(ra, rb, "case {case}: a cycle");
            root.insert(ra, rb);
        }

        // A new link's ends were joined in the old document through deleted shapes only.
        let old: HashSet<(&ShapeId, &ShapeId)> =
            before.links.iter().map(|Link(a, b)| (a, b)).collect();
        for Link(a, b) in &after.links {
            if old.contains(&(a, b)) || old.contains(&(b, a)) {
                continue;
            }
            let (mut seen, mut next) = (HashSet::from([a]), vec![a]);
            let mut reached = false;
            while let Some(x) = next.pop() {
                for Link(p, q) in &before.links {
                    let n = match () {
                        _ if p == x => q,
                        _ if q == x => p,
                        _ => continue,
                    };
                    if n == b && x != a {
                        reached = true;
                    }
                    if gone.contains(n) && seen.insert(n) {
                        next.push(n);
                    }
                }
            }
            assert!(
                reached,
                "case {case}: {a:?}-{b:?} weren't neighbors of deleted shapes"
            );
        }

        s.handle(Input::Undo, T);
        assert_eq!(*s.document(), before, "case {case}: undo");
    }
}
