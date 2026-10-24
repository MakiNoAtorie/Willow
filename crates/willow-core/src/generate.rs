//! Generated documents for big-document tests, `--generate N` and the bench: deterministic for
//! a seed. Trunks of shapes with side chains beside them, prose with some bold and italic, all
//! seven colors, act zones with nested zones.

use crate::{
    Color, DEFAULT_WIDTH, Document, Link, Paragraph, RichText, Run, Shape, ShapeId, Zone, id_for,
};

/// Shapes per trunk.
const TRUNK_LEN: i32 = 40;
/// Column pitch, and row pitch on trunks and on side chains, in canvas units.
const COL: i32 = 290;
const TRUNK_ROW: i32 = 170;
const SIDE_ROW: i32 = 120;
/// A trunk group: two side columns either side of the trunk, laid out in rows of groups.
const GROUP_W: i32 = 6 * COL;
const GROUP_H: i32 = TRUNK_LEN * TRUNK_ROW + ACT_GAP + 1000;
const GROUPS_PER_ROW: usize = 10;
/// Trunk rows per act, extra space between acts, and an act zone's padding around its columns.
const ACT_ROWS: i32 = TRUNK_LEN / 2;
const ACT_GAP: i32 = 120;
const ACT_PAD: i32 = 56;
/// A nested zone's padding around its side chain, room for its title, and a guess at a side
/// shape's height (the generator can't lay out text).
const NESTED_PAD: i32 = 28;
const NESTED_TOP: i32 = 48;
const SIDE_H: i32 = 80;
/// Side columns relative to the trunk column.
const SIDES: [i32; 4] = [-2, -1, 1, 2];

const WORDS: [&str; 48] = [
    "the",
    "tide",
    "turns",
    "and",
    "nothing",
    "is",
    "same",
    "again",
    "a",
    "letter",
    "arrives",
    "Mara",
    "climbs",
    "stair",
    "bell",
    "rings",
    "twice",
    "rain",
    "on",
    "lamp",
    "glass",
    "stranger",
    "at",
    "quay",
    "ferry",
    "late",
    "smoke",
    "headland",
    "gull",
    "rock",
    "keeper",
    "light",
    "storm",
    "harbour",
    "remembers",
    "everything",
    "quiet",
    "boat",
    "of",
    "salt",
    "wind",
    "night",
    "she",
    "waits",
    "for",
    "him",
    "under",
    "cliff",
];

/// xorshift64: tiny, fast and plenty for layout noise.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        let x = &mut self.0;
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        (*x % n as u64) as usize
    }

    fn between(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }
}

