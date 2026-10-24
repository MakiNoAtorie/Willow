//! One undo history per document, covering model changes only. Every command that changes the
//! document goes through `History`: shape edits through `change`, structural edits (shapes
//! added or removed, links, chain colors) through `add` / `remove` / `link` / `unlink` /
//! `recolor`, several changes as one step through `step`, a reload from disk through
//! `replace`. Viewport and selection never do.

use std::collections::{HashMap, HashSet};

use crate::{Color, Document, Link, Shape, ShapeId};

/// One recorded change, as what takes the document back across it. Applying an op returns
/// the op that takes it forward again.
#[derive(Debug)]
enum Op {
    /// Swap with the shape of this ID.
    Set(Shape),
    /// Put this shape back at this draw-order index.
    Insert(usize, Shape),
    /// Take out the shape at this index.
    Remove(usize),
    /// Put this link back at this index.
    Link(usize, Link),
    /// Take out the link at this index.
    Unlink(usize),
    /// Swap the member list of the zone with this ID.
    Members(ShapeId, Vec<ShapeId>),
}

impl Op {
    /// Applies the op and returns its inverse. `at` caches shape indices by ID.
    fn apply(self, doc: &mut Document, at: &mut Option<HashMap<ShapeId, usize>>) -> Op {
        match self {
            Op::Set(mut s) => {
                let at = at.get_or_insert_with(|| {
                    (doc.shapes.iter().enumerate())
                        .map(|(i, s)| (s.id.clone(), i))
                        .collect()
                });
                if let Some(&i) = at.get(&s.id) {
                    std::mem::swap(&mut doc.shapes[i], &mut s);
                }
                Op::Set(s)
            }
            Op::Insert(i, s) => {
                *at = None;
                doc.shapes.insert(i, s);
                Op::Remove(i)
            }
            Op::Remove(i) => {
                *at = None;
                Op::Insert(i, doc.shapes.remove(i))
            }
            Op::Link(i, l) => {
                doc.links.insert(i, l);
                Op::Unlink(i)
            }
            Op::Unlink(i) => Op::Link(i, doc.links.remove(i)),
            Op::Members(id, mut m) => {
                if let Some(z) = doc.zones.iter_mut().find(|z| z.id == id) {
                    std::mem::swap(&mut z.members, &mut m);
                }
                Op::Members(id, m)
            }
        }
    }

    /// A `Set` the document already matches: it changes nothing.
    fn is_noop(&self, doc: &Document) -> bool {
        matches!(self, Op::Set(s) if doc.shapes.contains(s))
    }
}

/// One undoable step, as the document is on the other side of it.
#[derive(Debug)]
enum Step {
    /// Ops in the order they were recorded; applied last first.
    Ops(Vec<Op>),
    /// The whole document (a reload from disk).
    Document(Box<Document>),
}

/// A group in progress: its ops, and the shapes it already holds a `Set` for.
#[derive(Debug, Default)]
struct Group {
    ops: Vec<Op>,
    set: HashSet<ShapeId>,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// A group in progress (an editing session, a command): its changes become one step.
    open: Option<Group>,
    /// Counts changes to the document (including undo and redo); crash recovery watches it.
    revision: u64,
}

impl History {
    /// Records that the document changed, and `op` (if any) to take it back: its own step, or
    /// part of the open group.
    fn record(&mut self, op: Option<Op>) {
        self.revision += 1;
        self.redo.clear();
        match (&mut self.open, op) {
            (_, None) => {}
            (Some(g), Some(op)) => g.ops.push(op),
            (None, Some(op)) => self.undo.push(Step::Ops(vec![op])),
        }
    }

    /// Changes shape `id` through `f`: its own step, or part of the open group. A change that
    /// leaves the shape as it was adds nothing.
    pub fn change(&mut self, doc: &mut Document, id: &ShapeId, f: impl FnOnce(&mut Shape)) {
        if let Some(i) = doc.shapes.iter().position(|s| &s.id == id) {
            self.change_at(doc, i, f);
        }
    }

    /// `change` for the shape at draw-order index `i`.
    fn change_at(&mut self, doc: &mut Document, i: usize, f: impl FnOnce(&mut Shape)) {
        let shape = &mut doc.shapes[i];
        let before = shape.clone();
        f(shape);
        if *shape == before {
            return;
        }
        // A group's first `Set` for a shape restores it whole; later ones would add nothing.
        let first = (self.open.as_mut()).is_none_or(|g| g.set.insert(before.id.clone()));
        self.record(first.then_some(Op::Set(before)));
    }

