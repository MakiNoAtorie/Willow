//! Drawn rectangles: where the writer sees a shape (ADR 0006). Everything the writer sees or
//! points at reads these; `Shape::x` and `y` are the placed position, read only for `@layout`,
//! moves and what the spacing pass takes in.

use std::cell::Ref;
use std::collections::HashMap;

use super::push::Rect;
use super::{Session, spacing};
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

/// What the last spacing pass worked out.
#[derive(Default)]
pub(super) struct Spaced {
    /// The document revision it holds for; `None` before the first run.
    revision: Option<u64>,
    /// What it took in: a change that leaves these as they were needs no new pass.
    rects: Vec<Rect>,
    ends: Vec<(usize, usize)>,
    ids: Vec<ShapeId>,
    /// Every chain in pass order, which text never changes.
    order: Vec<Vec<usize>>,
    /// The shapes drawn off their placed position.
    offsets: HashMap<ShapeId, (i32, i32)>,
}

impl Session {
    /// The spacing pass for the document as it is: cached, and checked on every read against
    /// the revision, which every change bumps. A keystroke reads back only the shape it
    /// changed: walking every shape's layout costs a cold frame more than the pass does.
    pub(super) fn spaced(&self) -> Ref<'_, Spaced> {
        let (revision, last) = (self.history.revision(), self.spaced.borrow().revision);
        if last != Some(revision) {
            let typed = last.and_then(|r| self.history.text_changed_since(r));
            let spaced = &mut *self.spaced.borrow_mut();
            let stale = match typed {
                Some(typed) => self.retake_text(spaced, typed),
                None => self.retake_all(spaced),
            };
            if stale {
                let moved = spacing::pass(&spaced.order, &spaced.rects, self.theme.min_gap);
                spaced.offsets = (self.doc.shapes.iter().zip(moved))
                    .filter(|&(_, offset)| offset != (0, 0))
                    .map(|(s, offset)| (s.id.clone(), offset))
                    .collect();
            }
            spaced.revision = Some(revision);
        }
        self.spaced.borrow()
    }

    /// Updates what the pass takes in for shapes `typed`; true if any of it changed.
    fn retake_text(&self, spaced: &mut Spaced, typed: &[usize]) -> bool {
        let mut stale = false;
        for &i in typed {
            let rect = self.rect(&self.doc.shapes[i]);
            stale |= spaced.rects[i] != rect;
            spaced.rects[i] = rect;
        }
        stale
    }

    /// Updates all that the pass takes in; true if any of it changed.
    fn retake_all(&self, spaced: &mut Spaced) -> bool {
        let shapes = &self.doc.shapes;
        let rects: Vec<_> = shapes.iter().map(|s| self.rect(s)).collect();
        let ends = self.link_ends();
        let same = spaced.rects == rects
            && spaced.ends == *ends
            && spaced.ids.iter().eq(shapes.iter().map(|s| &s.id));
        if !same {
            spaced.order = spacing::order(shapes, &ends);
            spaced.rects = rects;
            spaced.ends.clone_from(&ends);
            spaced.ids = shapes.iter().map(|s| s.id.clone()).collect();
        }
        !same
    }

    /// Runs the spacing pass afresh, whatever the cache holds: what the bench times, and what
    /// a change to the gaps or to measuring needs.
    pub fn respace(&self) {
        *self.spaced.borrow_mut() = Spaced::default();
        self.spaced();
    }

    /// How far shape `id` is drawn off its placed position. The one place that decides it:
    /// everything else on this page is built on it.
    fn drawn_offset(&self, id: &ShapeId) -> (i32, i32) {
        #[cfg(test)]
        if let Some(&offset) = self.drawn_offsets.get(id) {
            return offset;
        }
        (self.spaced().offsets.get(id).copied()).unwrap_or_default()
    }

    /// Top-left of `s` as drawn. `s` may be a preview or copy of a document shape: its own
    /// position counts, moved by the offset of the shape with its ID. Costs no layout, for
    /// the loops that cull before measuring.
    pub(crate) fn drawn_at(&self, s: &Shape) -> (i32, i32) {
        let (dx, dy) = self.drawn_offset(&s.id);
        (s.x.saturating_add(dx), s.y.saturating_add(dy))
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
