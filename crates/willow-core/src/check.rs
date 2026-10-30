//! The line check: text read line by line into the document built from the lines that parse,
//! and what each line is. Files load through it, and the code panel checks its text with it.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use crate::format::{Place, parse_layout, parse_shape, parse_tail, parse_zone};
use crate::{Color, Document, Link, Loaded, Shape, ShapeId, Side, Unplaced, Zone};

/// What one line of text is to the line check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    /// The header, a blank line, a `}`, a zone's line or its `@layout` line.
    #[default]
    Fine,
    /// A shape's line or its `@layout` line.
    Shape(ShapeId),
    /// Holds text edits back until fixed: a reference to a missing ID, a duplicate ID (the
    /// later line), an unclosed zone (its `zone` line), a stray `}`, a link closing a cycle,
    /// a zone nested two levels deep, a malformed shape, zone or `@layout` line, or a missing
    /// header.
    Invalid,
    /// Skipped, and dropped by the next regeneration: an unknown line, or an `@layout` line
    /// for an ID with no shape or zone of its kind (or with one placed already).
    Ignored,
}

use LineKind::{Ignored, Invalid};

/// Reads `text` (a leading byte order mark, CRLF and whitespace around lines are fine).
/// Shapes stack in the order their lines appear. Links resolve after the whole text, so they
/// may point forward. Invalid and ignored lines add nothing, except that a shape line with a
/// bad link or color keeps its shape, a deep zone's members join its parent, and an unclosed
/// zone closes at the end. A quote left open runs to the end of its line. A chain stating two
/// colors takes the first in the text; shapes and zones without layout are listed in
/// `unplaced`. All of these, and every invalid or ignored line, count as repairs.
pub fn check(text: &str) -> Loaded {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = vec![LineKind::Fine; text.lines().count().max(1)];
    if text.lines().next().map(str::trim_end) != Some("willow 1") {
        lines[0] = Invalid;
    }
    let mut doc = Document::default();
    let (mut layout, mut repaired) = (HashMap::new(), false);
    // Open zone blocks (index into `doc.zones` and line; `None` for a dropped zone line),
    // each shape's line, stated link and color, and every ID taken so far.
    let (mut open, mut stated): (Vec<Option<(usize, usize)>>, _) = (Vec::new(), Vec::new());
    let mut seen = HashSet::new();
    for (n, line) in text.lines().enumerate().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Members join the innermost zone still standing.
        let inner = open.iter().rev().find_map(|z| z.map(|(z, _)| z));
        if let Some(rest) = line.strip_prefix("@layout ") {
            match parse_layout(rest) {
                Some((id, l, whole)) => {
                    let (l, fine) = l.clamped();
                    repaired |= !whole || !fine;
                    match layout.entry(id) {
                        Entry::Vacant(e) => _ = e.insert((l, n)),
                        Entry::Occupied(_) => (repaired, lines[n]) = (true, Ignored),
                    }
                }
                None => (repaired, lines[n]) = (true, Invalid),
            }
        } else if line == "}" {
            if open.pop().is_none() {
                (repaired, lines[n]) = (true, Invalid);
            }
        } else if let Some((id, title, color)) = line.strip_prefix("zone ").and_then(parse_zone) {
            // A zone inside a nested zone is flattened: its members join its parent.
            let deep = open.iter().flatten().count() >= 2;
            if deep || !seen.insert(id) {
                (repaired, lines[n]) = (true, Invalid);
                open.push(None);
                continue;
            }
            if let Some(p) = inner {
                doc.zones[p].members.push(ShapeId(id.into()));
            }
            open.push(Some((doc.zones.len(), n)));
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
            if !seen.insert(id) {
                (repaired, lines[n]) = (true, Invalid);
                continue;
            }
            let mut s = Shape::new(ShapeId(id.into()), 0, 0);
            s.text = text;
            if let Some(z) = inner {
                doc.zones[z].members.push(s.id.clone());
            }
            let (to, color, whole) = parse_tail(rest.unwrap_or(""));
            repaired |= !whole || rest.is_none();
            lines[n] = if whole {
                LineKind::Shape(s.id.clone())
            } else {
                Invalid
            };
            doc.shapes.push(s);
            stated.push((n, to, color));
        } else {
            repaired = true;
            // A line starting like a shape, zone or layout line is one, malformed.
            let word = line.split([' ', '"']).next().unwrap_or_default();
            let known = matches!(word, "zone" | "@layout") || word.len() > 1 && word.ends_with(':');
            lines[n] = if known { Invalid } else { Ignored };
            // An unreadable line opening a block still takes the `}` closing it.
            if line.ends_with('{') {
                open.push(None);
            }
        }
    }
    for &(_, n) in open.iter().flatten() {
        (repaired, lines[n]) = (true, Invalid);
    }
    repaired |= !open.is_empty();
    let mut missing = vec![false; doc.shapes.len()];
    for (i, s) in doc.shapes.iter_mut().enumerate() {
        match layout.remove(s.id.0.as_str()) {
            Some((Place::Shape(x, y, w, h), n)) => {
                (s.x, s.y, s.width, s.extra_height) = (x, y, w, h);
                lines[n] = LineKind::Shape(s.id.clone());
            }
            Some((Place::Zone(..), n)) => (missing[i], lines[n]) = (true, Ignored),
            None => missing[i] = true,
        }
    }
    let mut unplaced = Unplaced::default();
    for (i, z) in doc.zones.iter_mut().enumerate() {
        match layout.remove(z.id.0.as_str()) {
            Some((Place::Zone(x, y, w, h), _)) => (z.x, z.y, z.width, z.height) = (x, y, w, h),
            Some((Place::Shape(..), n)) => {
                lines[n] = Ignored;
                unplaced.zones.push(i);
            }
            None => unplaced.zones.push(i),
        }
    }
    for &(_, n) in layout.values() {
        (repaired, lines[n]) = (true, Ignored);
    }
    let (dropped, anchor) = resolve(&mut doc, &stated, &mut lines);
    // A shape goes beside the shape it names, or else beside a placed one naming it.
    let mut named_by = vec![None; doc.shapes.len()];
    for (k, a) in anchor.iter().enumerate() {
        if let Some((j, side)) = *a
            && missing[j]
            && !missing[k]
        {
            named_by[j].get_or_insert((k, side.opposite()));
        }
    }
    unplaced.shapes = (0..doc.shapes.len())
        .filter(|&i| missing[i])
        .map(|i| (i, anchor[i].or(named_by[i])))
        .collect();
    repaired |= dropped || unplaced != Unplaced::default();
    Loaded {
        document: doc,
        repaired,
        unplaced,
        lines,
        sides: anchor,
    }
}

