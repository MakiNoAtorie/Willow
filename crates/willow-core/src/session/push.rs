//! Pushes: moving a block of shapes out of the way as a saved move (ADR 0004), recorded in the
//! open undo step, with a slide to the new place. Growth and creation push their own linked
//! subtrees to the creation gap and other chains to the minimum gap, cascading.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use super::Session;
use crate::input::Point;
use crate::{Link, Shape, ShapeId, Side};

/// A shape's drawn rectangle: its edges in canvas units.
#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

impl Rect {
    fn moved(self, dx: i32, dy: i32) -> Rect {
        let (dx, dy) = (dx as f32, dy as f32);
        Rect {
            l: self.l + dx,
            t: self.t + dy,
            r: self.r + dx,
            b: self.b + dy,
        }
    }

    fn union(self, o: Rect) -> Rect {
        Rect {
            l: self.l.min(o.l),
            t: self.t.min(o.t),
            r: self.r.max(o.r),
            b: self.b.max(o.b),
        }
    }

    fn grown(self, d: f32) -> Rect {
        Rect {
            l: self.l - d,
            t: self.t - d,
            r: self.r + d,
            b: self.b + d,
        }
    }

    fn overlaps(self, o: Rect) -> bool {
        self.l < o.r && o.l < self.r && self.t < o.b && o.t < self.b
    }
}

/// The sides a shape grows toward: its top never moves.
const GROWING: [Side; 3] = [Side::Bottom, Side::Left, Side::Right];

/// A move of `d` units toward `side`.
fn toward(side: Side, d: i32) -> (i32, i32) {
    match side {
        Side::Bottom => (0, d),
        Side::Top => (0, -d),
        Side::Right => (d, 0),
        Side::Left => (-d, 0),
    }
}

/// How far `o` must move toward `side` to sit `gap` beyond mover `m`, in whole units; 0 when
/// it needn't. `o` must lie within `gap` of `m`'s side lane, and have been wholly beyond `m`
/// toward `side` before the change (`om`, `oo`: their rectangles then; `om` is `None` for a
/// new shape, which crowds whatever is within `gap`), so shapes already overlapping stay.
/// A `carried` mover (a pushed block, not the change itself) keeps shapes it was already
/// closer to than `gap` at their old distance: pushed to the full gap, two interleaved chains
/// left that close would push each other forever.
fn crowding(
    m: Rect,
    om: Option<Rect>,
    o: Rect,
    oo: Rect,
    side: Side,
    gap: f32,
    carried: bool,
) -> i32 {
    let vertical = matches!(side, Side::Bottom | Side::Top);
    let lane = if vertical {
        o.l < m.r + gap && o.r > m.l - gap
    } else {
        o.t < m.b + gap && o.b > m.t - gap
    };
    // How far beyond `m` the shape was before the change; `None` if it wasn't.
    let was = om.map(|p| match side {
        Side::Bottom => oo.t - p.b,
        Side::Top => p.t - oo.b,
        Side::Right => oo.l - p.r,
        Side::Left => p.l - oo.r,
    });
    let beyond = match (om, was) {
        (Some(_), Some(d)) => d >= 0.0,
        _ => match side {
            Side::Bottom => o.b > m.t - gap,
            Side::Top => o.t < m.b + gap,
            Side::Right => o.r > m.l - gap,
            Side::Left => o.l < m.r + gap,
        },
    };
    let gap = match was {
        Some(d) if carried => gap.min(d),
        _ => gap,
    };
    let need = match side {
        Side::Bottom => m.b + gap - o.t,
        Side::Top => o.b + gap - m.t,
        Side::Right => m.r + gap - o.l,
        Side::Left => o.r + gap - m.l,
    };
    // A hair of slack: float noise must not round up to a whole-unit push.
    if lane && beyond && need > 1e-3 {
        need.ceil() as i32
    } else {
        0
    }
}

/// A pushed shape's slide: drawn `from` canvas units off its saved place at `since`, easing to 0.
#[derive(Debug)]
pub(super) struct Slide {
    from: Point,
    since: Duration,
}

impl Session {
    fn rect(&self, s: &Shape) -> Rect {
        let (x, y) = (s.x as f32, s.y as f32);
        Rect {
            l: x,
            t: y,
            r: x + s.width as f32,
            b: y + self.shape_height(s),
        }
    }

    /// The drawn rectangle of shape `id`.
    pub(super) fn rect_of(&self, id: &ShapeId) -> Option<Rect> {
        let s = self.doc.shapes.iter().find(|s| &s.id == id)?;
        Some(self.rect(s))
    }

    /// The drawn rectangle of the shape at index `i`.
    pub(super) fn rect_at(&self, i: usize) -> Option<Rect> {
        self.doc.shapes.get(i).map(|s| self.rect(s))
    }

    pub(super) fn index_of(&self, id: &ShapeId) -> Option<usize> {
        self.doc.shapes.iter().position(|s| &s.id == id)
    }

