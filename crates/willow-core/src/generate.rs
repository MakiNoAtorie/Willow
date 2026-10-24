//! Generated documents for big-document tests, `--generate N` and the bench: deterministic for
//! a seed. Trunks of shapes with side chains beside them, prose with some bold and italic, all
//! seven colors.

use crate::{Color, Document, Paragraph, RichText, Run, Shape, ShapeId, id_for};

/// Shapes per trunk.
const TRUNK_LEN: i32 = 40;
/// Column pitch, and row pitch on trunks and on side chains, in canvas units.
const COL: i32 = 290;
const TRUNK_ROW: i32 = 170;
const SIDE_ROW: i32 = 120;
/// A trunk group: two side columns either side of the trunk, laid out in rows of groups.
const GROUP_W: i32 = 6 * COL;
const GROUP_H: i32 = TRUNK_LEN * TRUNK_ROW + 1000;
const GROUPS_PER_ROW: usize = 10;
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

/// A document of `n` shapes generated from `seed`.
pub fn generate(n: usize, seed: u64) -> Document {
    let mut rng = Rng((seed ^ 0x9e37_79b9_7f4a_7c15).max(1));
    let mut shapes = Vec::with_capacity(n);
    let push = |shapes: &mut Vec<Shape>, x, y, text, color| {
        if shapes.len() < n {
            let mut s = Shape::new(ShapeId(id_for(shapes.len())), x, y);
            (s.text, s.color) = (text, color);
            shapes.push(s);
        }
    };
    let mut group = 0;
    while shapes.len() < n {
        let gx = (group % GROUPS_PER_ROW) as i32 * GROUP_W;
        let gy = (group / GROUPS_PER_ROW) as i32 * GROUP_H;
        let trunk = Color::ALL[group % 7];
        // Next free top in each side column.
        let mut free = [0; 4];
        for row in 0..TRUNK_LEN {
            let y = row * TRUNK_ROW;
            let text = prose(&mut rng, 6, 18);
            push(&mut shapes, gx, gy + y, text, trunk);
            if rng.below(3) > 0 {
                continue;
            }
            let side = rng.below(4);
            let (len, color) = (rng.between(2, 5) as i32, Color::ALL[rng.below(7)]);
            let start = y.max(free[side]);
            for k in 0..len {
                let text = prose(&mut rng, 2, 6);
                push(
                    &mut shapes,
                    gx + SIDES[side] * COL,
                    gy + start + k * SIDE_ROW,
                    text,
                    color,
                );
            }
            free[side] = start + len * SIDE_ROW + SIDE_ROW / 2;
        }
        group += 1;
    }
    Document { shapes }
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
