//! The spacing pass (ADR 0006): how far each shape is drawn off its placed position, from
//! placed rectangles, links and the theme's gaps alone. No history, nothing kept between runs,
//! and no dependence on document order, so a document has one picture per theme.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::RangeInclusive;

use super::push::Rect;
use crate::{Shape, Side};

/// The ways out of a crowd, ties going to the earlier.
const WAYS: [Side; 4] = [Side::Bottom, Side::Right, Side::Left, Side::Top];

/// Closer than this to the gap is float noise, not crowding.
const SLACK: f32 = 1e-3;

/// The index's cell size: about two default shapes across.
const CELL: f32 = 512.0;
/// More cells than this and a rectangle isn't listed by cell.
const MOST_CELLS: i64 = 4096;

/// A cheap hash of cell coordinates, which are the program's own: SipHash was a third of a pass.
#[derive(Default)]
struct CellHasher(u64);

impl Hasher for CellHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("cells hash as i32s");
    }

    fn write_i32(&mut self, v: i32) {
        self.0 = (self.0.rotate_left(5) ^ v as u32 as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

/// What is fixed so far. A rectangle is listed in every cell it touches, so a lookup costs
/// what is near it, not what is in the document.
#[derive(Default)]
struct Grid {
    cells: HashMap<(i32, i32), Vec<Rect>, BuildHasherDefault<CellHasher>>,
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

/// Each chain, as shape indices, in the order the pass fixes them. `ends` are the links as
/// index pairs (ends out of range are skipped). Reads placed positions only, so growth leaves
/// it as it is. Larger chains come first; then the chain with the highest top shape, the
/// leftmost, the lower ID.
pub(super) fn order(shapes: &[Shape], ends: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut chains = chains(shapes.len(), ends);
    chains.sort_by_cached_key(|c| {
        let top = (c.iter().map(|&i| &shapes[i]))
            .min_by_key(|s| (s.y, s.x, s.id.generation_order()))
            .expect("a chain has a shape");
        (Reverse(c.len()), top.y, top.x, top.id.generation_order())
    });
    chains
}

/// Each shape's offset, by index, from the chains in `order`, the shapes' placed rectangles
/// and the minimum gap.
pub(super) fn pass(order: &[Vec<usize>], rects: &[Rect], gap: f32) -> Vec<(i32, i32)> {
    let mut offsets = vec![(0, 0); rects.len()];
    let mut fixed = Grid::default();
    let mut block = Vec::new();
    // Slot for rules inside a chain (#96): a chain settles here, and `block` starts from that.
    // Each chain is fixed once, so pushes cascade down the order and the pass always ends.
    for chain in order {
        block.clear();
        block.extend(chain.iter().map(|&i| rects[i]));
        let (dx, dy) = clearing(&block, &fixed, gap);
        for &i in chain {
            offsets[i] = (dx, dy);
            fixed.add(rects[i].moved(dx, dy));
        }
    }
    offsets
}

/// The shapes of each chain, by index.
fn chains(n: usize, ends: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut next = vec![Vec::new(); n];
    for &(a, b) in ends.iter().filter(|&&(a, b)| a < n && b < n) {
        next[a].push(b);
        next[b].push(a);
    }
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

/// The shortest move along one axis that clears `block` of every `fixed` rectangle.
fn clearing(block: &[Rect], fixed: &Grid, gap: f32) -> (i32, i32) {
    let mut best = (i32::MAX, (0, 0));
    for side in WAYS {
        let mut d = 0;
        while d < best.0
            && let Some(need) = crowded(block, fixed, side, d, gap)
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
fn crowded(block: &[Rect], fixed: &Grid, side: Side, d: i32, gap: f32) -> Option<i32> {
    let (x, y) = side.unit();
    let mut need = None;
    for r in block {
        fixed.overlapping(r.moved(x * d, y * d).grown(gap - SLACK), |o| {
            let far = match side {
                Side::Bottom => o.b + gap - r.t,
                Side::Top => r.b + gap - o.t,
                Side::Right => o.r + gap - r.l,
                Side::Left => r.r + gap - o.l,
            };
            need = need.max(Some((far - SLACK).ceil() as i32));
        });
    }
    need
}
