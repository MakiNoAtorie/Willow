//! Dropping shapes: each joins the innermost zone under its center, or none.

use super::{Gesture, Session};
use crate::{Point, ShapeId};

/// Where a shape goes: the zone it is in, and the zone it would join.
type Change = (Option<ShapeId>, Option<ShapeId>);

impl Session {
    /// Moves shapes `ids` by (dx, dy), each joining the zone under its new center (or none),
    /// as one undo step. Pushes nothing.
    pub(super) fn drop_shapes(&mut self, ids: &[ShapeId], dx: i32, dy: i32) {
        let changes: Vec<Option<Change>> = (ids.iter())
            .map(|id| self.drop_change(id, dx, dy))
            .collect();
        self.history.step(&mut self.doc, |h, doc| {
            for (id, change) in ids.iter().zip(changes) {
                h.change(doc, id, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
                let Some((_, to)) = change else { continue };
                h.leave_zone(doc, id);
                if let Some(z) = to {
                    h.join_zone(doc, &z, id);
                }
            }
        });
    }

    /// Shape `id`'s zone and the one it would join moved by (dx, dy), if they differ.
    fn drop_change(&self, id: &ShapeId, dx: i32, dy: i32) -> Option<Change> {
        let s = self.doc.shapes.iter().find(|s| &s.id == id)?;
        let c = s.center(self.shape_height(s));
        let to = self.innermost_zone(Point {
            x: c.x + dx as f32,
            y: c.y + dy as f32,
        });
        let from = (self.doc.zones.iter()).find(|z| z.members.contains(id));
        let from = from.map(|z| z.id.clone());
        (from != to).then_some((from, to))
    }

    /// While a shape (or a multi-selection, by the pressed shape) is dragged: the zone a drop
    /// now would make it join, if that changes its membership.
    pub(crate) fn drop_zone(&self) -> Option<&ShapeId> {
        let Some(Gesture::Press {
            index,
            to: Some((x, y)),
            ..
        }) = self.gesture
        else {
            return None;
        };
        let s = &self.doc.shapes[index];
        let to = self.drop_change(&s.id, x - s.x, y - s.y)?.1?;
        (self.doc.zones.iter()).find(|z| z.id == to).map(|z| &z.id)
    }
}
