//! The `.willow` text format: `save` writes canonical text, `load` reads it back.

use std::collections::HashMap;
use std::fmt::{self, Write};

use crate::edit::build;
use crate::{Color, Document, Link, Point, RichText, Shape, ShapeId, Side, Zone, side};

/// A loaded file: its document, and whether loading had to repair anything (which leaves
/// the document unsaved).
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub document: Document,
    pub repaired: bool,
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

/// Reads a file's bytes. CRLF line endings are accepted as they are, not as a repair.
/// Shapes stack in the order their lines appear. Links resolve after the whole file, so they
/// may point forward. Lines it can't read are dropped; links to missing shapes or closing a
/// cycle are dropped; a chain stating two colors keeps the first; shapes and zones without
/// layout keep the defaults. All of these count as repairs.
pub fn load(bytes: &[u8]) -> Result<Loaded, LoadError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LoadError::NotUtf8)?;
    let mut lines = text.lines();
    let version = (lines.next())
        .and_then(|l| l.strip_prefix("willow "))
        .ok_or(LoadError::NotWillow)?;
    match version.parse::<u32>() {
        Ok(1) => {}
        Ok(n) if n > 1 => return Err(LoadError::Newer),
        _ => return Err(LoadError::NotWillow),
    }
    let mut doc = Document::default();
    let (mut layout, mut repaired) = (HashMap::new(), false);
    // Open zone blocks (indices into `doc.zones`), and each shape's stated link and color.
    let (mut open, mut stated): (Vec<usize>, _) = (Vec::new(), Vec::new());
    for line in lines.map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("@layout ") {
            match parse_layout(rest) {
                Some((id, l)) => _ = layout.insert(id, l),
                None => repaired = true,
            }
        } else if line == "}" {
            repaired |= open.pop().is_none();
        } else if let Some((id, title, color)) = line.strip_prefix("zone ").and_then(parse_zone) {
            if let Some(&p) = open.last() {
                doc.zones[p].members.push(ShapeId(id.into()));
            }
            open.push(doc.zones.len());
            doc.zones.push(Zone {
                id: ShapeId(id.into()),
                title,
                color,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                members: Vec::new(),
            });
        } else if let Some((id, text, rest)) = parse_shape(line) {
            let mut s = Shape::new(ShapeId(id.into()), 0, 0);
            s.text = text;
            if let Some(&z) = open.last() {
                doc.zones[z].members.push(s.id.clone());
            }
            doc.shapes.push(s);
            let tail = parse_tail(rest);
            repaired |= tail.is_none();
            stated.push(tail.unwrap_or_default());
        } else {
            repaired = true;
        }
    }
    repaired |= !open.is_empty();
    for s in &mut doc.shapes {
        match layout.remove(s.id.0.as_str()) {
            Some(Place::Shape(x, y, w, h)) => (s.x, s.y, s.width, s.extra_height) = (x, y, w, h),
            _ => repaired = true,
        }
    }
    for z in &mut doc.zones {
        match layout.remove(z.id.0.as_str()) {
            Some(Place::Zone(x, y, w, h)) => (z.x, z.y, z.width, z.height) = (x, y, w, h),
            _ => repaired = true,
        }
    }
    repaired |= !layout.is_empty();
    repaired |= resolve(&mut doc, &stated);
    Ok(Loaded {
        document: doc,
        repaired,
    })
}

/// Links each shape to the one it names and paints every chain the first color one of its
/// shapes states. True if anything had to be dropped.
fn resolve(doc: &mut Document, stated: &[(Option<&str>, Option<Color>)]) -> bool {
    let index: HashMap<&str, usize> = (doc.shapes.iter().enumerate())
        .map(|(i, s)| (s.id.0.as_str(), i))
        .collect();
    let mut root: Vec<usize> = (0..doc.shapes.len()).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    let mut repaired = false;
    for (i, (to, _)) in stated.iter().enumerate() {
        let Some(to) = to else { continue };
        let Some(&j) = index.get(to) else {
            repaired = true;
            continue;
        };
        let (a, b) = (find(&mut root, i), find(&mut root, j));
        if a == b {
            repaired = true;
            continue;
        }
        root[a] = b;
        let link = Link(doc.shapes[j].id.clone(), doc.shapes[i].id.clone());
        doc.links.push(link);
    }
    let mut color: Vec<Option<Color>> = vec![None; doc.shapes.len()];
    for (i, &(_, c)) in stated.iter().enumerate() {
        let Some(c) = c else { continue };
        let r = find(&mut root, i);
        match color[r] {
            None => color[r] = Some(c),
            Some(first) => repaired |= first != c,
        }
    }
    for i in 0..doc.shapes.len() {
        let r = find(&mut root, i);
        doc.shapes[i].color = color[r].unwrap_or_default();
    }
    repaired
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase())
}

