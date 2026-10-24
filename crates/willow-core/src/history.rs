//! One undo history per document, covering model changes only. Every command that changes a
//! shape (move, resize, autofit, text edits) goes through `History::change`; viewport and
//! selection never do.

use crate::{Document, Shape, ShapeId};

/// One undoable step: each touched shape as it is on the other side of the step. Undo and
/// redo both swap these with the document's shapes, so one step serves both directions.
// ponytail: shape snapshots cover changes to existing shapes; creating or deleting shapes
// needs `Option<Shape>` plus the draw-order index.
type Step = Vec<Shape>;

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// A group in progress (an editing session): every change in it becomes one step.
    open: Option<Step>,
}

impl History {
    /// Changes shape `id` through `f`: its own step, or part of the open group. A change that
    /// leaves the shape as it was adds nothing.
    pub fn change(&mut self, doc: &mut Document, id: &ShapeId, f: impl FnOnce(&mut Shape)) {
        let Some(shape) = doc.shapes.iter_mut().find(|s| &s.id == id) else {
            return;
        };
        let before = shape.clone();
        f(shape);
        if *shape == before {
            return;
        }
        self.redo.clear();
        match &mut self.open {
            Some(step) if step.iter().any(|s| &s.id == id) => {}
            Some(step) => step.push(before),
            None => self.undo.push(vec![before]),
        }
    }

    /// Opens a group: changes until `end` become one step.
    pub fn begin(&mut self) {
        self.open.get_or_insert_default();
    }

    /// Closes the group. Shapes it left as they were drop out; an empty group adds no step.
    pub fn end(&mut self, doc: &Document) {
        if let Some(mut step) = self.open.take() {
            step.retain(|snap| !doc.shapes.contains(snap));
            if !step.is_empty() {
                self.undo.push(step);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.open.as_ref().is_some_and(|s| !s.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last step. Inside a group, that is the group so far; the group stays open,
    /// empty, so further changes form the next step. A group that changed nothing undoes nothing.
    pub fn undo(&mut self, doc: &mut Document) {
        let open = self.open.as_ref();
        if open.is_some_and(|step| step.iter().all(|snap| doc.shapes.contains(snap))) {
            return;
        }
        self.across(doc, |h| (&mut h.undo, &mut h.redo));
    }

    pub fn redo(&mut self, doc: &mut Document) {
        self.across(doc, |h| (&mut h.redo, &mut h.undo));
    }

    /// Ends any open group, then moves the top step of one stack onto the other, swapping its
    /// shapes with the document's. Reopens the group if one was open.
    fn across(
        &mut self,
        doc: &mut Document,
        stacks: impl Fn(&mut Self) -> (&mut Vec<Step>, &mut Vec<Step>),
    ) {
        let was_open = self.open.is_some();
        self.end(doc);
        let (from, to) = stacks(self);
        if let Some(mut step) = from.pop() {
            for snap in &mut step {
                if let Some(shape) = doc.shapes.iter_mut().find(|s| s.id == snap.id) {
                    std::mem::swap(shape, snap);
                }
            }
            to.push(step);
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
}
