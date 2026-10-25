//! Multi-selection: the Shift+drag marquee, Shift+click toggling and moving the whole
//! selection as one undo step.

use super::{Gesture, Session};
use crate::{Point, ShapeId};

impl Session {
    /// The marquee being drawn: its top-left and bottom-right in canvas units.
    pub fn marquee(&self) -> Option<(Point, Point)> {
        let Some(Gesture::Marquee { from, to }) = self.gesture else {
            return None;
        };
        Some((
            Point {
                x: from.x.min(to.x),
                y: from.y.min(to.y),
            },
            Point {
                x: from.x.max(to.x),
                y: from.y.max(to.y),
            },
        ))
    }

    /// While a selected shape of a multi-selection is dragged: how far the whole selection
    /// moves, in canvas units. The document still holds the old positions.
    pub fn group_drag(&self) -> Option<(i32, i32)> {
        let Some(Gesture::Press {
            index,
            to: Some((x, y)),
            ..
        }) = self.gesture
        else {
            return None;
        };
        let s = &self.doc.shapes[index];
        (self.selected.len() > 1 && self.selected.contains(&s.id)).then(|| (x - s.x, y - s.y))
    }

    /// Moves every selected shape by (dx, dy) as one undo step. Pushes nothing.
    pub fn move_selection(&mut self, dx: i32, dy: i32) {
        let ids = self.selected.clone();
        self.history.step(&mut self.doc, |h, doc| {
            for id in &ids {
                h.change(doc, id, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
            }
        });
    }

    /// Shift+click: adds `id` to the selection, or takes it out.
    pub(super) fn toggle_selected(&mut self, id: ShapeId) {
        let mut ids = self.selected.clone();
        match ids.iter().position(|s| *s == id) {
            Some(i) => _ = ids.remove(i),
            None => ids.push(id),
        }
        self.select(ids);
    }

    /// Starts a marquee at screen point `pos`; the selection so far stays in it.
    pub(super) fn start_marquee(&mut self, pos: Point) {
        let p = self.viewport.to_canvas(pos);
        self.marquee_base = self.selected.clone();
        self.gesture = Some(Gesture::Marquee { from: p, to: p });
    }

    /// Stretches the marquee to screen point `pos`: the selection becomes what it was before
    /// the marquee plus every shape the marquee touches, in document order.
    pub(super) fn marquee_to(&mut self, pos: Point) {
        let Some(Gesture::Marquee { to, .. }) = &mut self.gesture else {
            return;
        };
        *to = self.viewport.to_canvas(pos);
        let (a, b) = self.marquee().expect("marquee");
        // ponytail: scans every shape per move; a spatial index if 10k-shape marquees lag.
        let touched: Vec<ShapeId> = (self.doc.shapes.iter())
            .filter(|s| {
                let (x, y) = (s.x as f32, s.y as f32);
                x <= b.x && x + s.width as f32 >= a.x && y <= b.y && y + self.shape_height(s) >= a.y
            })
            .map(|s| s.id.clone())
            .collect();
        let ids: Vec<ShapeId> = self.marquee_base.iter().cloned().chain(touched).collect();
        self.select(ids);
    }
}

impl Session {
    /// Selects every shape in the chains of the selected shapes; nothing selected, nothing happens.
    pub(super) fn select_chain(&mut self) {
        let all: Vec<ShapeId> = (self.selected.iter())
            .flat_map(|id| self.doc.chain(id))
            .cloned()
            .collect();
        if !all.is_empty() {
            self.select(all);
        }
    }
}
