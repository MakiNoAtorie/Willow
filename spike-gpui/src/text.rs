//! Justified rich-text layout built on GPUI's single-line shaper.
//!
//! GPUI only offers left/center/right alignment, so each paragraph is shaped
//! once as one unwrapped line (`TextSystem::layout_line`), broken greedily at
//! spaces using the shaped glyph x positions, and every non-final line has
//! its leftover width spread across its inner spaces. Glyphs are painted one
//! by one with `paint_glyph`. Caret stops and hit-testing come from the same
//! positioned-glyph table, so editing and drawing can't disagree.

use gpui::{
    Font, FontId, FontStyle, FontWeight, GlyphId, Hsla, Pixels, Point, TextRun, Window, WindowTextSystem,
    point, px,
};
use unicode_segmentation::UnicodeSegmentation;

pub const FONT_FAMILY: &str = "IBM Plex Sans";
pub const FONT_SIZE: f32 = 14.0;
pub const LINE_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
}

pub fn font_for(style: Style) -> Font {
    let mut f = gpui::font(FONT_FAMILY);
    if style.bold {
        f.weight = FontWeight::SEMIBOLD;
    }
    if style.italic {
        f.style = FontStyle::Italic;
    }
    f
}

/// One painted glyph, in layout units (shape-local, unzoomed).
#[derive(Clone, Debug)]
pub struct Glyph {
    pub font_id: FontId,
    pub id: GlyphId,
    pub pos: Point<f32>, // baseline origin
    pub emoji: bool,
}

/// A caret position: byte offset and where the caret goes.
#[derive(Clone, Copy, Debug)]
pub struct Stop {
    pub index: usize,
    pub x: f32,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub top: f32,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub glyphs: Vec<Glyph>,
    pub stops: Vec<Stop>, // sorted by index, then line
    pub lines: Vec<Line>,
    pub height: f32,
}

impl Layout {
    /// Caret stop for a byte offset. A line break has two stops with the same
    /// index; the later one (start of the next line) wins.
    pub fn stop_for(&self, index: usize) -> Stop {
        let i = self.stops.partition_point(|s| s.index <= index);
        self.stops[i.saturating_sub(1)]
    }

    pub fn closest(&self, p: Point<f32>) -> usize {
        let line = ((p.y / LINE_HEIGHT).floor().max(0.0) as usize).min(self.lines.len() - 1);
        self.closest_on_line(line, p.x)
    }

    pub fn closest_on_line(&self, line: usize, x: f32) -> usize {
        self.stops
            .iter()
            .filter(|s| s.line == line)
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map(|s| s.index)
            .unwrap_or(0)
    }

    pub fn paint(&self, origin: Point<Pixels>, zoom: f32, color: Hsla, window: &mut Window) {
        // ponytail: font size follows zoom continuously, so the glyph atlas gets
        // an entry per size seen. Quantizing to 0.5 px keeps that bounded.
        let size = px((FONT_SIZE * zoom * 2.0).round() / 2.0);
        for g in &self.glyphs {
            let o = origin + point(px(g.pos.x * zoom), px(g.pos.y * zoom));
            let _ = if g.emoji {
                window.paint_emoji(o, g.font_id, g.id, size)
            } else {
                window.paint_glyph(o, g.font_id, g.id, size, color)
            };
        }
    }
}

/// Lays `text` out justified to `width`. `styles` holds one entry per byte.
/// '\n' ends a paragraph; the last line of each paragraph is left-aligned.
pub fn layout(text: &str, styles: &[Style], width: f32, ts: &WindowTextSystem) -> Layout {
    let mut out = Layout::default();
    let mut para_start = 0;
    for para in text.split('\n') {
        layout_paragraph(para, para_start, styles, width, ts, &mut out);
        para_start += para.len() + 1;
    }
    out.height = out.lines.len() as f32 * LINE_HEIGHT;
    out
}

