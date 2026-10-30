//! The `.willow` text format: `save` writes canonical text, `load` reads it back.

use std::collections::HashMap;
use std::fmt::{self, Write};

use crate::check::{LineKind, check};
use crate::edit::build;
use crate::{
    Color, DEFAULT_WIDTH, Document, Link, Point, RichText, Shape, ShapeId, Side, Theme, side,
};

/// Loaded text: the document built from the lines that parse, whether loading had to repair
/// or drop anything (which, with no invalid line, leaves the document unsaved), and what each
/// line is.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub document: Document,
    pub repaired: bool,
    /// What the file gave no layout; `Session::open` places it, since that needs heights.
    pub unplaced: Unplaced,
    /// One per line of the text (`str::lines`), the header first; `check` gives at least one.
    pub lines: Vec<LineKind>,
}

impl Loaded {
    /// Some line is invalid: the text holds the document, and edits to it are held back.
    pub fn invalid(&self) -> bool {
        self.lines.contains(&LineKind::Invalid)
    }
}

/// Shapes and zones (indices into the loaded document) still to be placed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unplaced {
    /// Each shape, and the shape it goes beside and on which side of it; `None` goes below
    /// the lowest shape.
    pub(crate) shapes: Vec<(usize, Option<(usize, Side)>)>,
    pub(crate) zones: Vec<usize>,
}

/// Why a file can't open. `Display` is the reason an alert names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    NotWillow,
    Newer,
    NotUtf8,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LoadError::NotWillow => "It is not a Willow file.",
            LoadError::Newer => "It was made by a newer Willow.",
            LoadError::NotUtf8 => "It is not UTF-8 text.",
        })
    }
}

impl std::error::Error for LoadError {}

/// Reads a file's bytes: an error for anything but UTF-8 text with a `willow 1` header, else
/// `check` of the text.
pub fn load(bytes: &[u8]) -> Result<Loaded, LoadError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let version = (lines.next())
        .and_then(|l| l.trim_end().strip_prefix("willow "))
        .ok_or(LoadError::NotWillow)?;
    if version != "1" {
        // Any other version written as Willow writes numbers (`2`, `1.1`, `2.0`) is newer.
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let (major, minor) = version.split_once('.').unwrap_or((version, "0"));
        let newer = digits(major) && digits(minor) && !major.starts_with('0');
        return Err(if newer {
            LoadError::Newer
        } else {
            LoadError::NotWillow
        });
    }
    Ok(check(text))
}

/// Places what `unplaced` lists. `height` measures a shape.
pub(crate) fn place(
    doc: &mut Document,
    unplaced: &Unplaced,
    theme: &Theme,
    height: impl Fn(&Shape) -> f32,
) {
    position(doc, unplaced, theme, &height);
}

