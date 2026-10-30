//! Drawn rectangles: where the writer sees a shape (ADR 0006). Everything the writer sees or
//! points at reads these; `Shape::x` and `y` are the placed position, read only for `@layout`,
//! moves and what the spacing pass takes in.

use super::Session;
use crate::{Point, Shape, ShapeId};

/// Where a shape is drawn at rest, in canvas units: slides and gesture previews are on top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnRect {
    /// Top-left corner.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DrawnRect {
    pub fn center(self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }
}

impl Session {
    /// How far shape `id` is drawn off its placed position. The one place that decides it:
    /// everything else on this page is built on it.
    fn drawn_offset(&self, id: &ShapeId) -> (i32, i32) {
        #[cfg(test)]
        if let Some(&offset) = self.drawn_offsets.get(id) {
            return offset;
        }
        // Drawn equals placed until the spacing pass arrives.
        let _ = id;
        (0, 0)
    }

    /// Top-left of `s` as drawn. `s` may be a preview or copy of a document shape: its own
    /// position counts, moved by the offset of the shape with its ID. Costs no layout, for
    /// the loops that cull before measuring.
    pub(crate) fn drawn_at(&self, s: &Shape) -> (i32, i32) {
        let (dx, dy) = self.drawn_offset(&s.id);
        (s.x + dx, s.y + dy)
    }

    /// The drawn rectangle of `s`, as `drawn_at` takes it.
    pub(crate) fn drawn(&self, s: &Shape) -> DrawnRect {
        let (x, y) = self.drawn_at(s);
        DrawnRect {
            x: x as f32,
            y: y as f32,
            width: s.width as f32,
            height: self.shape_height(s),
        }
    }

    /// The drawn rectangle of shape `id`.
    pub fn drawn_rect(&self, id: &ShapeId) -> Option<DrawnRect> {
        let s = self.doc.shapes.iter().find(|s| &s.id == id)?;
        Some(self.drawn(s))
    }
}