    /// Makes room after shape `id` changed from `old`: on each side it grew toward, the
    /// subtrees linked there move to keep the creation gap, and other chains it or they now
    /// crowd move to the minimum gap. Shrinking moves nothing.
    pub(super) fn make_room(&mut self, id: &ShapeId, old: Rect) {
        let (Some(i), Some(new)) = (self.index_of(id), self.rect_of(id)) else {
            return;
        };
        for side in GROWING {
            let out = match side {
                Side::Bottom => new.b > old.b,
                Side::Left => new.l < old.l,
                Side::Right => new.r > old.r,
                Side::Top => false,
            };
            if !out {
                continue;
            }
            let mut movers = vec![(i, Some(old))];
            for n in self.linked_on(id, side) {
                let Some(j) = self.index_of(&n) else { continue };
                let (near, far) = (&self.doc.shapes[i], &self.doc.shapes[j]);
                let (dx, dy) = self.clearance(near, far, side, self.theme.creation_gap);
                if (dx, dy) != (0, 0) {
                    let block = self.subtree(j, i);
                    movers.extend(block.iter().map(|&k| (k, self.rect_at(k))));
                    self.push(&block, dx, dy);
                }
            }
            self.clear_others(side, movers);
        }
    }

    /// Pushes other chains clear of `movers` toward `side` to the minimum gap, then whatever
    /// those crowd in turn. `movers` are shape indices with their rectangles before they moved
    /// (`None`: just created); the first one's chain never moves.
    pub(super) fn clear_others(&mut self, side: Side, start: Vec<(usize, Option<Rect>)>) {
        let Some(&(first, _)) = start.first() else {
            return;
        };
        let (gap, n) = (self.theme.min_gap, self.doc.shapes.len());
        // ponytail: lays out and labels every shape once per growth (not per keystroke); a
        // spatial index if 10k-shape growth shows up in the bench.
        let chain = self.chains();
        let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, &c) in chain.iter().enumerate() {
            members.entry(c).or_default().push(i);
        }
        let mut rects: Vec<Rect> = self.doc.shapes.iter().map(|s| self.rect(s)).collect();
        // Where each shape was before the change; order along `side` is judged there.
        let mut orig: Vec<Option<Rect>> = rects.iter().copied().map(Some).collect();
        for &(m, old) in &start {
            orig[m] = old;
        }
        let mut movers: Vec<usize> = start.into_iter().map(|(m, _)| m).collect();
        // Every block moves only toward `side`, and a carried block keeps what it was already
        // close to at that distance, so no chain of pushes comes back around for more and the
        // cascade ends once nothing is crowded. The cap only guards against a bug.
        for _ in 0..=members.len() {
            if movers.is_empty() {
                break;
            }
            // Everything a mover could crowd lies within `gap` of where it went or came from.
            let reach = (movers.iter())
                .map(|&m| orig[m].map_or(rects[m], |p| p.union(rects[m])))
                .reduce(Rect::union)
                .expect("movers")
                .grown(gap);
            let mut need: BTreeMap<usize, i32> = BTreeMap::new();
            for o in (0..n).filter(|&o| chain[o] != chain[first] && reach.overlaps(rects[o])) {
                let oo = orig[o].unwrap_or(rects[o]);
                for &m in movers.iter().filter(|&&m| chain[m] != chain[o]) {
                    let carried = chain[m] != chain[first];
                    let d = crowding(rects[m], orig[m], rects[o], oo, side, gap, carried);
                    if d > 0 {
                        let e = need.entry(chain[o]).or_default();
                        *e = (*e).max(d);
                    }
                }
            }
            movers.clear();
            for (c, d) in need {
                let (dx, dy) = toward(side, d);
                let block = &members[&c];
                for &k in block {
                    rects[k] = rects[k].moved(dx, dy);
                }
                movers.extend(block);
                self.push(block, dx, dy);
            }
        }
    }

    /// Each shape's chain as a label (a shape index), by shape index.
    fn chains(&self) -> Vec<usize> {
        let n = self.doc.shapes.len();
        let mut up: Vec<usize> = (0..n).collect();
        fn root(up: &mut [usize], mut i: usize) -> usize {
            while up[i] != i {
                up[i] = up[up[i]];
                i = up[i];
            }
            i
        }
        for &(a, b) in self.link_ends().iter().filter(|&&(a, b)| a < n && b < n) {
            let (ra, rb) = (root(&mut up, a), root(&mut up, b));
            up[ra] = rb;
        }
        (0..n).map(|i| root(&mut up, i)).collect()
    }

    /// `root` and every shape linked beyond it, not through `from` (shape indices): the
    /// subtree a push moves as one block.
    pub(super) fn subtree(&self, root: usize, from: usize) -> Vec<usize> {
        let n = self.doc.shapes.len();
        let mut next = vec![Vec::new(); n];
        for &(a, b) in self.link_ends().iter().filter(|&&(a, b)| a < n && b < n) {
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

    /// The shapes linked to `id` on its `side` (by the side rule), nearest center first; ties
    /// go in generation order.
    pub(super) fn linked_on(&self, id: &ShapeId, side: Side) -> Vec<ShapeId> {
        let find = |id: &ShapeId| self.doc.shapes.iter().find(|s| &s.id == id);
        let center = |s: &Shape| s.center(self.shape_height(s));
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
        toward(side, need.max(0.0).ceil() as i32)
    }

    /// Moves the shapes at indices `block` by (dx, dy): a saved move in the open undo step (or
    /// its own), sliding there from where they were drawn.
    pub(super) fn push(&mut self, block: &[usize], dx: i32, dy: i32) {
        if (dx, dy) == (0, 0) {
            return;
        }
        let now = self.now;
        self.history.step(&mut self.doc, |h, doc| {
            for &i in block {
                h.change_at(doc, i, |s| (s.x, s.y) = (s.x + dx, s.y + dy));
            }
        });
        for &i in block {
            let id = &self.doc.shapes[i].id;
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
