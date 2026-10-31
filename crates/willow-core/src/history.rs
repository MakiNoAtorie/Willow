//! One undo history per document, covering model changes only. Every command that changes the
//! document goes through `History`: shape edits through `change`, structural edits (shapes
//! added or removed, links, chain colors) through `add` / `remove` / `link` / `unlink` /
//! `recolor`, several changes as one step through `step`, a reload from disk through
//! `replace`. Viewport and selection never do.
//!
//! Zones: `add_zone` / `remove_zone` add and remove one, keeping every ID in at most one
//! member list and no member list naming a missing zone; `change_zone` changes position, size,
//! title or color; `join_zone` and `leave_zone` change membership; `order_zones` sets the draw
//! order, parents first. Each is undoable and joins an open group like the shape steps.
//!
//! Panel text: `text` is the code panel's text for the document as it is now, when that isn't
//! plain regenerated text. Every new step records it as the text on its far side, and undo and
//! redo swap it, so the session shows it again. `amend` joins a text edit to the last step;
//! `text_step` is a step that changes only the text.

use std::collections::{HashMap, HashSet};

use crate::{Color, Document, Link, Shape, ShapeId, Zone};

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
    /// Put this zone back at this draw-order index.
    InsertZone(usize, Zone),
    /// Take out the zone at this index.
    RemoveZone(usize),
    /// Swap with the zone of this ID.
    SetZone(Zone),
    /// Put the zones in this ID order.
    Order(Vec<ShapeId>),
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
            Op::InsertZone(i, z) => {
                doc.zones.insert(i, z);
                Op::RemoveZone(i)
            }
            Op::RemoveZone(i) => Op::InsertZone(i, doc.zones.remove(i)),
            Op::SetZone(mut z) => {
                if let Some(d) = doc.zones.iter_mut().find(|d| d.id == z.id) {
                    std::mem::swap(d, &mut z);
                }
                Op::SetZone(z)
            }
            Op::Order(order) => {
                let old = doc.zones.iter().map(|z| z.id.clone()).collect();
                doc.zones
                    .sort_by_cached_key(|z| order.iter().position(|id| id == &z.id));
                Op::Order(old)
            }
        }
    }

    /// A `Set` or `SetZone` the document already matches: it changes nothing. (Members
    /// change only through `Members` ops, which restore their own.)
    fn is_noop(&self, doc: &Document) -> bool {
        match self {
            Op::Set(s) => doc.shapes.contains(s),
            Op::SetZone(z) => doc.zones.contains(z),
            _ => false,
        }
    }
}

/// One undoable step, as the document and the panel text are on the other side of it.
#[derive(Debug)]
struct Step {
    change: Change,
    /// The panel text there; `None` when plain regenerated text.
    text: Option<String>,
}

#[derive(Debug)]
enum Change {
    /// Ops in the order they were recorded; applied last first.
    Ops(Vec<Op>),
    /// The whole document (a reload from disk, a text edit).
    Document(Box<Document>),
}

/// A group in progress: its ops, the panel text before them, and the shapes and zones it holds
/// a `Set` or `SetZone` for.
#[derive(Debug, Default)]
struct Group {
    ops: Vec<Op>,
    text: Option<String>,
    set: HashSet<ShapeId>,
}

/// The latest unbroken run of changes that left every shape's box alone but for text height:
/// revisions `from` to `to`, the shapes they touched by index.
#[derive(Debug, Default)]
struct TextEdits {
    from: u64,
    to: u64,
    shapes: Vec<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// A group in progress (an editing session, a command): its changes become one step.
    open: Option<Group>,
    /// Counts changes to the document (including undo and redo); crash recovery watches it.
    revision: u64,
    text_edits: TextEdits,
    /// The panel text for the document as it is now, when it isn't plain regenerated text.
    pub text: Option<String>,
}

impl History {
    /// Records that the document changed, and `op` (if any) to take it back: its own step, or
    /// part of the open group.
    fn record(&mut self, op: Option<Op>) {
        self.revision += 1;
        self.redo.clear();
        match (&mut self.open, op) {
            (_, None) => {}
            (Some(g), Some(op)) => {
                if g.ops.is_empty() {
                    g.text = self.text.take();
                }
                g.ops.push(op);
            }
            (None, Some(op)) => self.push(Change::Ops(vec![op])),
        }
    }

    /// Pushes a new step to undo, with the panel text before it.
    fn push(&mut self, change: Change) {
        let text = self.text.take();
        self.undo.push(Step { change, text });
    }