/// A document of `n` shapes generated from `seed`. Each trunk runs through two act zones;
/// each act nests one zone around a side chain. About half the side chains next to the trunk
/// link to it and take its color.
pub fn generate(n: usize, seed: u64) -> Document {
    let mut rng = Rng((seed ^ 0x9e37_79b9_7f4a_7c15).max(1));
    let mut doc = Document::default();
    // Shapes and zones share one ID sequence.
    let mut next = 0;
    let mut fresh = || {
        next += 1;
        ShapeId(id_for(next - 1))
    };
    let mut group = 0;
    while doc.shapes.len() < n {
        let gx = (group % GROUPS_PER_ROW) as i32 * GROUP_W;
        let gy = (group / GROUPS_PER_ROW) as i32 * GROUP_H;
        let trunk_color = Color::ALL[group % 7];
        let mut prev: Option<ShapeId> = None;
        for act in 0..2 {
            let rows = act * ACT_ROWS..(act + 1) * ACT_ROWS;
            let top = gy + rows.start * TRUNK_ROW + act * ACT_GAP;
            let bottom = top + ACT_ROWS * TRUNK_ROW;
            let mut zone = Zone {
                id: fresh(),
                title: format!("Part {} · Act {}", group + 1, act + 1),
                color: Color::ALL[rng.below(7)],
                x: gx + SIDES[0] * COL - ACT_PAD,
                y: top - ACT_PAD,
                width: (4 * COL + DEFAULT_WIDTH as i32 + 2 * ACT_PAD) as u32,
                height: (bottom - top + ACT_PAD) as u32,
                members: Vec::new(),
            };
            let mut nested: Option<Zone> = None;
            // Next free top in each side column.
            let mut free = [top; 4];
            for row in rows {
                let y = top + (row % ACT_ROWS) * TRUNK_ROW;
                let text = prose(&mut rng, 6, 18);
                let Some(t) = push(&mut doc, n, &mut fresh, gx, y, text, trunk_color) else {
                    break;
                };
                zone.members.push(t.clone());
                if let Some(p) = prev.replace(t.clone()) {
                    doc.links.push(Link(p, t.clone()));
                }
                if rng.below(3) > 0 {
                    continue;
                }
                let side = rng.below(4);
                let (len, mut color) = (rng.between(2, 5) as i32, Color::ALL[rng.below(7)]);
                let linked = SIDES[side].abs() == 1 && rng.below(2) == 0;
                let start = y.max(free[side]);
                if start + len * SIDE_ROW > bottom {
                    continue;
                }
                if linked {
                    color = trunk_color;
                }
                let x = gx + SIDES[side] * COL;
                let in_nested = nested.is_none();
                if in_nested {
                    let id = fresh();
                    zone.members.push(id.clone());
                    nested = Some(Zone {
                        id,
                        title: prose(&mut rng, 2, 3).plain(),
                        color: Color::ALL[rng.below(7)],
                        x: x - NESTED_PAD,
                        y: start - NESTED_TOP,
                        width: (DEFAULT_WIDTH as i32 + 2 * NESTED_PAD) as u32,
                        height: ((len - 1) * SIDE_ROW + SIDE_H + NESTED_PAD + NESTED_TOP) as u32,
                        members: Vec::new(),
                    });
                }
                let mut back = linked.then(|| t.clone());
                for k in 0..len {
                    let text = prose(&mut rng, 2, 6);
                    let y = start + k * SIDE_ROW;
                    let Some(s) = push(&mut doc, n, &mut fresh, x, y, text, color) else {
                        break;
                    };
                    match &mut nested {
                        Some(z) if in_nested => z.members.push(s.clone()),
                        _ => zone.members.push(s.clone()),
                    }
                    if let Some(b) = back.replace(s.clone()) {
                        doc.links.push(Link(b, s));
                    }
                }
                free[side] = start + len * SIDE_ROW + SIDE_ROW / 2;
            }
            // Parents before the zones nested in them.
            doc.zones.extend([Some(zone), nested].into_iter().flatten());
        }
        group += 1;
    }
    doc
}

/// Pushes a shape at (x, y) unless the document already has `n`; its ID.
fn push(
    doc: &mut Document,
    n: usize,
    fresh: &mut impl FnMut() -> ShapeId,
    x: i32,
    y: i32,
    text: RichText,
    color: Color,
) -> Option<ShapeId> {
    if doc.shapes.len() >= n {
        return None;
    }
    let mut s = Shape::new(fresh(), x, y);
    (s.text, s.color) = (text, color);
    let id = s.id.clone();
    doc.shapes.push(s);
    Some(id)
}

/// One sentence of `lo..=hi` words, about one word in six bold or italic.
fn prose(rng: &mut Rng, lo: usize, hi: usize) -> RichText {
    let count = rng.between(lo, hi);
    let mut runs: Vec<Run> = Vec::new();
    for i in 0..count {
        let mut word = WORDS[rng.below(WORDS.len())].to_string();
        if i == 0 {
            word[..1].make_ascii_uppercase();
        } else {
            // The space joins whichever run precedes it.
            runs.last_mut().expect("first word pushed").text.push(' ');
        }
        if i + 1 == count {
            word.push('.');
        }
        let (bold, italic) = match rng.below(12) {
            0 => (true, false),
            1 => (false, true),
            _ => (false, false),
        };
        match runs.last_mut() {
            Some(r) if (r.bold, r.italic) == (bold, italic) => r.text.push_str(&word),
            _ => runs.push(Run {
                text: word,
                bold,
                italic,
            }),
        }
    }
    RichText {
        paragraphs: vec![Paragraph { runs }],
    }
}