/// A shape goes one creation gap from its anchor on its side, or below the lowest shape (the
/// one whose bottom edge is lowest) when it has none; a zone wraps its members with padding.
/// Every shape is measured only when something goes below the lowest.
fn position(
    doc: &mut Document,
    unplaced: &Unplaced,
    theme: &Theme,
    height: &impl Fn(&Shape) -> f32,
) {
    let (gap, pad) = (theme.creation_gap, theme.zone.padding);
    let shapes = &mut doc.shapes;
    let mut anchor = vec![None; shapes.len()];
    let mut placed = vec![true; shapes.len()];
    for &(i, a) in &unplaced.shapes {
        (placed[i], anchor[i]) = (false, a);
    }
    // The lowest bottom edge and its shape's x. Measuring every shape for it is paid only
    // when something goes below the lowest.
    let needs_lowest = !unplaced.zones.is_empty() || unplaced.shapes.iter().any(|s| s.1.is_none());
    let bottom = |s: &Shape| (s.y as f32 + height(s), s.x);
    let mut lowest = (0..shapes.len())
        .filter(|&i| needs_lowest && placed[i])
        .map(|i| bottom(&shapes[i]))
        .max_by(|a, b| a.partial_cmp(b).expect("finite heights"));
    // Below the lowest shape: its x, one gap under its bottom.
    let below = |lowest: Option<(f32, i32)>| match lowest {
        Some((b, x)) => (x, (b + gap).round() as i32),
        None => (0, 0),
    };
    for &(start, _) in &unplaced.shapes {
        // Walk anchors up to a placed shape, then place back down.
        let (mut path, mut i) = (Vec::new(), start);
        while !placed[i] {
            path.push(i);
            match anchor[i] {
                Some((j, _)) => i = j,
                None => break,
            }
        }
        for &i in path.iter().rev() {
            let (x, y) = match anchor[i] {
                Some((j, side)) => {
                    let (a, s) = (&shapes[j], &shapes[i]);
                    crate::chain::beside(a, height(a), s.width, height(s), side, gap)
                }
                None => below(lowest),
            };
            (shapes[i].x, shapes[i].y, placed[i]) = (x, y, true);
            if needs_lowest {
                let b = bottom(&shapes[i]);
                if lowest.is_none_or(|l| b > l) {
                    lowest = Some(b);
                }
            }
        }
    }
    if unplaced.zones.is_empty() {
        return;
    }
    let index: HashMap<&ShapeId, usize> =
        shapes.iter().enumerate().map(|(i, s)| (&s.id, i)).collect();
    let zindex: HashMap<ShapeId, usize> = (doc.zones.iter().enumerate())
        .map(|(i, z)| (z.id.clone(), i))
        .collect();
    // Nested zones come after their parents, so going backwards wraps them first.
    for &z in unplaced.zones.iter().rev() {
        let rects = doc.zones[z].members.iter().filter_map(|m| {
            if let Some(&i) = index.get(m) {
                let s = &shapes[i];
                Some((s.x as f32, s.y as f32, s.width as f32, height(s)))
            } else {
                let c = &doc.zones[*zindex.get(m)?];
                Some((c.x as f32, c.y as f32, c.width as f32, c.height as f32))
            }
        });
        let wrapped = wrap(rects, pad);
        let zone = &mut doc.zones[z];
        ((zone.x, zone.y), zone.width, zone.height) = match wrapped {
            Some((x, y, w, h)) => ((x, y), w, h),
            // ponytail: an empty zone gets a small box below the lowest shape; a later
            // spacing pass may move it.
            None => (
                below(lowest),
                DEFAULT_WIDTH + 2 * pad as u32,
                2 * pad as u32,
            ),
        };
    }
}

/// The rectangle (x, y, width, height) around `rects` (the same) padded by `pad` on every
/// side, rounded out to whole canvas units; `None` for no rectangles. A loaded zone without a
/// position and a created zone both wrap their members with it.
pub(crate) fn wrap(
    rects: impl IntoIterator<Item = (f32, f32, f32, f32)>,
    pad: f32,
) -> Option<(i32, i32, u32, u32)> {
    let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
    for (x, y, w, h) in rects {
        (lo, hi) = (
            (lo.0.min(x), lo.1.min(y)),
            (hi.0.max(x + w), hi.1.max(y + h)),
        );
    }
    if lo.0 > hi.0 {
        return None;
    }
    let (x, y) = ((lo.0 - pad).floor(), (lo.1 - pad).floor());
    let (w, h) = ((hi.0 + pad).ceil() - x, (hi.1 + pad).ceil() - y);
    Some((x as i32, y as i32, w as u32, h as u32))
}

pub(crate) fn is_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase())
}

/// A shape's x, y, width and extra height, or a zone's x, y, width and height.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Place {
    Shape(i32, i32, u32, u32),
    Zone(i32, i32, u32, u32),
}

/// Loaded coordinates and sizes are clamped to this, so `x + width`, and a copy beside it,
/// stay well inside `i32`.
const LIMIT: i32 = 500_000_000;