    /// Changes shape `id` through `f`: its own step, or part of the open group. A change that
    /// leaves the shape as it was adds nothing.
    pub fn change(&mut self, doc: &mut Document, id: &ShapeId, f: impl FnOnce(&mut Shape)) {
        if let Some(i) = doc.shapes.iter().position(|s| &s.id == id) {
            self.change_at(doc, i, f);
        }
    }

    /// `change` for the shape at draw-order index `i`.
    pub fn change_at(&mut self, doc: &mut Document, i: usize, f: impl FnOnce(&mut Shape)) {
        let shape = &mut doc.shapes[i];
        let before = shape.clone();
        f(shape);
        if *shape == before {
            return;
        }
        let same_box = (shape.x, shape.y, shape.width, shape.extra_height)
            == (before.x, before.y, before.width, before.extra_height);
        // A group's first `Set` for a shape restores it whole; later ones would add nothing.
        let first = (self.open.as_mut()).is_none_or(|g| g.set.insert(before.id.clone()));
        self.record(first.then_some(Op::Set(before)));
        if same_box {
            let edits = &mut self.text_edits;
            if edits.to + 1 != self.revision {
                *edits = TextEdits {
                    from: self.revision - 1,
                    ..TextEdits::default()
                };
            }
            edits.to = self.revision;
            if edits.shapes.last() != Some(&i) {
                edits.shapes.push(i);
            }
        }
    }

