//! Insert-and-push, the one push saved as a move (ADR 0006): only the command knows which shape
//! goes below which. Every other push is the spacing pass's and is never saved.

use std::collections::HashSet;

use super::Session;
use crate::{Link, Shape, ShapeId, Side};

/// Shape `root` and every shape linked beyond it, not through `from`, of `n` shapes linked at
/// `ends` (index pairs; ends out of range are skipped).
pub(super) fn beyond(n: usize, ends: &[(usize, usize)], root: usize, from: usize) -> Vec<usize> {
    let mut next = vec![Vec::new(); n];
    for &(a, b) in ends.iter().filter(|&&(a, b)| a < n && b < n) {
        next[a].push(b);
        next[b].push(a);
    }
    let (mut out, mut seen) = (vec![root], HashSet::from([root, from]));
    let mut i = 0;
    while let Some(&s) = out.get(i) {
        out.extend(next[s].iter().filter(|&&k| seen.insert(k)));
        i += 1;
    }
    out
}

impl Session {
    pub(super) fn index_of(&self, id: &ShapeId) -> Option<usize> {
        self.doc.shapes.iter().position(|s| &s.id == id)
    }

    /// `root` and every shape linked beyond it, not through `from` (shape indices): the
    /// subtree a push moves as one block.
    pub(super) fn subtree(&self, root: usize, from: usize) -> Vec<usize> {
        beyond(self.doc.shapes.len(), &self.link_ends(), root, from)
    }

    /// The shapes linked to `id` on the `side` the writer sees (the side rule on drawn centers),
    /// nearest center first; ties go in generation order.
    pub(super) fn linked_on(&self, id: &ShapeId, side: Side) -> Vec<ShapeId> {
        let find = |id: &ShapeId| self.doc.shapes.iter().find(|s| &s.id == id);
        let center = |s: &Shape| self.drawn(s).center();
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
        let (x, y) = side.unit();
        let d = need.max(0.0).ceil() as i32;
        (x * d, y * d)
    }

    /// Moves the shapes at indices `block` by (dx, dy): a saved move in the open undo step,
    /// sliding there from where they were drawn.
    pub(super) fn push(&mut self, block: &[usize], dx: i32, dy: i32) {
        if (dx, dy) == (0, 0) {
            return;
        }
        self.history.step(&mut self.doc, |h, doc| {
            for &i in block {
                h.change_at(doc, i, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
            }
        });
        let spaced = self.spaced.get_mut();
        for &i in block {
            spaced.slide_moved(&self.doc.shapes[i].id, (dx, dy), self.theme, self.now);
        }
    }
}