impl Place {
    /// Coordinates and sizes clamped to `LIMIT`, a zero shape width made the default; and
    /// whether nothing changed.
    pub(crate) fn clamped(self) -> (Place, bool) {
        let at = |v: i32| v.clamp(-LIMIT, LIMIT);
        let size = |v: u32| v.min(LIMIT as u32);
        let fixed = match self {
            Place::Shape(x, y, 0, h) => Place::Shape(at(x), at(y), DEFAULT_WIDTH, size(h)),
            Place::Shape(x, y, w, h) => Place::Shape(at(x), at(y), size(w), size(h)),
            Place::Zone(x, y, w, h) => Place::Zone(at(x), at(y), size(w), size(h)),
        };
        (fixed, fixed == self)
    }
}

/// `id x,y wW [hH]` for a shape, `id x,y WxH` for a zone. A bare `id x,y` is a shape of the
/// default width, and not whole (a repair).
pub(crate) fn parse_layout(rest: &str) -> Option<(&str, Place, bool)> {
    let mut t = rest.split(' ');
    let id = t.next().filter(|id| is_id(id))?;
    let (x, y) = t.next()?.split_once(',')?;
    let (x, y) = (x.parse().ok()?, y.parse().ok()?);
    let Some(size) = t.next() else {
        return Some((id, Place::Shape(x, y, DEFAULT_WIDTH, 0), false));
    };
    let place = match size.strip_prefix('w') {
        Some(w) => {
            let h = match t.next() {
                Some(h) => h.strip_prefix('h')?.parse().ok()?,
                None => 0,
            };
            Place::Shape(x, y, w.parse().ok()?, h)
        }
        None => {
            let (w, h) = size.split_once('x')?;
            Place::Zone(x, y, w.parse().ok()?, h.parse().ok()?)
        }
    };
    t.next().is_none().then_some((id, place, true))
}

/// After a shape's text: `[below|above a | right of a|left of a] [color name]`. The ID linked
/// to with the side of it this shape sits on, the stated color, and whether all of it read
/// (a bad link or color drops only itself).
#[allow(clippy::type_complexity)]
pub(crate) fn parse_tail(rest: &str) -> (Option<(&str, Side)>, Option<Color>, bool) {
    let t: Vec<&str> = rest.split_whitespace().collect();
    let (to, t) = match t.as_slice() {
        ["below", to, t @ ..] => (Some((*to, Side::Bottom)), t),
        ["above", to, t @ ..] => (Some((*to, Side::Top)), t),
        ["right", "of", to, t @ ..] => (Some((*to, Side::Right)), t),
        ["left", "of", to, t @ ..] => (Some((*to, Side::Left)), t),
        t => (None, t),
    };
    let good = to.filter(|(to, _)| is_id(to));
    let color = parse_color(t);
    (good, color.flatten(), good == to && color.is_some())
}

/// `[]` or `["color", name]`.
fn parse_color(t: &[&str]) -> Option<Option<Color>> {
    match t {
        [] => Some(None),
        ["color", name] => Color::ALL.into_iter().find(|c| c.name() == *name).map(Some),
        _ => None,
    }
}

/// `id "Title" [color name] {` (after `zone `).
pub(crate) fn parse_zone(rest: &str) -> Option<(&str, String, Color)> {
    let (id, rest) = rest.split_once(" \"")?;
    let (mut title, mut chars) = (String::new(), rest.chars());
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => title.push(match chars.next()? {
                'n' => '\n',
                c => c,
            }),
            c => title.push(c),
        }
    }
    let t: Vec<&str> = chars.as_str().split_whitespace().collect();
    let (&"{", color) = t.split_last()? else {
        return None;
    };
    is_id(id).then_some((id, title, parse_color(color)?.unwrap_or_default()))
}