    /// Runs `f` as one step; inside an open group, its changes join the group.
    pub fn step(&mut self, doc: &mut Document, f: impl FnOnce(&mut Self, &mut Document)) {
        let opened = self.open.is_none();
        self.begin();
        f(self, doc);
        if opened {
            self.end(doc);
        }
    }

    /// Adds `shape` on top of every other (last in draw order). Its ID must be new.
    pub fn add(&mut self, doc: &mut Document, shape: Shape) {
        doc.shapes.push(shape);
        self.record(Some(Op::Remove(doc.shapes.len() - 1)));
    }

    /// Makes `shape` a member of zone `zone`, recorded.
    pub fn join_zone(&mut self, doc: &mut Document, zone: &ShapeId, shape: &ShapeId) {
        if let Some(z) = doc.zones.iter_mut().find(|z| &z.id == zone) {
            let old = z.members.clone();
            z.members.push(shape.clone());
            self.record(Some(Op::Members(zone.clone(), old)));
        }
    }

    /// Removes shape `id` and its links, as one step. Nothing else changes: no reconnecting.
    #[cfg_attr(not(test), allow(dead_code))] // delete, cut (#36 on)
    pub fn remove(&mut self, doc: &mut Document, id: &ShapeId) -> Option<Shape> {
        let i = doc.shapes.iter().position(|s| &s.id == id)?;
        let shape = doc.shapes[i].clone();
        self.step(doc, |h, doc| {
            while let Some(j) = (doc.links.iter()).rposition(|Link(a, b)| a == id || b == id) {
                let l = doc.links.remove(j);
                h.record(Some(Op::Link(j, l)));
            }
            let shape = doc.shapes.remove(i);
            h.record(Some(Op::Insert(i, shape)));
        });
        Some(shape)
    }

    /// `Document::link`, recorded as one step: refused (false) for the same shape, a missing
    /// one or a cycle; a join repaints `b`'s chain in `a`'s color.
    pub fn link(&mut self, doc: &mut Document, a: &ShapeId, b: &ShapeId) -> bool {
        let Some(color) = doc.link_color(a, b) else {
            return false;
        };
        self.step(doc, |h, doc| {
            h.recolor(doc, b, color);
            doc.links.push(Link(a.clone(), b.clone()));
            h.record(Some(Op::Unlink(doc.links.len() - 1)));
        });
        true
    }

    /// `Document::unlink`, recorded: both halves keep the color. False if there was no link.
    pub fn unlink(&mut self, doc: &mut Document, a: &ShapeId, b: &ShapeId) -> bool {
        let found = (doc.links.iter()).position(|Link(x, y)| (x, y) == (a, b) || (x, y) == (b, a));
        let Some(i) = found else {
            return false;
        };
        let l = doc.links.remove(i);
        self.record(Some(Op::Link(i, l)));
        true
    }

    /// Paints `id`'s whole chain `color`, as one step.
    pub fn recolor(&mut self, doc: &mut Document, id: &ShapeId, color: Color) {
        let chain: HashSet<ShapeId> = doc.chain(id).into_iter().cloned().collect();
        self.step(doc, |h, doc| {
            for i in 0..doc.shapes.len() {
                if chain.contains(&doc.shapes[i].id) {
                    h.change_at(doc, i, |s| s.color = color);
                }
            }
        });
    }

    /// Replaces the whole document with `new` as one step, closing any open group first.
    pub fn replace(&mut self, doc: &mut Document, new: Document) {
        self.end(doc);
        self.revision += 1;
        self.redo.clear();
        let old = std::mem::replace(doc, new);
        self.undo.push(Step::Document(Box::new(old)));
    }

    /// Opens a group: changes until `end` become one step.
    pub fn begin(&mut self) {
        self.open.get_or_insert_default();
    }

