//! Dropping shapes: each joins the innermost zone under its center, or none.

use std::collections::{HashMap, HashSet};

use super::{Gesture, Session};
use crate::{Point, Shape, ShapeId};

/// Where a shape goes: the zone it is in, and the zone it would join.
type Change = (Option<ShapeId>, Option<ShapeId>);

impl Session {
    /// Moves shapes `ids` by (dx, dy), each joining the zone under its new center (or none),
    /// as one undo step. Pushes nothing.
    pub(super) fn drop_shapes(&mut self, ids: &[ShapeId], dx: i32, dy: i32) {
        let changes: Vec<Option<Change>> = (ids.iter())
            .map(|id| {
                let s = self.doc.shapes.iter().find(|s| &s.id == id)?;
                self.drop_change(s, dx, dy)
            })
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

    /// Shape `s`'s zone and the one it would join moved by (dx, dy), if they differ.
    fn drop_change(&self, s: &Shape, dx: i32, dy: i32) -> Option<Change> {
        let to = self.zone_under_moved(s, dx, dy);
        let from = self.doc.parent_zone(&s.id).cloned();
        (from != to).then_some((from, to))
    }

    /// The innermost zone under shape `s`'s center moved by (dx, dy).
    fn zone_under_moved(&self, s: &Shape, dx: i32, dy: i32) -> Option<ShapeId> {
        let c = self.drawn(s).center();
        self.innermost_zone(Point {
            x: c.x + dx as f32,
            y: c.y + dy as f32,
        })
    }

    /// While a shape or a multi-selection is dragged: the zone a drop now would make a shape
    /// join that isn't in it. The pressed shape's target first, else that of the first
    /// selected shape in story order.
    pub(crate) fn drop_zone(&self) -> Option<&ShapeId> {
        let Some(Gesture::Press {
            index,
            to: Some((x, y)),
            ..
        }) = self.gesture
        else {
            return None;
        };
        let (s, (dx, dy)) = (&self.doc.shapes[index], self.dragged(index, (x, y)));
        let to = match self.drop_change(s, dx, dy) {
            Some((_, Some(to))) => to,
            _ => self.group_drag().and(self.first_drop_target(dx, dy))?,
        };
        (self.doc.zones.iter()).find(|z| z.id == to).map(|z| &z.id)
    }

    /// The zone the first selected shape in story order would join moved by (dx, dy), among
    /// those it would change.
    fn first_drop_target(&self, dx: i32, dy: i32) -> Option<ShapeId> {
        // Hashed, not scanned: a selection may hold thousands of zone members.
        let parent: HashMap<&ShapeId, &ShapeId> = (self.doc.zones.iter())
            .flat_map(|z| z.members.iter().map(move |m| (m, &z.id)))
            .collect();
        let selected: HashSet<&ShapeId> = self.selected.iter().collect();
        // ponytail: story order every frame while no pressed-shape target; cache it per
        // gesture if dragging thousands of shapes misses the frame target.
        (crate::format::story_order(&self.doc, &self.heights()).into_iter())
            .map(|i| &self.doc.shapes[i])
            .filter(|s| selected.contains(&s.id))
            .find_map(|s| {
                let to = self.zone_under_moved(s, dx, dy)?;
                (parent.get(&s.id) != Some(&&to)).then_some(to)
            })
    }
}