fn runs_for(text: &str, base: usize, styles: &[Style]) -> Vec<TextRun> {
    let mut runs: Vec<(usize, Style)> = Vec::new();
    for (i, c) in text.char_indices() {
        let s = styles.get(base + i).copied().unwrap_or_default();
        match runs.last_mut() {
            Some((len, last)) if *last == s => *len += c.len_utf8(),
            _ => runs.push((c.len_utf8(), s)),
        }
    }
    runs.into_iter()
        .map(|(len, s)| TextRun {
            len,
            font: font_for(s),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect()
}

fn layout_paragraph(
    para: &str,
    base: usize,
    styles: &[Style],
    width: f32,
    ts: &WindowTextSystem,
    out: &mut Layout,
) {
    let line_no = |out: &Layout| out.lines.len();
    if para.is_empty() {
        let top = line_no(out) as f32 * LINE_HEIGHT;
        out.stops.push(Stop { index: base, x: 0.0, line: line_no(out) });
        out.lines.push(Line { top, start: base, end: base });
        return;
    }
    let shaped = ts.layout_line(para, px(FONT_SIZE), &runs_for(para, base, styles), None);
    let ascent = f32::from(shaped.ascent);
    let descent = f32::from(shaped.descent);
    let baseline_off = (LINE_HEIGHT - (ascent + descent)) / 2.0 + ascent;

    // Flatten glyphs; x_at[i] = x where the char at byte i starts.
    let mut flat: Vec<(usize, f32, FontId, GlyphId, bool)> = Vec::new();
    for run in &shaped.runs {
        for g in &run.glyphs {
            flat.push((g.index, f32::from(g.position.x), run.font_id, g.id, g.is_emoji));
        }
    }
    flat.sort_by_key(|g| g.0);
    // x_by[i]: x where byte i starts (ligature tails take the next glyph's x).
    let mut x_by = vec![f32::NAN; para.len() + 1];
    x_by[para.len()] = f32::from(shaped.width);
    for g in &flat {
        x_by[g.0] = g.1;
    }
    for i in (0..para.len()).rev() {
        if x_by[i].is_nan() {
            x_by[i] = x_by[i + 1];
        }
    }
    let x_at = |i: usize| x_by[i];
    // spaces_before[i]: count of ' ' in para[..i]
    let mut spaces_before = vec![0usize; para.len() + 1];
    for (i, b) in para.bytes().enumerate() {
        spaces_before[i + 1] = spaces_before[i] + (b == b' ') as usize;
    }

    // Greedy breaking at spaces; a word longer than the line breaks at a grapheme.
    let bytes = para.as_bytes();
    let mut lines: Vec<(usize, usize)> = Vec::new(); // [start, end) incl. trailing space
    let mut start = 0;
    while start < para.len() {
        let x0 = x_at(start);
        let mut end = para.len();
        let mut last_space = None;
        for (i, g) in para[start..].grapheme_indices(true).map(|(i, g)| (i + start, g)) {
            let next = i + g.len();
            if x_at(next) - x0 > width && g != " " {
                end = match last_space {
                    Some(s) => s + 1,
                    None if i > start => i,
                    None => next,
                };
                break;
            }
            if g == " " {
                last_space = Some(i);
            }
        }
        lines.push((start, end));
        start = end;
    }

    let n = lines.len();
    for (li, &(s, e)) in lines.iter().enumerate() {
        let line = line_no(out);
        let top = line as f32 * LINE_HEIGHT;
        let x0 = x_at(s);
        // Trailing space doesn't count toward the line's width.
        let content_end = if e > s && bytes[e - 1] == b' ' { e - 1 } else { e };
        let natural = x_at(content_end) - x0;
        let gaps = spaces_before[content_end] - spaces_before[s];
        let justify = li + 1 < n && gaps > 0;
        let per_gap = if justify { (width - natural) / gaps as f32 } else { 0.0 };

        let shifted = |i: usize| -> f32 {
            let before = spaces_before[i.min(content_end)] - spaces_before[s];
            x_at(i) - x0 + per_gap * before as f32
        };
        for g in flat.iter().filter(|g| g.0 >= s && g.0 < e) {
            let x = shifted(g.0);
            out.glyphs.push(Glyph {
                font_id: g.2,
                id: g.3,
                pos: point(x, top + baseline_off),
                emoji: g.4,
            });
        }
        // One stop per grapheme, so the caret never lands inside a cluster.
        for (i, _) in para[s..e].grapheme_indices(true).map(|(i, g)| (i + s, g)) {
            out.stops.push(Stop { index: base + i, x: shifted(i), line });
        }
        // End-of-line stop: after the last char, past any trailing space.
        // The next line's first stop has the same index.
        let end_x = if justify { width } else { x_at(e) - x0 };
        out.stops.push(Stop { index: base + e, x: end_x, line });
        out.lines.push(Line { top, start: base + s, end: base + e });
    }
}