    /// Closes the group. Shapes it left as they were drop out; an empty group adds no step.
    pub fn end(&mut self, doc: &Document) {
        if let Some(g) = self.open.take() {
            let ops: Vec<Op> = g.ops.into_iter().filter(|op| !op.is_noop(doc)).collect();
            if !ops.is_empty() {
                self.undo.push(Step::Ops(ops));
            }
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.open.as_ref().is_some_and(|g| !g.ops.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last step. Inside a group, that is the group so far; the group stays open,
    /// empty, so further changes form the next step. A group that changed nothing undoes nothing.
    pub fn undo(&mut self, doc: &mut Document) {
        let open = self.open.as_ref();
        if open.is_some_and(|g| g.ops.iter().all(|op| op.is_noop(doc))) {
            return;
        }
        self.across(doc, |h| (&mut h.undo, &mut h.redo));
    }

    pub fn redo(&mut self, doc: &mut Document) {
        self.across(doc, |h| (&mut h.redo, &mut h.undo));
    }

    /// Ends any open group, then moves the top step of one stack onto the other, applying it
    /// and keeping what takes it back. Reopens the group if one was open.
    fn across(
        &mut self,
        doc: &mut Document,
        stacks: impl Fn(&mut Self) -> (&mut Vec<Step>, &mut Vec<Step>),
    ) {
        let was_open = self.open.is_some();
        self.end(doc);
        let (from, to) = stacks(self);
        if let Some(step) = from.pop() {
            to.push(match step {
                // Last op first. The inverses, kept in that order, replay reversed again.
                Step::Ops(ops) => {
                    let mut at = None;
                    Step::Ops(
                        ops.into_iter()
                            .rev()
                            .map(|op| op.apply(doc, &mut at))
                            .collect(),
                    )
                }
                Step::Document(mut d) => {
                    std::mem::swap(doc, &mut d);
                    Step::Document(d)
                }
            });
            self.revision += 1;
        }
        if was_open {
            self.begin();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x(doc: &Document) -> i32 {
        doc.shapes[0].x
    }

    #[test]
    fn a_group_is_one_step_and_undo_inside_it_reverts_the_group_so_far() {
        let (mut doc, mut h) = (Document::new(), History::default());
        let a = doc.shapes[0].id.clone();

        h.begin();
        h.change(&mut doc, &a, |s| s.x = 5);
        h.change(&mut doc, &a, |s| s.x = 9);
        assert!(h.can_undo());

        // Undo while the group is open: back to before the group, still grouping.
        h.undo(&mut doc);
        assert_eq!(x(&doc), 0);
        assert!(!h.can_undo() && h.can_redo());
        h.redo(&mut doc);
        assert_eq!(x(&doc), 9);

        h.change(&mut doc, &a, |s| s.width = 300);
        h.end(&doc);
        h.undo(&mut doc);
        assert_eq!((x(&doc), doc.shapes[0].width), (9, 210));
        h.undo(&mut doc);
        assert_eq!(x(&doc), 0);
        assert!(!h.can_undo());
    }

    fn id(s: &str) -> ShapeId {
        ShapeId(s.into())
    }

    /// a-b linked (rose), c alone (sky), in draw order a, b, c.
    fn three() -> Document {
        let mut doc = Document::default();
        for (n, x) in [("a", 0), ("b", 0), ("c", 400)] {
            doc.shapes.push(Shape::new(id(n), x, 0));
        }
        doc.shapes[1].y = 200;
        doc.link(&id("a"), &id("b"));
        doc.recolor(&id("a"), crate::Color::Rose);
        doc.recolor(&id("c"), crate::Color::Sky);
        doc
    }

    #[test]
    fn undo_and_redo_of_a_structural_step_restore_the_document_exactly() {
        let mut doc = three();
        let mut h = History::default();
        let before = doc.clone();

        h.step(&mut doc, |h, doc| {
            h.add(doc, Shape::new(id("d"), 0, 400));
            assert!(h.link(doc, &id("b"), &id("d")));
            h.remove(doc, &id("a"));
            h.recolor(doc, &id("c"), crate::Color::Sage);
            assert!(h.link(doc, &id("c"), &id("b")));
        });
        let after = doc.clone();
        assert_ne!(before, after);
        assert!(!after.shapes.iter().any(|s| s.id == id("a")));
        assert!(!after.links.iter().any(|l| l.0 == id("a") || l.1 == id("a")));
        assert!(after.shapes.iter().all(|s| s.color == crate::Color::Sage));

        for _ in 0..2 {
            h.undo(&mut doc);
            assert_eq!(doc, before);
            h.redo(&mut doc);
            assert_eq!(doc, after);
        }
        h.undo(&mut doc);
        assert!(!h.can_undo());
    }

    #[test]
    fn a_group_mixing_shape_edits_with_a_new_linked_shape_is_one_step() {
        let mut doc = three();
        let mut h = History::default();
        let before = doc.clone();

        h.begin();
        h.change(&mut doc, &id("b"), |s| s.x = 50);
        h.step(&mut doc, |h, doc| {
            h.add(doc, Shape::new(id("d"), 0, 400));
            h.link(doc, &id("b"), &id("d"));
        });
        h.change(&mut doc, &id("d"), |s| s.width = 300);
        h.change(&mut doc, &id("b"), |s| s.y = 250);
        h.end(&doc);
        let after = doc.clone();

        h.undo(&mut doc);
        assert_eq!(doc, before);
        assert!(!h.can_undo());
        h.redo(&mut doc);
        assert_eq!(doc, after);
    }
}
