//! Drawn rectangles: where the writer sees a shape (ADR 0006). Everything the writer sees or
//! points at reads these; `Shape::x` and `y` are the placed position, read only for `@layout`,
//! moves and what the spacing pass takes in.

use std::cell::Ref;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::Session;
use super::spacing::{self, Rect};
use crate::history::History;
use crate::{Document, Point, Shape, ShapeId, Theme, format};

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
    /// Every shape's height as of `revision`: walking the layout cache is half of what an
    /// autosave takes from the UI thread.
    heights: Vec<f32>,
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
    /// Slides `id`, whose placed position just moved by (dx, dy), from where it was drawn.
    pub(super) fn slide_moved(
        &mut self,
        id: &ShapeId,
        (dx, dy): (i32, i32),
        theme: &Theme,
        now: Duration,
    ) {
        let (x, y) = (self.index.get(id)).map_or((0, 0), |&i| self.offsets[i]);
        let by = (x.saturating_sub(dx), y.saturating_sub(dy));
        self.slide(id, by, theme, now);
    }

    /// Slides `id` from `by` units off where it was going, picking up from where it is at `now`.
    fn slide(&mut self, id: &ShapeId, by: (i32, i32), theme: &Theme, now: Duration) {
        let left = self.slides.get(id).map(|s| s.left(theme, now));
        let left = left.unwrap_or_default();
        let from = Point {
            x: left.x + by.0 as f32,
            y: left.y + by.1 as f32,
        };
        self.slides.insert(id.clone(), Slide { from, since: now });
    }

    /// Slides every shape the last pass moved off its offset in `old`. A shape whose placed
    /// position changed by `moved` (by index; empty for none) starts from there, so a drop
    /// slides only if pushed off where it was released, or stays put if drawn where it was.
    fn slide_from(
        &mut self,
        old: &[(i32, i32)],
        moved: &[(i32, i32)],
        theme: &Theme,
        now: Duration,
    ) {
        for (i, &(x, y)) in old.iter().enumerate() {
            let (nx, ny) = self.offsets[i];
            let (mx, my) = moved.get(i).copied().unwrap_or_default();
            let drawn_there = (x.saturating_sub(mx), y.saturating_sub(my)) == (nx, ny);
            let (x, y) = match (mx, my) {
                (0, 0) => (x, y),
                _ if drawn_there => (nx, ny),
                _ => (0, 0),
            };
            if (x, y) != (nx, ny) {
                let id = self.ids[i].clone();
                let by = (x.saturating_sub(nx), y.saturating_sub(ny));
                self.slide(&id, by, theme, now);
            }
        }
    }
}

/// Writes what `Session::landing` worked out, for the move's undo step.
pub(super) fn land(h: &mut History, doc: &mut Document, landing: Vec<(usize, (i32, i32))>) {
    for (i, at) in landing {
        h.change_at(doc, i, |s| (s.x, s.y) = at);
    }
}

impl Session {
    /// Where a move of shapes `ids` by (dx, dy) leaves what it touches, by shape index: each
    /// chain holding one takes its drawn positions as placed, then the movement is added, so
    /// a drop lands exactly and that chain stops settling back. No movement touches nothing.
    pub(super) fn landing<'a>(
        &self,
        ids: impl IntoIterator<Item = &'a ShapeId>,
        dx: i32,
        dy: i32,
    ) -> Vec<(usize, (i32, i32))> {
        if (dx, dy) == (0, 0) {
            return Vec::new();
        }
        let ids: HashSet<&ShapeId> = ids.into_iter().collect();
        let shapes = &self.doc.shapes;
        let spaced = self.spaced();
        let touched = (spaced.order.iter())
            .filter(|chain| chain.iter().any(|s| ids.contains(&shapes[s.shape].id)))
            .flatten();
        touched
            .map(|step| {
                let (i, s) = (step.shape, &shapes[step.shape]);
                let (ox, oy) = self.drawn_offset_at(i);
                let (mx, my) = if ids.contains(&s.id) {
                    (dx, dy)
                } else {
                    (0, 0)
                };
                let at = |p: i32, o: i32, m: i32| p.saturating_add(o).saturating_add(m);
                (i, (at(s.x, ox, mx), at(s.y, oy, my)))
            })
            .collect()
    }

    /// The spacing pass for the document as it is: cached, and checked on every read against
    /// the revision, which every change bumps. A keystroke reads back only the shape it
    /// changed: walking every shape's layout costs a cold frame more than the pass does.
    pub(super) fn spaced(&self) -> Ref<'_, Spaced> {
        let (revision, last) = (self.history.revision(), self.spaced.borrow().revision);
        if last != Some(revision) {
            let typed = last.and_then(|r| self.history.text_changed_since(r));
            let spaced = &mut *self.spaced.borrow_mut();
            let stale = match typed {
                Some(typed) => self.retake_text(spaced, typed).then(Vec::new),
                None => self.retake_all(spaced),
            };
            if let Some(moved) = stale {
                let gap = self.theme.min_gap;
                let new = spacing::pass(&spaced.order, &spaced.rects, &spaced.settled, gap);
                let old = std::mem::replace(&mut spaced.offsets, new);
                // A first pass has nothing to slide from.
                if last.is_some() {
                    spaced.slide_from(&old, &moved, self.theme, self.now);
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
            let s = &self.doc.shapes[i];
            spaced.heights[i] = self.shape_height(s);
            let rect = Rect::of(s, spaced.heights[i]);
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

    /// Updates all that the pass takes in; if any of it changed, how far each shape's placed
    /// position did, by shape index.
    fn retake_all(&self, spaced: &mut Spaced) -> Option<Vec<(i32, i32)>> {
        let shapes = &self.doc.shapes;
        spaced.heights = shapes.iter().map(|s| self.shape_height(s)).collect();
        let at = shapes.iter().zip(&spaced.heights);
        let rects: Vec<_> = at.map(|(s, &h)| Rect::of(s, h)).collect();
        let ends = self.link_ends();
        let same = spaced.rects == rects
            && spaced.ends == *ends
            && spaced.ids.iter().eq(shapes.iter().map(|s| &s.id));
        if same {
            return None;
        }
        let was = |s: &Shape| spaced.index.get(&s.id).map(|&i| spaced.rects[i]);
        let moved = (shapes.iter().zip(&rects))
            .map(|(s, r)| was(s).map_or((0, 0), |o| ((r.l - o.l) as i32, (r.t - o.t) as i32)))
            .collect();
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
        Some(moved)
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

    /// Every shape's height as of the last history step, in document order: marked text sits
    /// outside the history, so a shape being composed in may differ.
    pub(super) fn spaced_heights(&self) -> Vec<f32> {
        self.spaced().heights.clone()
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

    /// Every shape's drawn top-left, in document order: what `save` reads with `heights`.
    pub fn drawn_positions(&self) -> Vec<(i32, i32)> {
        let at = |(i, s): (usize, &Shape)| {
            let (dx, dy) = self.drawn_offset_at(i);
            (s.x.saturating_add(dx), s.y.saturating_add(dy))
        };
        self.doc.shapes.iter().enumerate().map(at).collect()
    }

    /// Every shape (indices into the document) in story order within its chain, as drawn.
    pub(super) fn story_order(&self) -> Vec<usize> {
        format::story_order(&self.doc, &self.heights(), &self.drawn_positions())
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