/// `id: "text"` and whatever follows the closing quote; `None` after a quote left open,
/// which runs to the end of the line.
pub(crate) fn parse_shape(line: &str) -> Option<(&str, RichText, Option<&str>)> {
    let (id, rest) = line.split_once(": \"")?;
    if !is_id(id) {
        return None;
    }
    let (mut plain, mut styles, mut style) = (String::new(), Vec::new(), (false, false));
    let mut chars = rest.chars();
    loop {
        let Some(ch) = chars.next() else {
            return Some((id, build(&plain, &styles), None));
        };
        let ch = match ch {
            '"' => return Some((id, build(&plain, &styles), Some(chars.as_str()))),
            // A backslash ending the line escapes nothing.
            '\\' => match chars.next().unwrap_or('\\') {
                'n' => '\n',
                c => c,
            },
            '*' => {
                // A run of stars toggles: `*` italic, `**` bold, `***` both.
                let k = 1 + chars.clone().take_while(|&c| c == '*').count();
                chars.by_ref().take(k - 1).for_each(drop);
                style.0 ^= k & 2 != 0;
                style.1 ^= k & 1 != 0;
                continue;
            }
            c => c,
        };
        plain.push(ch);
        let s = if ch == '\n' { (false, false) } else { style };
        styles.extend(std::iter::repeat_n(s, ch.len_utf8()));
    }
}

/// Where a shape or block sorts in story order: topmost, then leftmost, then ID in
/// generation order (a … z, aa).
type Key<'a> = (i32, i32, usize, &'a str);

fn key(s: &Shape) -> Key<'_> {
    (s.y, s.x, s.id.0.len(), &s.id.0)
}

/// Something written inside a block: a chain's shapes in that block (walk order), or a zone.
enum Unit {
    Chain(Vec<usize>),
    Zone(usize),
}

/// Canonical text for `doc` in story order (ADR 0002): header, blocks, then the layout.
/// `heights[i]` is `doc.shapes[i]`'s drawn height (`Session::heights`), which the side rule
/// needs.
pub fn save(doc: &Document, heights: &[f32]) -> String {
    let (shapes, zones) = (&doc.shapes, &doc.zones);
    let index: HashMap<&ShapeId, usize> =
        shapes.iter().enumerate().map(|(i, s)| (&s.id, i)).collect();
    let zindex: HashMap<&ShapeId, usize> =
        zones.iter().enumerate().map(|(i, z)| (&z.id, i)).collect();
    // Blocks: 0 is the top level, z + 1 is zone z.
    let (mut block, mut parent) = (vec![0; shapes.len()], vec![0; zones.len()]);
    for (z, zone) in zones.iter().enumerate() {
        for m in &zone.members {
            if let Some(&i) = index.get(m) {
                block[i] = z + 1;
            } else if let Some(&c) = zindex.get(m) {
                parent[c] = z + 1;
            }
        }
    }

    let center = |i: usize| shapes[i].center(heights.get(i).copied().unwrap_or(0.0));
    let Walks { walks, chain, from } = walks(doc, &center);

    // Units per block, each keyed by its topmost shape.
    let mut units: Vec<Vec<(Key, Unit)>> = (0..=zones.len()).map(|_| Vec::new()).collect();
    let mut zkey: Vec<Option<Key>> = vec![None; zones.len()];
    for walk in &walks {
        let mut parts: HashMap<usize, Vec<usize>> = HashMap::new();
        for &i in walk {
            parts.entry(block[i]).or_default().push(i);
            // Every enclosing zone sorts by its topmost shape.
            let (k, mut b) = (key(&shapes[i]), block[i]);
            for _ in 0..zones.len() {
                if b == 0 {
                    break;
                }
                zkey[b - 1] = Some(zkey[b - 1].map_or(k, |z| z.min(k)));
                b = parent[b - 1];
            }
        }
        for (b, part) in parts {
            let k = (part.iter().map(|&i| key(&shapes[i])).min()).expect("a part has shapes");
            units[b].push((k, Unit::Chain(part)));
        }
    }
    for (z, zone) in zones.iter().enumerate() {
        // An empty zone sorts by its own corner.
        let k = zkey[z].unwrap_or((zone.y, zone.x, zone.id.0.len(), &zone.id.0));
        units[parent[z]].push((k, Unit::Zone(z)));
    }
    for u in &mut units {
        u.sort_by_key(|(k, _)| *k);
    }

    let mut w = Writer {
        doc,
        center: &center,
        units: &units,
        chain: &chain,
        from: &from,
        colored: vec![false; walks.len()],
        out: String::from("willow 1\n"),
        shapes: Vec::new(),
        zones: Vec::new(),
    };
    for unit in &units[0] {
        w.out.push('\n');
        w.unit(&unit.1, 0);
    }
    let Writer {
        mut out,
        shapes: written,
        zones: zones_written,
        ..
    } = w;
    if !units[0].is_empty() {
        out.push('\n');
    }
    for i in written {
        let s = &shapes[i];
        let _ = write!(out, "@layout {} {},{} w{}", s.id.0, s.x, s.y, s.width);
        if s.extra_height > 0 {
            let _ = write!(out, " h{}", s.extra_height);
        }
        out.push('\n');
    }
    for z in zones_written {
        let z = &zones[z];
        let _ = writeln!(
            out,
            "@layout {} {},{} {}x{}",
            z.id.0, z.x, z.y, z.width, z.height
        );
    }
    out
}

