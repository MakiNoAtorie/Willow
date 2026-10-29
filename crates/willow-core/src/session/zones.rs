//! Zone commands: Create zone from the selection.

use super::Session;
use crate::{Color, ShapeId, Zone};

impl Session {
    /// Whether Create zone is available: a selection whose shapes all have the same parent, none
    /// or one zone that is not itself nested.
    pub(super) fn can_create_zone(&self) -> bool {
        self.zone_parent().is_some()
    }

    /// The parent a zone around the selection would join: `Some(None)` for the top level,
    /// `Some(Some(zone))` for a nested zone, `None` when it can't be made.
    fn zone_parent(&self) -> Option<Option<ShapeId>> {
        let parent_of = |id: &ShapeId| {
            (self.doc.zones.iter())
                .find(|z| z.members.contains(id))
                .map(|z| &z.id)
        };
        let parent = parent_of(self.selected.first()?);
        if self.selected.iter().any(|id| parent_of(id) != parent) {
            return None;
        }
        // A zone inside a nested zone would be a third level.
        if parent.is_some_and(|p| parent_of(p).is_some()) {
            return None;
        }
        Some(parent.cloned())
    }

    /// Wraps the selected shapes in a new neutral zone, padded like the loader's, joining their
    /// shared parent zone if any. One undo step; does nothing when `can_create_zone` is false.
    pub(super) fn create_zone(&mut self) {
        let Some(parent) = self.zone_parent() else {
            return;
        };
        self.stop_editing();
        let pad = self.theme.zone.padding;
        let members: Vec<_> = (self.doc.shapes.iter())
            .filter(|s| self.selected.contains(&s.id))
            .collect();
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for s in &members {
            let (x, y) = (s.x as f32, s.y as f32);
            let (r, b) = (x + s.width as f32, y + self.shape_height(s));
            (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(r), hi.1.max(b)));
        }
        // The loader's rounding.
        let (x, y) = ((lo.0 - pad).floor(), (lo.1 - pad).floor());
        let id = self.doc.next_ids(1).remove(0);
        let zone = Zone {
            id: id.clone(),
            title: String::new(),
            color: Color::Neutral,
            x: x as i32,
            y: y as i32,
            width: ((hi.0 + pad).ceil() - x) as u32,
            height: ((hi.1 + pad).ceil() - y) as u32,
            members: members.iter().map(|s| s.id.clone()).collect(),
        };
        self.history.step(&mut self.doc, |h, doc| {
            h.add_zone(doc, zone);
            if let Some(p) = &parent {
                h.join_zone(doc, p, &id);
            }
        });
    }
}
