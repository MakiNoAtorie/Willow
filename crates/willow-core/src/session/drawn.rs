//! Drawn rectangles: where the writer sees a shape (ADR 0006). Everything the writer sees or
//! points at reads these; `Shape::x` and `y` are the placed position, read only for `@layout`,
//! moves and what the spacing pass takes in.

use std::cell::Ref;
use std::collections::HashMap;
use std::time::Duration;

use super::Session;
use super::spacing::{self, Rect};
use crate::{Point, Shape, ShapeId, Theme};

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

/// A pushed shape's slide: drawn `from` canvas units off its rest position at `since`, easing
/// to 0.
struct Slide {
    from: Point,
    since: Duration,
}

impl Slide {
    /// How far off its rest position the shape is drawn at `now`.
    fn left(&self, theme: &Theme, now: Duration) -> Point {
        let k = 1.0 - theme.push.progress(now.saturating_sub(self.since));
        Point {
            x: self.from.x * k,
            y: self.from.y * k,
        }
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
    order: Vec<Vec<spacing::Step>>,
    /// How each chain settled inside itself, by shape index; a keystroke redoes only its chain.
    settled: Vec<(i32, i32)>,
    /// Where each shape of `ids` is in them, for reads by ID.
    index: HashMap<ShapeId, usize, spacing::Cheap>,
    /// How far each shape is drawn off its placed position, by shape index.
    offsets: Vec<(i32, i32)>,
    /// Slides by shape: a pass starts them, `tick` drops finished ones.
    slides: HashMap<ShapeId, Slide>,
}

impl Spaced {
    /// Slides `id` from `by` units off where it was going, picking up from where it is at `now`.
    pub(super) fn slide(&mut self, id: &ShapeId, by: (i32, i32), theme: &Theme, now: Duration) {
        let left = self.slides.get(id).map(|s| s.left(theme, now));
        let left = left.unwrap_or_default();
        let from = Point {
            x: left.x + by.0 as f32,
            y: left.y + by.1 as f32,
        };
        self.slides.insert(id.clone(), Slide { from, since: now });
    }

    /// Slides every shape the last pass moved off its offset in `old`.
    fn slide_from(&mut self, old: &[(i32, i32)], theme: &Theme, now: Duration) {
        for (i, &(x, y)) in old.iter().enumerate() {
            let (nx, ny) = self.offsets[i];
            if (x, y) != (nx, ny) {
                let id = self.ids[i].clone();
                let by = (x.saturating_sub(nx), y.saturating_sub(ny));
                self.slide(&id, by, theme, now);
            }
        }
    }
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
                let gap = self.theme.min_gap;
                let moved = spacing::pass(&spaced.order, &spaced.rects, &spaced.settled, gap);
                let old = std::mem::replace(&mut spaced.offsets, moved);
                // A first pass has nothing to slide from.
                if last.is_some() {
                    spaced.slide_from(&old, self.theme, self.now);
                }
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
            if spaced.rects[i] != rect {
                spaced.rects[i] = rect;
                stale = true;
                let chain = spaced.order.iter().find(|c| c.iter().any(|s| s.shape == i));
                let chain = chain.expect("every shape is in a chain");
                spacing::settle(chain, &spaced.rects, self.gaps(), &mut spaced.settled);
            }
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
            if !spaced.ids.iter().eq(shapes.iter().map(|s| &s.id)) {
                // The last offsets by the new indices: slides compare the two passes.
                let offset = |s: &Shape| spaced.index.get(&s.id).map(|&i| spaced.offsets[i]);
                let offsets = shapes.iter().map(|s| offset(s).unwrap_or_default());
                spaced.offsets = offsets.collect();
                spaced.ids = shapes.iter().map(|s| s.id.clone()).collect();
                let at = spaced.ids.iter().cloned().enumerate();
                spaced.index = at.map(|(i, id)| (id, i)).collect();
            }
            spaced.settled = vec![(0, 0); shapes.len()];
            for chain in &spaced.order {
                spacing::settle(chain, &spaced.rects, self.gaps(), &mut spaced.settled);
            }
        }
        !same
    }

    fn gaps(&self) -> spacing::Gaps {
        spacing::Gaps {
            creation: self.theme.creation_gap,
            min: self.theme.min_gap,
        }
    }

    /// Runs the spacing pass afresh, whatever the cache holds, with nothing sliding: what the
    /// bench times, and what a reload or a change to the gaps or to measuring needs.
    pub fn respace(&self) {
        *self.spaced.borrow_mut() = Spaced::default();
        self.spaced();
    }

    /// A shape's placed rectangle.
    pub(super) fn rect(&self, s: &Shape) -> Rect {
        let (x, y) = (s.x as f32, s.y as f32);
        Rect {
            l: x,
            t: y,
            r: x + s.width as f32,
            b: y + self.shape_height(s),
        }
    }

    /// How far `id` is drawn off its rest position at `now` while it slides; (0, 0) otherwise.
    pub(crate) fn sliding(&self, id: &ShapeId, now: Duration) -> Point {
        let spaced = self.spaced();
        let slide = spaced.slides.get(id);
        slide.map_or(Point::default(), |s| s.left(self.theme, now))
    }

    /// Whether any slide still runs at `now`.
    pub(super) fn slides_running(&self, now: Duration) -> bool {
        let d = self.theme.push.duration;
        (self.spaced().slides.values()).any(|s| now < s.since + d)
    }

    /// Forgets slides finished by `now`, so that drawing looks up none of them.
    pub(super) fn drop_finished_slides(&mut self, now: Duration) {
        let d = self.theme.push.duration;
        let slides = &mut self.spaced.get_mut().slides;
        slides.retain(|_, s| now < s.since + d);
    }

    /// How far shape `id` is drawn off its placed position. The one place that decides it:
    /// everything else on this page is built on it.
    fn drawn_offset(&self, id: &ShapeId) -> (i32, i32) {
        #[cfg(test)]
        if let Some(&offset) = self.drawn_offsets.get(id) {
            return offset;
        }
        let spaced = self.spaced();
        let i = spaced.index.get(id);
        i.map_or((0, 0), |&i| spaced.offsets[i])
    }

    /// `drawn_offset` of the shape at index `i`, for the loops over every shape: no ID lookup.
    pub(crate) fn drawn_offset_at(&self, i: usize) -> (i32, i32) {
        #[cfg(test)]
        if let Some(&offset) = self.drawn_offsets.get(&self.doc.shapes[i].id) {
            return offset;
        }
        self.spaced().offsets.get(i).copied().unwrap_or_default()
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