    /// The shapes (by index) that may have changed since `revision`, when nothing but their
    /// text did; `None` when anything else may have.
    pub(crate) fn text_changed_since(&self, revision: u64) -> Option<&[usize]> {
        let edits = &self.text_edits;
        (edits.to == self.revision && edits.from <= revision).then_some(&edits.shapes[..])
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

    /// Takes shape `id` out of the zone that holds it, recorded. Nothing if none does.
    pub fn leave_zone(&mut self, doc: &mut Document, id: &ShapeId) {
        self.leave_zones(doc, std::slice::from_ref(id));
    }

    /// Removes shape `id`, its links and its zone memberships, as one step. Nothing else
    /// changes: no reconnecting.
    pub fn remove(&mut self, doc: &mut Document, id: &ShapeId) -> Option<Shape> {
        let i = doc.shapes.iter().position(|s| &s.id == id)?;
        let shape = doc.shapes[i].clone();
        self.step(doc, |h, doc| {
            while let Some(j) = (doc.links.iter()).rposition(|Link(a, b)| a == id || b == id) {
                let l = doc.links.remove(j);
                h.record(Some(Op::Link(j, l)));
            }
            h.leave_zones(doc, std::slice::from_ref(id));
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
        self.push(Change::Document(Box::new(old)));
    }

    /// Replaces the whole document with `new` as part of the last step, which then takes it
    /// back to before both. The caller knows no other change came since that step.
    pub fn amend(&mut self, doc: &mut Document, new: Document) {
        self.revision += 1;
        self.redo.clear();
        *doc = new;
    }

    /// A step that changes only the panel text, from `text` to what the caller makes it.
    pub fn text_step(&mut self, doc: &Document) {
        self.end(doc);
        self.revision += 1;
        self.redo.clear();
        self.push(Change::Ops(Vec::new()));
    }

    /// Opens a group: changes until `end` become one step.
    /// Whether a group is open: `begin`/`end` around more changes would close it early.
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn begin(&mut self) {
        self.open.get_or_insert_default();
    }

    /// Closes the group. Shapes it left as they were drop out; an empty group adds no step.
    pub fn end(&mut self, doc: &Document) {
        if let Some(g) = self.open.take() {
            let ops: Vec<Op> = g.ops.into_iter().filter(|op| !op.is_noop(doc)).collect();
            if !ops.is_empty() {
                let text = g.text;
                self.undo.push(Step {
                    change: Change::Ops(ops),
                    text,
                });
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
    /// and keeping what takes it back, the panel text included. Reopens the group if one was
    /// open.
    fn across(
        &mut self,
        doc: &mut Document,
        stacks: impl Fn(&mut Self) -> (&mut Vec<Step>, &mut Vec<Step>),
    ) {
        let was_open = self.open.is_some();
        self.end(doc);
        if let Some(step) = stacks(self).0.pop() {
            let change = match step.change {
                // Last op first. The inverses, kept in that order, replay reversed again.
                Change::Ops(ops) => {
                    let mut at = None;
                    Change::Ops(
                        ops.into_iter()
                            .rev()
                            .map(|op| op.apply(doc, &mut at))
                            .collect(),
                    )
                }
                Change::Document(mut d) => {
                    std::mem::swap(doc, &mut d);
                    Change::Document(d)
                }
            };
            let text = std::mem::replace(&mut self.text, step.text);
            stacks(self).1.push(Step { change, text });
            self.revision += 1;
        }
        if was_open {
            self.begin();
        }
    }

    /// Takes `ids` out of every zone that lists them, recorded.
    fn leave_zones(&mut self, doc: &mut Document, ids: &[ShapeId]) {
        for z in doc
            .zones
            .iter_mut()
            .filter(|z| z.members.iter().any(|m| ids.contains(m)))
        {
            let old = z.members.clone();
            z.members.retain(|m| !ids.contains(m));
            self.record(Some(Op::Members(z.id.clone(), old)));
        }
    }
}

// The zone steps.
impl History {
    /// Adds `zone` as one step, its ID new. Its members leave the zones that held them; the
    /// caller joins it to a parent with `join_zone`. It draws on top, below any zone it holds.
    pub fn add_zone(&mut self, doc: &mut Document, zone: Zone) {
        self.step(doc, |h, doc| {
            h.leave_zones(doc, &zone.members);
            let i = (doc.zones.iter().position(|z| zone.members.contains(&z.id)))
                .unwrap_or(doc.zones.len());
            doc.zones.insert(i, zone);
            h.record(Some(Op::RemoveZone(i)));
        });
    }

    /// Removes zone `id` as one step: its parent stops listing it, its nested zones become
    /// top-level and its shapes loose. Nothing else changes.
    pub fn remove_zone(&mut self, doc: &mut Document, id: &ShapeId) -> Option<Zone> {
        let i = doc.zones.iter().position(|z| &z.id == id)?;
        let zone = doc.zones[i].clone();
        self.step(doc, |h, doc| {
            h.leave_zones(doc, std::slice::from_ref(id));
            h.record(Some(Op::InsertZone(i, doc.zones.remove(i))));
        });
        Some(zone)
    }

    /// Changes zone `id`'s position, size, title or color through `f`: its own step, or part
    /// of the open group. Members change only through `join_zone` and the other zone steps.
    /// A change that leaves the zone as it was adds nothing.
    pub fn change_zone(&mut self, doc: &mut Document, id: &ShapeId, f: impl FnOnce(&mut Zone)) {
        let Some(zone) = doc.zones.iter_mut().find(|z| &z.id == id) else {
            return;
        };
        let before = zone.clone();
        f(zone);
        debug_assert_eq!(zone.members, before.members, "change_zone changed members");
        if *zone == before {
            return;
        }
        // Zones share the shapes' ID space, so one set serves both.
        let first = (self.open.as_mut()).is_none_or(|g| g.set.insert(before.id.clone()));
        self.record(first.then_some(Op::SetZone(before)));
    }

    /// Sets the zones' draw order to `order` (every zone's ID, bottom first), recorded. Parents
    /// still draw first: a nested zone listed before its parent draws right after it.
    pub fn order_zones(&mut self, doc: &mut Document, order: &[ShapeId]) {
        // Hashed, not scanned: the generated 10k document has hundreds of zones, thousands of members.
        let mut parent = std::collections::HashMap::new();
        for z in &doc.zones {
            parent.extend(z.members.iter().map(|m| (m, &z.id)));
        }
        let at: std::collections::HashMap<_, _> =
            order.iter().enumerate().map(|(i, id)| (id, i)).collect();
        // The zones of `order[..i]` by parent.
        let mut kids: std::collections::HashMap<&ShapeId, Vec<&ShapeId>> = Default::default();
        let mut new = Vec::new();
        for (i, id) in order.iter().enumerate() {
            let p = parent.get(id).copied();
            if let Some(p) = p {
                kids.entry(p).or_default().push(id);
            }
            if p.is_some_and(|p| at.get(p).is_some_and(|&j| j >= i)) {
                continue;
            }
            new.push(id.clone());
            // `id` itself is not its own child, and was added to its parent's list only.
            new.extend(kids.get(id).into_iter().flatten().map(|c| (*c).clone()));
        }
        if doc.zones.iter().map(|z| &z.id).eq(new.iter()) {
            return;
        }
        let undo = Op::Order(new).apply(doc, &mut None);
        self.record(Some(undo));
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

#[cfg(test)]
mod zone_tests;