/// Every shape (indices into `doc.shapes`) in story order within its chain: chains from their
/// topmost shape, each walked as `save` walks it. Chains come in no particular order.
pub(crate) fn story_order(doc: &Document, heights: &[f32]) -> Vec<usize> {
    let center = |i: usize| doc.shapes[i].center(heights.get(i).copied().unwrap_or(0.0));
    walks(doc, &center).walks.concat()
}

/// Every chain walked depth-first from its topmost shape (`walk`), chains in order of their
/// topmost shapes; each shape's chain (an index into `walks`) and the shape it was reached
/// from.
struct Walks {
    walks: Vec<Vec<usize>>,
    chain: Vec<usize>,
    from: Vec<Option<usize>>,
}

fn walks(doc: &Document, center: &dyn Fn(usize) -> Point) -> Walks {
    let (shapes, next) = (&doc.shapes, neighbors(doc));
    let mut tops: Vec<usize> = (0..shapes.len()).collect();
    tops.sort_by_key(|&i| key(&shapes[i]));
    let (mut chain, mut from) = (vec![usize::MAX; shapes.len()], vec![None; shapes.len()]);
    let mut walks = Vec::new();
    for top in tops {
        if chain[top] == usize::MAX {
            let c = walks.len();
            walks.push(walk(top, c, &next, shapes, center, &mut chain, &mut from));
        }
    }
    Walks { walks, chain, from }
}

/// Each shape's linked shapes (indices into `doc.shapes`).
fn neighbors(doc: &Document) -> Vec<Vec<usize>> {
    let index: HashMap<&ShapeId, usize> = (doc.shapes.iter().enumerate())
        .map(|(i, s)| (&s.id, i))
        .collect();
    let mut next = vec![Vec::new(); doc.shapes.len()];
    for Link(a, b) in &doc.links {
        if let (Some(&a), Some(&b)) = (index.get(a), index.get(b)) {
            next[a].push(b);
            next[b].push(a);
        }
    }
    next
}

/// Walks `top`'s chain in story order: depth-first, neighbors below, right, left, then above.
/// Marks each shape reached in `chain` (as chain `c`; unvisited is `usize::MAX`) and the shape
/// it was reached from in `from`.
fn walk(
    top: usize,
    c: usize,
    next: &[Vec<usize>],
    shapes: &[Shape],
    center: &dyn Fn(usize) -> Point,
    chain: &mut [usize],
    from: &mut [Option<usize>],
) -> Vec<usize> {
    let (mut walk, mut stack) = (Vec::new(), vec![(top, None)]);
    while let Some((i, f)) = stack.pop() {
        if chain[i] != usize::MAX {
            continue;
        }
        (chain[i], from[i]) = (c, f);
        walk.push(i);
        let at = center(i);
        let mut ns: Vec<usize> = (next[i].iter().copied())
            .filter(|&n| chain[n] == usize::MAX)
            .collect();
        ns.sort_by_key(|&n| (rank(side(at, center(n))), key(&shapes[n])));
        stack.extend(ns.into_iter().rev().map(|n| (n, Some(i))));
    }
    walk
}

