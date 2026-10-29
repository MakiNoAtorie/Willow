//! The title band: the strip along a zone's top edge, the only part of a zone the mouse grabs.
//! Dragging it moves the zone with its members and nested zones.

use std::collections::HashSet;

use super::{Gesture, Session};
use crate::{Document, Point, ShapeId, Zone};

impl Session {
    /// Index of the zone whose title band is under screen point `p`: the band of the zone
    /// drawn on top there, unless a shape covers it. The one zone hit-test.
    pub fn band_at(&self, p: Point) -> Option<usize> {
        if self.shape_at(p).is_some() {
            return None;
        }
        let c = self.viewport.to_canvas(p);
        let id = self.innermost_zone(c)?;
        let i = self.doc.zones.iter().position(|z| z.id == id)?;
        // Constant on screen zoomed out, scaling with the title zoomed in.
        let k = 1.0 / self.viewport.zoom.min(1.0);
        (c.y <= self.doc.zones[i].y as f32 + self.theme.zone.band * k).then_some(i)
    }

    /// The zone whose ring shows for the band: the hovered band while no gesture runs, or
    /// while a zone is dragged, the zone a drop now would nest it in, if that changes.
    pub(crate) fn band_ring(&self) -> Option<&ShapeId> {
        let id = match self.gesture {
            None => self.over_band.clone()?,
            Some(Gesture::Band {
                index,
                to: Some((x, y)),
                ..
            }) => {
                let z = &self.doc.zones[index];
                self.nest_change(index, x - z.x, y - z.y)??
            }
            Some(_) => return None,
        };
        (self.doc.zones.iter()).find(|z| z.id == id).map(|z| &z.id)
    }

    /// While a zone is dragged by its band: how far, in canvas units, and the IDs of every
    /// zone and shape it carries. The document still holds the old positions.
    pub(crate) fn zone_drag(&self) -> Option<((i32, i32), HashSet<&ShapeId>)> {
        let Some(Gesture::Band {
            index,
            to: Some((x, y)),
            ..
        }) = self.gesture
        else {
            return None;
        };
        let z = &self.doc.zones[index];
        Some(((x - z.x, y - z.y), self.carried(index)))
    }

    /// Zone `index`, its members and the members of the zones nested in it.
    fn carried(&self, index: usize) -> HashSet<&ShapeId> {
        let z = &self.doc.zones[index];
        let nested = (self.doc.zones.iter()).filter(|n| z.members.contains(&n.id));
        (std::iter::once(&z.id).chain(&z.members))
            .chain(nested.flat_map(|n| &n.members))
            .collect()
    }

    /// Moves zone `index` by (dx, dy) with everything it carries, as one undo step: it nests
    /// in the top-level zone under its new center, or none, and draws above its siblings.
    /// Pushes nothing.
    pub(super) fn drop_zone_by(&mut self, index: usize, dx: i32, dy: i32) {
        let carried: HashSet<ShapeId> = self.carried(index).into_iter().cloned().collect();
        let id = self.doc.zones[index].id.clone();
        let nest = self.nest_change(index, dx, dy);
        self.history.step(&mut self.doc, |h, doc| {
            for i in 0..doc.shapes.len() {
                if carried.contains(&doc.shapes[i].id) {
                    h.change_at(doc, i, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
                }
            }
            for z in &carried {
                h.change_zone(doc, z, |z| (z.x, z.y) = (z.x + dx, z.y + dy));
            }
            if let Some(to) = nest {
                h.leave_zone(doc, &id);
                if let Some(p) = to {
                    h.join_zone(doc, &p, &id);
                }
            }
            let order = raised(doc, &id);
            h.order_zones(doc, &order);
        });
    }

    /// The parent zone `index` would get moved by (dx, dy), if that changes it: the
    /// top-level zone drawn on top under its new center, or none. A zone holding nested
    /// zones never nests.
    fn nest_change(&self, index: usize, dx: i32, dy: i32) -> Option<Option<ShapeId>> {
        let zones = &self.doc.zones;
        let z = &zones[index];
        if zones.iter().any(|n| z.members.contains(&n.id)) {
            return None;
        }
        let c = Point {
            x: (z.x + dx) as f32 + z.width as f32 / 2.0,
            y: (z.y + dy) as f32 + z.height as f32 / 2.0,
        };
        let top_level = |q: &Zone| !zones.iter().any(|p| p.members.contains(&q.id));
        let to = (zones.iter())
            .rfind(|q| q.id != z.id && top_level(q) && contains(q, c))
            .map(|q| q.id.clone());
        let from = zones.iter().find(|p| p.members.contains(&z.id));
        (from.map(|p| &p.id) != to.as_ref()).then_some(to)
    }
}

fn contains(z: &Zone, c: Point) -> bool {
    let (x, y) = (z.x as f32, z.y as f32);
    (x..=x + z.width as f32).contains(&c.x) && (y..=y + z.height as f32).contains(&c.y)
}

/// The zones' draw order with zone `id` (and the zones it holds) raised above its siblings:
/// a top-level zone to the top, a nested one to just after its parent's other zones.
fn raised(doc: &Document, id: &ShapeId) -> Vec<ShapeId> {
    let parent = |q: &ShapeId| {
        (doc.zones.iter())
            .find(|p| p.members.contains(q))
            .map(|p| &p.id)
    };
    let ids = doc.zones.iter().map(|z| &z.id);
    let group: Vec<&ShapeId> = (ids.clone())
        .filter(|q| *q == id || parent(q) == Some(id))
        .collect();
    let mut order: Vec<&ShapeId> = ids.filter(|q| !group.contains(q)).collect();
    let at = match parent(id) {
        None => order.len(),
        Some(p) => (order.iter())
            .rposition(|q| *q == p || parent(q) == Some(p))
            .map_or(order.len(), |i| i + 1),
    };
    order.splice(at..at, group);
    order.into_iter().cloned().collect()
}