/// A shape's x, y, width and extra height, or a zone's x, y, width and height.
enum Place {
    Shape(i32, i32, u32, u32),
    Zone(i32, i32, u32, u32),
}

/// `id x,y wW [hH]` for a shape, `id x,y WxH` for a zone.
fn parse_layout(rest: &str) -> Option<(&str, Place)> {
    let mut t = rest.split(' ');
    let id = t.next().filter(|id| is_id(id))?;
    let (x, y) = t.next()?.split_once(',')?;
    let (x, y) = (x.parse().ok()?, y.parse().ok()?);
    let size = t.next()?;
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
    t.next().is_none().then_some((id, place))
}

/// After a shape's text: `[below|above a | right of a|left of a] [color name]`. The ID linked
/// to and the stated color.
fn parse_tail(rest: &str) -> Option<(Option<&str>, Option<Color>)> {
    let t: Vec<&str> = rest.split_whitespace().collect();
    let (to, t) = match t.as_slice() {
        ["below" | "above", to, t @ ..] | ["right" | "left", "of", to, t @ ..] => (Some(*to), t),
        t => (None, t),
    };
    if to.is_some_and(|to| !is_id(to)) {
        return None;
    }
    Some((to, parse_color(t)?))
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
fn parse_zone(rest: &str) -> Option<(&str, String, Color)> {
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

/// `id: "text"` and whatever follows the closing quote.
fn parse_shape(line: &str) -> Option<(&str, RichText, &str)> {
    let (id, rest) = line.split_once(": \"")?;
    if !is_id(id) {
        return None;
    }
    let (mut plain, mut styles, mut style) = (String::new(), Vec::new(), (false, false));
    let mut chars = rest.chars();
    loop {
        let ch = match chars.next()? {
            '"' => return Some((id, build(&plain, &styles), chars.as_str())),
            '\\' => match chars.next()? {
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

    // Walk each chain depth-first from its topmost shape, neighbors below, right, left, above.
    let center = |i: usize| Point {
        x: shapes[i].x as f32 + shapes[i].width as f32 / 2.0,
        y: shapes[i].y as f32 + heights.get(i).copied().unwrap_or(0.0) / 2.0,
    };
    let mut next = vec![Vec::new(); shapes.len()];
    for Link(a, b) in &doc.links {
        if let (Some(&a), Some(&b)) = (index.get(a), index.get(b)) {
            next[a].push(b);
            next[b].push(a);
        }
    }
    let mut order: Vec<usize> = (0..shapes.len()).collect();
    order.sort_by_key(|&i| key(&shapes[i]));
    let (mut chain, mut from) = (vec![usize::MAX; shapes.len()], vec![None; shapes.len()]);
    let mut walks: Vec<Vec<usize>> = Vec::new();
    for &top in &order {
        if chain[top] != usize::MAX {
            continue;
        }
        let (c, mut walk, mut stack) = (walks.len(), Vec::new(), vec![(top, None)]);
        while let Some((i, f)) = stack.pop() {
            if chain[i] != usize::MAX {
                continue;
            }
            (chain[i], from[i]) = (c, f);
            walk.push(i);
            let c = center(i);
            let mut ns: Vec<usize> = next[i]
                .iter()
                .copied()
                .filter(|&n| chain[n] == usize::MAX)
                .collect();
            ns.sort_by_key(|&n| (rank(side(c, center(n))), key(&shapes[n])));
            stack.extend(ns.into_iter().rev().map(|n| (n, Some(i))));
        }
        walks.push(walk);
    }

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
