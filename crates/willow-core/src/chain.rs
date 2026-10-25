//! Links and chains: commands that keep every chain a tree of one color.

use std::collections::{HashMap, HashSet};

use crate::{Document, Point, Shape, ShapeId};

/// A link between two shapes. Unordered, and it stores no sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link(pub ShapeId, pub ShapeId);

/// A side of a shape, as faced by the other end of a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    /// The side facing this one across a link.
    pub fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    /// One step toward this side, y down: (dx, dy).
    pub fn unit(self) -> (i32, i32) {
        match self {
            Side::Top => (0, -1),
            Side::Bottom => (0, 1),
            Side::Left => (-1, 0),
            Side::Right => (1, 0),
        }
    }
}

/// Where a `w` x `h` shape goes `gap` from `a` (`ah` tall) on `side`: centered on that side's
/// midpoint, so the connector between them runs straight.
pub(crate) fn beside(a: &Shape, ah: f32, w: u32, h: f32, side: Side, gap: f32) -> (i32, i32) {
    let (ax, ay, aw) = (a.x as f32, a.y as f32, a.width as f32);
    let (cx, cy) = (ax + (aw - w as f32) / 2.0, ay + (ah - h) / 2.0);
    let (x, y) = match side {
        Side::Bottom => (cx, ay + ah + gap),
        Side::Top => (cx, ay - h - gap),
        Side::Right => (ax + aw + gap, cy),
        Side::Left => (ax - w as f32 - gap, cy),
    };
    (x.round() as i32, y.round() as i32)
}

/// The side of the shape centered at `from` that faces the shape centered at `to`: vertical
/// when |dy| ≥ |dx| (so an exact diagonal is vertical), else horizontal. Drawing, the saved
/// text and story order all use this one rule.
pub fn side(from: Point, to: Point) -> Side {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    match (dy.abs() >= dx.abs(), dy >= 0.0, dx >= 0.0) {
        (true, true, _) => Side::Bottom,
        (true, false, _) => Side::Top,
        (false, _, true) => Side::Right,
        (false, _, false) => Side::Left,
    }
}

impl Document {
    /// Links `a` and `b`, unless that would close a cycle (they're already in one chain) or
    /// either is missing. Joining two chains repaints `b`'s in `a`'s color. True if linked.
    pub fn link(&mut self, a: &ShapeId, b: &ShapeId) -> bool {
        let Some(color) = self.link_color(a, b) else {
            return false;
        };
        self.recolor(b, color);
        self.links.push(Link(a.clone(), b.clone()));
        true
    }

    /// The color linking `a` to `b` paints `b`'s chain (`a`'s), or `None` when the link is
    /// refused: a missing shape, or both already in one chain (same shape, linked, a cycle).
    // ponytail: walks every link per call; a bulk loader wants union-find instead.
    pub(crate) fn link_color(&self, a: &ShapeId, b: &ShapeId) -> Option<crate::Color> {
        let color = self.shapes.iter().find(|s| &s.id == a)?.color;
        let adopted = self.chain(b);
        (!adopted.is_empty() && !adopted.contains(&a)).then_some(color)
    }

    /// Removes the link between `a` and `b`, in either order. Both halves keep the chain's
    /// color. True if there was one.
    pub fn unlink(&mut self, a: &ShapeId, b: &ShapeId) -> bool {
        let len = self.links.len();
        (self.links).retain(|Link(x, y)| !((x, y) == (a, b) || (x, y) == (b, a)));
        self.links.len() < len
    }

    /// Paints `id`'s whole chain `color`.
    pub fn recolor(&mut self, id: &ShapeId, color: crate::Color) {
        let chain: HashSet<ShapeId> = self.chain(id).into_iter().cloned().collect();
        for s in &mut self.shapes {
            if chain.contains(&s.id) {
                s.color = color;
            }
        }
    }

    /// The shapes reachable from `id` through links, `id` first; empty if there's no such shape.
    pub fn chain(&self, id: &ShapeId) -> Vec<&ShapeId> {
        let Some(start) = self.shapes.iter().find(|s| &s.id == id) else {
            return Vec::new();
        };
        let mut next: HashMap<&ShapeId, Vec<&ShapeId>> = HashMap::new();
        for Link(a, b) in &self.links {
            next.entry(a).or_default().push(b);
            next.entry(b).or_default().push(a);
        }
        let (mut chain, mut seen) = (vec![&start.id], HashSet::from([&start.id]));
        let mut i = 0;
        while let Some(s) = chain.get(i) {
            for &n in next.get(s).into_iter().flatten() {
                if seen.insert(n) {
                    chain.push(n);
                }
            }
            i += 1;
        }
        chain
    }
}
