//! The spacing pass (ADR 0006): how far each shape is drawn off its placed position, from
//! placed rectangles, links and the theme's gaps alone. No history, nothing kept between runs,
//! and no dependence on document order, so a document has one picture per theme.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::RangeInclusive;

use crate::{Point, Shape, Side, side};

/// A shape's rectangle, what the pass works on: its edges in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rect {
    pub(super) l: f32,
    pub(super) t: f32,
    pub(super) r: f32,
    pub(super) b: f32,
}

impl Rect {
    fn moved(self, (dx, dy): (i32, i32)) -> Rect {
        let (dx, dy) = (dx as f32, dy as f32);
        Rect {
            l: self.l + dx,
            t: self.t + dy,
            r: self.r + dx,
            b: self.b + dy,
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

/// The ways out of a crowd, ties going to the earlier.
const WAYS: [Side; 4] = [Side::Bottom, Side::Right, Side::Left, Side::Top];

/// Closer than this to the gap is float noise, not crowding.
const SLACK: f32 = 1e-3;

/// The index's cell size: about two default shapes across.
const CELL: f32 = 512.0;
/// More cells than this and a rectangle isn't listed by cell.
const MOST_CELLS: i64 = 4096;

/// A cheap hash for cell coordinates and shape IDs, read thousands of times a frame: SipHash
/// was a third of a pass.
#[derive(Default)]
pub(super) struct CheapHasher(u64);

impl CheapHasher {
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for CheapHasher {
    fn write(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|&b| self.add(b as u64));
    }

    fn write_i32(&mut self, v: i32) {
        self.add(v as u32 as u64);
    }

    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

/// `HashMap`'s third parameter for a map hashed by `CheapHasher`.
pub(super) type Cheap = BuildHasherDefault<CheapHasher>;

/// What is fixed so far. A rectangle is listed in every cell it touches, so a lookup costs
/// what is near it, not what is in the document.
#[derive(Default)]
struct Grid {
    cells: HashMap<(i32, i32), Vec<Rect>, Cheap>,
    /// Too big to list: a file may ask for a shape half a billion units across.
    huge: Vec<Rect>,
}

/// The columns and rows `r` touches, unless there are too many.
fn span(r: Rect) -> Option<(RangeInclusive<i32>, RangeInclusive<i32>)> {
    let cell = |v: f32| (v / CELL).floor() as i32;
    let (cols, rows) = (cell(r.l)..=cell(r.r), cell(r.t)..=cell(r.b));
    let count = |c: &RangeInclusive<i32>| *c.end() as i64 - *c.start() as i64 + 1;
    (count(&cols) * count(&rows) <= MOST_CELLS).then_some((cols, rows))
}

impl Grid {
    fn add(&mut self, r: Rect) {
        let Some((cols, rows)) = span(r) else {
            self.huge.push(r);
            return;
        };
        for c in cols {
            for w in rows.clone() {
                self.cells.entry((c, w)).or_default().push(r);
            }
        }
    }

    /// Calls `f` on every rectangle overlapping `r`, on some more than once.
    fn overlapping(&self, r: Rect, mut f: impl FnMut(Rect)) {
        let mut hit = |o: &Rect| {
            if r.overlaps(*o) {
                f(*o);
            }
        };
        self.huge.iter().for_each(&mut hit);
        let Some((cols, rows)) = span(r) else {
            self.cells.values().flatten().for_each(hit);
            return;
        };
        for c in cols {
            for w in rows.clone() {
                let cell = self.cells.get(&(c, w));
                cell.into_iter().flatten().for_each(&mut hit);
            }
        }
    }
}

/// The theme's two gaps.
#[derive(Clone, Copy)]
pub(super) struct Gaps {
    pub(super) creation: f32,
    pub(super) min: f32,
}

/// A shape as the walk through its chain reaches it.
#[derive(Clone, Copy)]
pub(super) struct Step {
    pub(super) shape: usize,
    /// The shape it was reached from, and the side of that shape it sits on.
    from: Option<(usize, Side)>,
}

/// Each chain, walked, in the order the pass fixes them. `ends` are the links as index pairs
/// (ends out of range are skipped). Reads placed positions only, so growth leaves it as it is.
/// Larger chains come first; then the chain with the highest top shape, the leftmost, the
/// lower ID.
pub(super) fn order(shapes: &[Shape], ends: &[(usize, usize)]) -> Vec<Vec<Step>> {
    let next = neighbors(shapes.len(), ends);
    let key = |i: usize| {
        let s = &shapes[i];
        (s.y, s.x, s.id.generation_order())
    };
    let top = |c: &Vec<usize>| {
        let top = c.iter().copied().min_by_key(|&i| key(i));
        (top.expect("a chain has a shape"), c.len())
    };
    let mut tops: Vec<_> = chains(&next).iter().map(top).collect();
    tops.sort_by_cached_key(|&(top, size)| (Reverse(size), key(top)));
    let mut seen = vec![false; shapes.len()];
    let walked = |&(top, _): &(usize, usize)| walk(top, shapes, &next, &mut seen);
    tops.iter().map(walked).collect()
}

/// The point link sides are judged from inside a chain: top edge and horizontal center, which
/// growth never changes.
fn anchor(s: &Shape) -> Point {
    Point {
        x: s.x as f32 + s.width as f32 / 2.0,
        y: s.y as f32,
    }
}

/// `top`'s chain depth-first, neighbors below, right, left, then above.
fn walk(top: usize, shapes: &[Shape], next: &[Vec<usize>], seen: &mut [bool]) -> Vec<Step> {
    let (mut walk, mut stack) = (Vec::new(), vec![(top, None)]);
    while let Some((shape, from)) = stack.pop() {
        if std::mem::replace(&mut seen[shape], true) {
            continue;
        }
        walk.push(Step { shape, from });
        let at = anchor(&shapes[shape]);
        let mut ahead: Vec<_> = (next[shape].iter().filter(|&&n| !seen[n]))
            .map(|&n| (side(at, anchor(&shapes[n])), n))
            .collect();
        ahead.sort_by_cached_key(|&(side, n)| {
            let way = WAYS.iter().position(|&w| w == side);
            let s = &shapes[n];
            (way, s.y, s.x, s.id.generation_order())
        });
        stack.extend(
            ahead
                .iter()
                .rev()
                .map(|&(side, n)| (n, Some((shape, side)))),
        );
    }
    walk
}

/// Spaces `chain` inside itself: `settled` gets each of its shapes' offsets from `rects`. The
/// walk fixes shapes one by one, so of two that crowd, the later moves. A shape starts where
/// the shape it was reached from went, which carries a subtree along with its root.
pub(super) fn settle(chain: &[Step], rects: &[Rect], gaps: Gaps, settled: &mut [(i32, i32)]) {
    let mut fixed = Grid::default();
    for step in chain {
        let mut at = (0, 0);
        let mut link = None;
        if let Some((from, side)) = step.from {
            at = settled[from];
            // In `gaps.min` terms: what the linked shape takes up, creation gap included.
            let near = rects[from].moved(at).grown(gaps.creation - gaps.min);
            let d = clear_of(rects[step.shape].moved(at), near, side, gaps.min);
            at = add(at, side.unit(), d);
            link = Some(near);
        }
        let r = rects[step.shape].moved(at);
        at = add(at, clearing(&[r], &fixed, link, gaps.min), 1);
        settled[step.shape] = at;
        fixed.add(rects[step.shape].moved(at));
    }
}

/// `at` moved by `by` times `d`, which rounding at huge coordinates may have overflown.
fn add(at: (i32, i32), by: (i32, i32), d: i32) -> (i32, i32) {
    let along = |a: i32, b: i32| a.saturating_add(b.saturating_mul(d));
    (along(at.0, by.0), along(at.1, by.1))
}

/// Each shape's offset, by index, from the chains in `order`, the shapes' placed rectangles,
/// how each chain settled and the minimum gap.
pub(super) fn pass(
    order: &[Vec<Step>],
    rects: &[Rect],
    settled: &[(i32, i32)],
    gap: f32,
) -> Vec<(i32, i32)> {
    let mut offsets = vec![(0, 0); rects.len()];
    let mut fixed = Grid::default();
    let mut block = Vec::new();
    // Each chain is fixed once, so pushes cascade down the order and the pass always ends.
    for chain in order {
        block.clear();
        block.extend(chain.iter().map(|s| rects[s.shape].moved(settled[s.shape])));
        let by = clearing(&block, &fixed, None, gap);
        for (step, r) in chain.iter().zip(&block) {
            offsets[step.shape] = add(settled[step.shape], by, 1);
            fixed.add(r.moved(by));
        }
    }
    offsets
}

/// Each shape's neighbors, by index.
fn neighbors(n: usize, ends: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut next = vec![Vec::new(); n];
    for &(a, b) in ends.iter().filter(|&&(a, b)| a < n && b < n) {
        next[a].push(b);
        next[b].push(a);
    }
    next
}

/// The shapes of each chain, by index.
fn chains(next: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = next.len();
    let mut seen = vec![false; n];
    let mut chains = Vec::new();
    for start in 0..n {
        if !seen[start] {
            seen[start] = true;
            let mut chain = vec![start];
            let mut i = 0;
            while let Some(&s) = chain.get(i) {
                for &k in &next[s] {
                    if !seen[k] {
                        seen[k] = true;
                        chain.push(k);
                    }
                }
                i += 1;
            }
            chains.push(chain);
        }
    }
    chains
}

/// How far `r` must go toward `side` to sit `gap` beyond `o`, or 0 if they don't crowd.
fn clear_of(r: Rect, o: Rect, side: Side, gap: f32) -> i32 {
    if r.grown(gap - SLACK).overlaps(o) {
        need(r, o, side, gap)
    } else {
        0
    }
}

/// How far `r` must go toward `side` to sit `gap` beyond `o`, in whole units.
fn need(r: Rect, o: Rect, side: Side, gap: f32) -> i32 {
    let far = match side {
        Side::Bottom => o.b + gap - r.t,
        Side::Top => r.b + gap - o.t,
        Side::Right => o.r + gap - r.l,
        Side::Left => r.r + gap - o.l,
    };
    (far - SLACK).ceil() as i32
}

/// The shortest move along one axis that clears `block` of every `fixed` rectangle and of
/// `also`.
fn clearing(block: &[Rect], fixed: &Grid, also: Option<Rect>, gap: f32) -> (i32, i32) {
    let mut best = (i32::MAX, (0, 0));
    for side in WAYS {
        let mut d = 0;
        while d < best.0
            && let Some(need) = crowded(block, fixed, also, side, d, gap)
        {
            // Rounding at huge coordinates must not stall the walk.
            d = need.max(d.saturating_add(1));
        }
        if d < best.0 {
            let (x, y) = side.unit();
            best = (d, (x * d, y * d));
        }
    }
    best.1
}

/// With `block` moved `d` toward `side`: how far from its place it must go that way to clear
/// what it crowds there, or `None` when it crowds nothing.
fn crowded(
    block: &[Rect],
    fixed: &Grid,
    also: Option<Rect>,
    side: Side,
    d: i32,
    gap: f32,
) -> Option<i32> {
    let (x, y) = side.unit();
    let mut most = None;
    for r in block {
        let reach = r.moved((x * d, y * d)).grown(gap - SLACK);
        let mut hit = |o: Rect| most = most.max(Some(need(*r, o, side, gap)));
        fixed.overlapping(reach, &mut hit);
        also.filter(|o| reach.overlaps(*o)).map(hit);
    }
    most
}
