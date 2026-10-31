//! New shape beside the chain (Ctrl/Cmd+Shift+N).

use super::Session;
use crate::{Point, Shape, ShapeId};

impl Session {
    /// An unlinked neutral shape in editing, one creation gap right of the chain of the first
    /// selected shape in story order, top-aligned with that shape; with nothing selected,
    /// centered in the view. Joins a zone like midpoint creation; the view follows.
    pub(super) fn new_beside(&mut self) {
        let heights = self.heights();
        let probe = Shape::new(ShapeId(String::new()), 0, 0);
        let (w, h) = (probe.width as f32, self.shape_height(&probe));
        let center = match self.first_in_story_order(&heights) {
            Some(i) => {
                let from = &self.doc.shapes[i];
                let right = (self.doc.chain(&from.id).into_iter())
                    .filter_map(|c| self.doc.shapes.iter().find(|s| &s.id == c))
                    .map(|s| self.drawn_at(s).0 + s.width as i32)
                    .max()
                    .expect("the chain holds its start");
                Point {
                    x: right as f32 + self.theme.creation_gap + w / 2.0,
                    y: self.drawn_at(from).1 as f32 + h / 2.0,
                }
            }
            None => self.viewport.center(),
        };
        self.new_shape_at(center);
        if let Some(id) = self.selected.first().cloned() {
            self.reveal(&id);
        }
    }
}