/// Links each shape to the one it names and paints every chain the first color one of its
/// shapes states in the text; a link to a missing shape or closing a cycle makes its line
/// invalid. Whether anything had to be dropped or a chain stated two colors, and for each
/// shape the shape its kept link names and the side of it the shape sits on.
#[allow(clippy::type_complexity)]
fn resolve(
    doc: &mut Document,
    stated: &[(usize, Option<(&str, Side)>, Option<Color>)],
    lines: &mut [LineKind],
) -> (bool, Vec<Option<(usize, Side)>>) {
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
    let (mut repaired, mut anchor) = (false, vec![None; stated.len()]);
    for (i, &(n, to, _)) in stated.iter().enumerate() {
        let Some((to, side)) = to else { continue };
        let Some(&j) = index.get(to) else {
            (repaired, lines[n]) = (true, Invalid);
            continue;
        };
        let (a, b) = (find(&mut root, i), find(&mut root, j));
        if a == b {
            (repaired, lines[n]) = (true, Invalid);
            continue;
        }
        root[a] = b;
        anchor[i] = Some((j, side));
        let link = Link(doc.shapes[j].id.clone(), doc.shapes[i].id.clone());
        doc.links.push(link);
    }
    // Each chain's first stated color; stating another is a repair.
    let mut color: Vec<Option<Color>> = vec![None; doc.shapes.len()];
    for (i, &(_, _, c)) in stated.iter().enumerate() {
        let Some(c) = c else { continue };
        let r = find(&mut root, i);
        repaired |= *color[r].get_or_insert(c) != c;
    }
    for (i, shape) in doc.shapes.iter_mut().enumerate() {
        shape.color = color[find(&mut root, i)].unwrap_or_default();
    }
    (repaired, anchor)
}
