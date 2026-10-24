//! The `.willow` text format: `save` writes canonical text, `load` reads it back.

use std::collections::HashMap;
use std::fmt::{self, Write};

use crate::edit::build;
use crate::{Document, RichText, Shape, ShapeId};

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
/// Lines it can't read are dropped, and shapes without layout keep the defaults; both count
/// as repairs.
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
    let (mut shapes, mut layout, mut repaired) = (Vec::new(), HashMap::new(), false);
    for line in lines.map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = line.strip_prefix("@layout ") {
            match parse_layout(rest) {
                Some((id, l)) => _ = layout.insert(id, l),
                None => repaired = true,
            }
        } else if let Some((id, text)) = parse_shape(line) {
            let mut s = Shape::new(ShapeId(id.into()), 0, 0);
            s.text = text;
            shapes.push(s);
        } else {
            repaired = true;
        }
    }
    for s in &mut shapes {
        match layout.remove(s.id.0.as_str()) {
            Some((x, y, w, h)) => (s.x, s.y, s.width, s.extra_height) = (x, y, w, h),
            None => repaired = true,
        }
    }
    let document = Document {
        shapes,
        ..Document::default()
    };
    Ok(Loaded {
        document,
        repaired: repaired || !layout.is_empty(),
    })
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase())
}

/// A shape's x, y, width and extra height.
type Place = (i32, i32, u32, u32);

/// `id x,y wW [hH]`.
fn parse_layout(rest: &str) -> Option<(&str, Place)> {
    let mut t = rest.split(' ');
    let id = t.next().filter(|id| is_id(id))?;
    let (x, y) = t.next()?.split_once(',')?;
    let w = t.next()?.strip_prefix('w')?.parse().ok()?;
    let h = match t.next() {
        Some(h) => h.strip_prefix('h')?.parse().ok()?,
        None => 0,
    };
    let l = (x.parse().ok()?, y.parse().ok()?, w, h);
    t.next().is_none().then_some((id, l))
}

/// `id: "text"`; whatever follows the closing quote is not read yet.
fn parse_shape(line: &str) -> Option<(&str, RichText)> {
    let (id, rest) = line.split_once(": \"")?;
    if !is_id(id) {
        return None;
    }
    let (mut plain, mut styles, mut style) = (String::new(), Vec::new(), (false, false));
    let mut chars = rest.chars();
    loop {
        let ch = match chars.next()? {
            '"' => return Some((id, build(&plain, &styles))),
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

/// Canonical text for `doc`: header, one block per chain in story order, then the layout.
pub fn save(doc: &Document) -> String {
    let mut shapes: Vec<&Shape> = doc.shapes.iter().collect();
    // Story order: by topmost shape, ties by x, then ID in generation order (a … z, aa).
    shapes.sort_by_key(|s| (s.y, s.x, s.id.0.len(), &s.id.0));
    let mut out = String::from("willow 1\n");
    for s in &shapes {
        // Every shape is a chain of one until links arrive; chains are top-level blocks.
        out.push('\n');
        let _ = writeln!(out, "{}: \"{}\"", s.id.0, text(&s.text));
    }
    if !shapes.is_empty() {
        out.push('\n');
    }
    for s in &shapes {
        let _ = write!(out, "@layout {} {},{} w{}", s.id.0, s.x, s.y, s.width);
        if s.extra_height > 0 {
            let _ = write!(out, " h{}", s.extra_height);
        }
        out.push('\n');
    }
    out
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
            for ch in text.chars() {
                if matches!(ch, '"' | '*' | '\\') {
                    out.push('\\');
                }
                out.push(ch);
            }
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
