//! Pushes: moving a block of shapes out of the way as a saved move (ADR 0004), recorded in the
//! open undo step, with a slide to the new place.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::Session;
use crate::input::Point;
use crate::{Link, Shape, ShapeId, Side};

/// A pushed shape's slide: drawn `from` canvas units off its saved place at `since`, easing to 0.
#[derive(Debug)]
pub(super) struct Slide {
    from: Point,
    since: Duration,
}

impl Session {
    /// `root` and every shape linked beyond it, not through `from`: the subtree a push moves
    /// as one block.
    pub(super) fn subtree(&self, root: &ShapeId, from: &ShapeId) -> Vec<ShapeId> {
        let (mut out, mut seen) = (vec![root.clone()], HashSet::from([root, from]));
        let mut i = 0;
        // ponytail: scans every link per shape; an adjacency map if subtrees get large.
        while let Some(s) = out.get(i).cloned() {
            for Link(a, b) in &self.doc.links {
                let n = match (a == &s, b == &s) {
                    (true, _) => b,
                    (_, true) => a,
                    _ => continue,
                };
                if seen.insert(n) {
                    out.push(n.clone());
                }
            }
            i += 1;
        }
        out
    }

    /// The shapes linked to `id` on its `side` (by the side rule), nearest center first; ties
    /// go in generation order.
    pub(super) fn linked_on(&self, id: &ShapeId, side: Side) -> Vec<ShapeId> {
        let find = |id: &ShapeId| self.doc.shapes.iter().find(|s| &s.id == id);
        let center = |s: &Shape| Point {
            x: s.x as f32 + s.width as f32 / 2.0,
            y: s.y as f32 + self.shape_height(s) / 2.0,
        };
        let Some(c) = find(id).map(center) else {
            return Vec::new();
        };
        let mut on: Vec<(f32, &ShapeId)> = (self.doc.links.iter())
            .filter_map(|Link(a, b)| match (a == id, b == id) {
                (true, _) => find(b),
                (_, true) => find(a),
                _ => None,
            })
            .map(|s| (center(s), &s.id))
            .filter(|&(p, _)| crate::side(c, p) == side)
            .map(|(p, n)| ((p.x - c.x).hypot(p.y - c.y), n))
            .collect();
        on.sort_by(|(d, a), (e, b)| {
            d.total_cmp(e)
                .then(a.0.len().cmp(&b.0.len()).then(a.0.cmp(&b.0)))
        });
        on.into_iter().map(|(_, n)| n.clone()).collect()
    }

    /// How far `far` must move toward `side` (as seen from `near`) to sit at least `gap` beyond
    /// `near`'s `side` edge, in whole units; (0, 0) if it already does.
    pub(super) fn clearance(&self, near: &Shape, far: &Shape, side: Side, gap: f32) -> (i32, i32) {
        let (nh, fh) = (self.shape_height(near), self.shape_height(far));
        let (nx, ny, fx, fy) = (near.x as f32, near.y as f32, far.x as f32, far.y as f32);
        let need = match side {
            Side::Bottom => ny + nh + gap - fy,
            Side::Top => fy + fh + gap - ny,
            Side::Right => nx + near.width as f32 + gap - fx,
            Side::Left => fx + far.width as f32 + gap - nx,
        };
        let d = need.max(0.0).ceil() as i32;
        match side {
            Side::Bottom => (0, d),
            Side::Top => (0, -d),
            Side::Right => (d, 0),
            Side::Left => (-d, 0),
        }
    }

    /// Moves `ids` by (dx, dy) as one block: a saved move in the open undo step (or its own),
    /// sliding there from where they were drawn.
    pub(super) fn push(&mut self, ids: &[ShapeId], dx: i32, dy: i32) {
        if (dx, dy) == (0, 0) {
            return;
        }
        let now = self.now;
        // ponytail: `change` finds each shape by a linear scan; index them if blocks get large.
        self.history.step(&mut self.doc, |h, doc| {
            for id in ids {
                h.change(doc, id, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
            }
        });
        for id in ids {
            let left = self.sliding(id, now);
            let from = Point {
                x: left.x - dx as f32,
                y: left.y - dy as f32,
            };
            self.slides.insert(id.clone(), Slide { from, since: now });
        }
    }

    /// How far `id` is drawn off its saved place at `now` while it slides; (0, 0) otherwise.
    pub(crate) fn sliding(&self, id: &ShapeId, now: Duration) -> Point {
        let Some(s) = self.slides.get(id) else {
            return Point::default();
        };
        let k = 1.0 - self.theme.push.progress(now.saturating_sub(s.since));
        Point {
            x: s.from.x * k,
            y: s.from.y * k,
        }
    }

    /// Whether any slide still runs at `now`.
    pub(super) fn slides_running(&self, now: Duration) -> bool {
        let d = self.theme.push.duration;
        self.slides.values().any(|s| now < s.since + d)
    }
}

/// Slides by shape; finished ones are harmless (offset 0) and replaced by the next push.
pub(super) type Slides = HashMap<ShapeId, Slide>;
