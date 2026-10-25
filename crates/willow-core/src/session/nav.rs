//! Arrow-key navigation: picking the next shape in a direction.

use std::collections::HashSet;
use std::time::Duration;

use super::Session;
use crate::{Document, Link, Point, ShapeId, Side, side};

/// The shape (index into `doc.shapes`) an arrow toward `dir` goes to from shape `from`; each
/// shape is `heights` tall. A shape lies toward `dir` by the link-side rule on centers. Linked
/// shapes that way win, else any shape that way; the nearest by center, ties by ID.
pub(crate) fn next_shape(doc: &Document, heights: &[f32], from: usize, dir: Side) -> Option<usize> {
    let shapes = &doc.shapes;
    let center = |i: usize| shapes[i].center(heights[i]);
    let at = center(from);
    let distance = |i: usize| {
        let c = center(i);
        (c.x - at.x).hypot(c.y - at.y)
    };
    let nearest = |them: &mut dyn Iterator<Item = usize>| {
        them.filter(|&i| i != from && side(at, center(i)) == dir)
            .min_by(|&i, &j| {
                let key = |i: usize| shapes[i].id.generation_order();
                (distance(i).total_cmp(&distance(j))).then_with(|| key(i).cmp(&key(j)))
            })
    };
    let mut linked = (doc.links.iter()).filter_map(|Link(a, b)| match &shapes[from].id {
        id if id == a => shapes.iter().position(|s| &s.id == b),
        id if id == b => shapes.iter().position(|s| &s.id == a),
        _ => None,
    });
    nearest(&mut linked).or_else(|| nearest(&mut (0..shapes.len())))
}

/// The view easing by `delta` canvas units from `start`; `done` is the part already applied.
#[derive(Clone, Copy, Debug)]
pub(super) struct Follow {
    delta: Point,
    start: Duration,
    done: f32,
}

/// How far a view spanning `view` (start, length) along one axis must move to show `lo..hi`
/// with `margin`; 0 when it shows it whole already. Too long to fit, its start shows.
fn shift(lo: f32, hi: f32, view: (f32, f32), margin: f32) -> f32 {
    let (start, len) = view;
    if lo >= start && hi <= start + len {
        0.0
    } else if lo < start || hi - lo > len - 2.0 * margin {
        lo - margin - start
    } else {
        hi + margin - (start + len)
    }
}

impl Session {
    /// When shape `id` isn't wholly on screen, eases the view just far enough to show it with
    /// `Theme.follow_margin`, over `Theme.follow`; never zooms. What the view does when the
    /// keyboard selects or creates a shape. A running follow retargets.
    pub(crate) fn reveal(&mut self, id: &ShapeId) {
        self.advance_follow(self.now);
        let Some(i) = self.index_of(id) else {
            return;
        };
        let s = &self.doc.shapes[i];
        let (h, vp) = (self.shape_height(s), self.viewport);
        let rest = self.follow.map_or(Point::default(), |f| Point {
            x: f.delta.x * (1.0 - f.done),
            y: f.delta.y * (1.0 - f.done),
        });
        let (z, m) = (vp.zoom, self.theme.follow_margin / vp.zoom);
        let (x, y) = (s.x as f32, s.y as f32);
        let dx = shift(
            x,
            x + s.width as f32,
            (vp.origin.x + rest.x, vp.size.x / z),
            m,
        );
        let dy = shift(y, y + h, (vp.origin.y + rest.y, vp.size.y / z), m);
        let delta = Point {
            x: rest.x + dx,
            y: rest.y + dy,
        };
        self.follow = (delta != Point::default()).then_some(Follow {
            delta,
            start: self.now,
            done: 0.0,
        });
    }

    /// Advances a running follow to `now`. True if one was running.
    pub(super) fn advance_follow(&mut self, now: Duration) -> bool {
        let Some(f) = &mut self.follow else {
            return false;
        };
        let p = self.theme.follow.progress(now.saturating_sub(f.start));
        self.viewport.origin.x += f.delta.x * (p - f.done);
        self.viewport.origin.y += f.delta.y * (p - f.done);
        f.done = p;
        if p >= 1.0 {
            self.follow = None;
        }
        true
    }

    /// A follow is still easing at `now`.
    pub(super) fn following(&self, now: Duration) -> bool {
        (self.follow).is_some_and(|f| now < f.start + self.theme.follow.duration)
    }

    /// Moves the selection one shape toward `dir` (an arrow key); with nothing selected it
    /// starts a keyboard pan. A multi-selection first collapses to its first shape in story
    /// order; with nothing that way, nothing else happens. Not an undo step. True if the
    /// selection changed or a pan started.
    pub(crate) fn navigate(&mut self, dir: Side) -> bool {
        let old = self.selected.clone();
        let heights = self.heights();
        let Some(from) = self.first_in_story_order(&heights) else {
            self.key_pan_down(dir);
            return true;
        };
        let Some(to) = next_shape(&self.doc, &heights, from, dir) else {
            // Nothing that way: a multi-selection still collapses, the view stays.
            self.select([self.doc.shapes[from].id.clone()]);
            return self.selected != old;
        };
        let id = self.doc.shapes[to].id.clone();
        self.reveal(&id);
        self.select([id]);
        self.selected != old
    }

    /// The selected shape (index into `doc.shapes`) that comes first in story order.
    pub(crate) fn first_in_story_order(&self, heights: &[f32]) -> Option<usize> {
        let selected: HashSet<&ShapeId> = self.selected.iter().collect();
        (crate::format::story_order(&self.doc, heights).into_iter())
            .find(|&i| selected.contains(&self.doc.shapes[i].id))
    }
}
