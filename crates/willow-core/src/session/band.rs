//! The title band: the strip along a zone's top edge, the only part of a zone the mouse grabs.
//! Dragging it moves the zone with its members and nested zones.

use std::collections::HashSet;

use super::drawn::land;
use super::{Gesture, Session};
use crate::{Document, Point, ShapeId};

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

    /// Tracks the edge or band the pointer at screen point `p` hovers, first that applies: a
    /// shape's resize edge, a zone's edge or corner, a title band. None over a midpoint, and
    /// no edge while Space is held. Whether any of them changed.
    pub(super) fn track_edges(&mut self, p: Point) -> bool {
        let free = self.midpoint_at(p).is_none();
        let edge = (free && !self.space_held)
            .then(|| self.edge_at(p))
            .flatten();
        let edge = edge.map(|e| e.1);
        let zone_edge = (free && edge.is_none() && !self.space_held)
            .then(|| self.zone_edge_at(p))
            .flatten()
            .map(|e| e.1);
        let band = (free && edge.is_none() && zone_edge.is_none())
            .then(|| self.band_at(p))
            .flatten()
            .map(|i| self.doc.zones[i].id.clone());
        let changed =
            edge != self.over_edge || zone_edge != self.over_zone_edge || band != self.over_band;
        (self.over_edge, self.over_zone_edge, self.over_band) = (edge, zone_edge, band);
        changed
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
    pub(super) fn drop_zone_by(&mut self, index: usize, dx: i32, dy: i32) {
        let carried: HashSet<ShapeId> = self.carried(index).into_iter().cloned().collect();
        let id = self.doc.zones[index].id.clone();
        let nest = self.nest_change(index, dx, dy);
        let landing = self.landing(&carried, dx, dy);
        self.history.step(&mut self.doc, |h, doc| {
            land(h, doc, landing);
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
        let doc = &self.doc;
        let z = &doc.zones[index];
        if doc.zones.iter().any(|n| z.members.contains(&n.id)) {
            return None;
        }
        let c = Point {
            x: (z.x + dx) as f32 + z.width as f32 / 2.0,
            y: (z.y + dy) as f32 + z.height as f32 / 2.0,
        };
        let to = self.zone_at(c, |q| q.id != z.id && doc.parent_zone(&q.id).is_none());
        let to = to.map(|q| q.id.clone());
        (doc.parent_zone(&z.id) != to.as_ref()).then_some(to)
    }
}

/// The zones' draw order with zone `id` (and the zones it holds) raised above its siblings:
/// a top-level zone to the top, a nested one to just after its parent's other zones.
pub(super) fn raised(doc: &Document, id: &ShapeId) -> Vec<ShapeId> {
    let parent = |q: &ShapeId| doc.parent_zone(q);
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