/// Neighbors are visited below, right, left, then above.
fn rank(s: Side) -> u8 {
    match s {
        Side::Bottom => 0,
        Side::Right => 1,
        Side::Left => 2,
        Side::Top => 3,
    }
}

/// Writes blocks, remembering the order shapes and zones were written in for the layout.
struct Writer<'a> {
    doc: &'a Document,
    center: &'a dyn Fn(usize) -> Point,
    units: &'a [Vec<(Key<'a>, Unit)>],
    chain: &'a [usize],
    from: &'a [Option<usize>],
    /// Whether each chain's color has been written (on its first shape in story order).
    colored: Vec<bool>,
    out: String,
    shapes: Vec<usize>,
    zones: Vec<usize>,
}

impl Writer<'_> {
    fn unit(&mut self, unit: &Unit, depth: usize) {
        let indent = "  ".repeat(depth);
        match unit {
            Unit::Chain(part) => {
                for &i in part {
                    self.shape(i, &indent);
                }
            }
            &Unit::Zone(z) => {
                let zone = &self.doc.zones[z];
                let _ = write!(self.out, "{indent}zone {} \"", zone.id.0);
                escape(&mut self.out, &zone.title);
                self.out.push('"');
                if zone.color != Color::Neutral {
                    let _ = write!(self.out, " color {}", zone.color.name());
                }
                self.out.push_str(" {\n");
                self.zones.push(z);
                for (_, u) in &self.units[z + 1] {
                    self.unit(u, depth + 1);
                }
                let _ = writeln!(self.out, "{indent}}}");
            }
        }
    }

    fn shape(&mut self, i: usize, indent: &str) {
        let s = &self.doc.shapes[i];
        let _ = write!(self.out, "{indent}{}: \"{}\"", s.id.0, text(&s.text));
        if let Some(f) = self.from[i] {
            let word = match side((self.center)(f), (self.center)(i)) {
                Side::Bottom => "below",
                Side::Top => "above",
                Side::Right => "right of",
                Side::Left => "left of",
            };
            let _ = write!(self.out, " {word} {}", self.doc.shapes[f].id.0);
        }
        let c = self.chain[i];
        if !self.colored[c] {
            self.colored[c] = true;
            if s.color != Color::Neutral {
                let _ = write!(self.out, " color {}", s.color.name());
            }
        }
        self.out.push('\n');
        self.shapes.push(i);
    }
}

/// Pushes `s` with `"`, `*` and `\` escaped and line breaks as `\n`.
fn escape(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '"' | '*' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            c => out.push(c),
        }
    }
}

/// Text inside the quotes: escapes, `\n` between paragraphs, and style markers that toggle
/// on each change, closed before each `\n` and reopened after.
fn text(t: &RichText) -> String {
    let mut out = String::new();
    for (i, p) in t.paragraphs.iter().enumerate() {
        if i > 0 {
            out.push_str("\\n");
        }
        let mut style = (false, false);
        let runs = p.runs.iter().filter(|r| !r.text.is_empty());
        for (next, text) in runs
            .map(|r| ((r.bold, r.italic), &r.text))
            .chain([((false, false), &String::new())])
        {
            out.push_str(marker(style, next));
            style = next;
            escape(&mut out, text);
        }
    }
    out
}

/// The marker that turns style `from` into `to`: `**` toggles bold, `*` italic, `***` both.
fn marker(from: (bool, bool), to: (bool, bool)) -> &'static str {
    match (from.0 != to.0, from.1 != to.1) {
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "*",
        (false, false) => "",
    }
}
