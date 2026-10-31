//! Deleting shapes. Each deleted shape's neighbors on opposite sides (above and below, left
//! and right) reconnect through a hub; nothing moves.

use std::collections::{HashMap, HashSet};

use super::Session;
use crate::{Document, Link, Point, ShapeId, Side, side};

impl Session {
    /// Deletes the shapes `ids` (missing ones are skipped) as one undo step: one at a time in
    /// story order, each reconnecting its own neighbors, so a run of deleted shapes in a chain
    /// closes into one link. Nothing moves; deleting the edited shape ends editing first. What
    /// Delete, Backspace and Cut do to the selection.
    pub fn delete(&mut self, ids: &[ShapeId]) {
        if self.editing.as_ref().is_some_and(|e| ids.contains(&e.id)) {
            self.stop_editing();
        }
        let doomed: HashSet<&ShapeId> = ids.iter().collect();
        let shapes = &self.doc.shapes;
        let order: Vec<ShapeId> = (self.story_order().into_iter())
            .map(|i| shapes[i].id.clone())
            .filter(|id| doomed.contains(id))
            .collect();
        if order.is_empty() {
            return;
        }
        let center: HashMap<ShapeId, Point> = (shapes.iter().enumerate())
            .map(|(i, s)| (s.id.clone(), self.drawn_index(i).center()))
            .collect();
        self.history.step(&mut self.doc, |h, doc| {
            for id in &order {
                let relink = reconnect(doc, id, &center);
                h.remove(doc, id);
                for (hub, n) in relink {
                    h.link(doc, &hub, &n);
                }
            }
        });
        self.drop_missing_selection();
    }
}

/// The links (hub, neighbor) that replace `id`'s once it's gone. Per axis, when both sides
/// have neighbors, the hub is on the side with fewer (a tie goes to above, or left), the one
/// nearest `id` by center (ties by ID), and every neighbor on the other side links to it.
/// Each neighbor is in its own subtree once `id` is gone, so none of these closes a cycle.
fn reconnect(
    doc: &Document,
    id: &ShapeId,
    center: &HashMap<ShapeId, Point>,
) -> Vec<(ShapeId, ShapeId)> {
    let at = center[id];
    let mut sides: [Vec<&ShapeId>; 4] = Default::default();
    for Link(a, b) in &doc.links {
        let n = match () {
            _ if a == id => b,
            _ if b == id => a,
            _ => continue,
        };
        sides[side(at, center[n]) as usize].push(n);
    }
    let dist = |n: &ShapeId| {
        let c = center[n];
        (c.x - at.x).hypot(c.y - at.y)
    };
    let mut out = Vec::new();
    for (first, second) in [(Side::Top, Side::Bottom), (Side::Left, Side::Right)] {
        let (a, b) = (&sides[first as usize], &sides[second as usize]);
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let (hubs, others) = if b.len() < a.len() { (b, a) } else { (a, b) };
        let hub = (hubs.iter())
            .min_by(|x, y| {
                (dist(x).total_cmp(&dist(y)))
                    .then_with(|| x.generation_order().cmp(&y.generation_order()))
            })
            .expect("not empty");
        out.extend(others.iter().map(|&n| ((*hub).clone(), n.clone())));
    }
    out
}
