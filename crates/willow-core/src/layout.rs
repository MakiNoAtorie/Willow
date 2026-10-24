//! Shape text layout (ADR 0003): line breaking, justification, caret stops and point-to-caret.
//! The app measures; the core decides where every cluster goes.
//!
//! Offsets are UTF-8 bytes into the shape's plain text: paragraphs joined by `\n`
//! (`RichText::plain`). Positions are canvas units from the text's top-left (inside the padding).

use std::ops::Range;

use unicode_linebreak::{BreakClass, break_property, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use crate::{Paragraph, Point, RichText, Theme};

/// The app's text-measuring function: the only adapter between core and app.
pub trait Measure {
    /// Advance of each grapheme cluster of `para`, given as byte ranges into the paragraph's
    /// text, with the whole paragraph shaped as one line in the theme's font and size.
    fn advances(&self, para: &Paragraph, clusters: &[Range<usize>], theme: &Theme) -> Vec<f32>;
}

/// One grapheme cluster where it is drawn. Justified spaces are wider than measured.
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    pub range: Range<usize>,
    pub x: f32,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The paragraph's index and where its text starts.
    pub para: usize,
    pub para_start: usize,
    /// Byte range, trailing spaces included.
    pub range: Range<usize>,
    /// Top of the line; the line is `Theme::line_height()` tall.
    pub y: f32,
    pub clusters: Vec<Cluster>,
    /// The paragraph goes on in the next line (a soft wrap): this line's end and the next
    /// line's start are the same offset.
    pub wraps: bool,
    /// Caret x at the line's end.
    pub end_x: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    /// Text height: at least one line, even for empty text.
    pub height: f32,
    /// The width it was laid out in.
    pub width: f32,
}

/// A caret position: a byte offset at a cluster boundary. At a soft wrap the line's end and
/// the next line's start are the same offset; `upstream` shows it at the end of the first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    pub offset: usize,
    pub upstream: bool,
}

impl Line {
    /// Caret stops with their x: each cluster's start, then the line's end.
    fn stops(&self) -> impl Iterator<Item = (Caret, f32)> + '_ {
        let at = |offset, upstream| Caret { offset, upstream };
        (self
            .clusters
            .iter()
            .map(move |c| (at(c.range.start, false), c.x)))
        .chain([(at(self.range.end, self.wraps), self.end_x)])
    }

    /// The stop nearest `x`; the later one on a tie, so past a trailing space is its end.
    fn nearest(&self, x: f32) -> Caret {
        let mut best = (Caret::default(), f32::INFINITY);
        for (c, sx) in self.stops() {
            if (sx - x).abs() <= best.1 {
                best = (c, (sx - x).abs());
            }
        }
        best.0
    }
}

impl Layout {
    /// Index of the line caret `c` shows on.
    pub fn line_of(&self, c: Caret) -> usize {
        let i = (self.lines)
            .partition_point(|l| l.range.start <= c.offset)
            .saturating_sub(1);
        let wrap_end = i > 0 && self.lines[i - 1].wraps && self.lines[i - 1].range.end == c.offset;
        if c.upstream && wrap_end { i - 1 } else { i }
    }

    /// Point-to-caret: the stop nearest `p` (text-local) on the line under it.
    pub fn caret_at(&self, p: Point) -> Caret {
        let i = self.lines.iter().rposition(|l| l.y <= p.y).unwrap_or(0);
        self.lines[i].nearest(p.x)
    }

    /// Top of caret `c`, text-local.
    pub fn caret_point(&self, c: Caret) -> Point {
        let line = &self.lines[self.line_of(c)];
        let x = (line.stops())
            .find(|(s, _)| s.offset == c.offset)
            .map_or(line.end_x, |s| s.1);
        Point { x, y: line.y }
    }

    /// The caret one line down (or up) nearest column `goal_x`; past the last line it is the
    /// text's end, before the first the start.
    pub fn vertical(&self, c: Caret, goal_x: f32, down: bool) -> Caret {
        let i = self.line_of(c);
        let j = if down { i + 1 } else { i.wrapping_sub(1) };
        match self.lines.get(j) {
            Some(line) => line.nearest(goal_x),
            None if down => Caret {
                offset: self.lines[i..].last().map_or(0, |l| l.range.end),
                upstream: false,
            },
            None => Caret::default(),
        }
    }

    /// Text-local (top-left, bottom-right) of byte range `r` on each line it touches, each
    /// `line_height` tall. A range going on past a line's end (its paragraph break, or across
    /// a soft wrap) reaches the right edge; one ending at a soft wrap stops at that line's end.
    pub fn selection_rects(&self, r: Range<usize>, line_height: f32) -> Vec<(Point, Point)> {
        let x_of = |l: &Line, i: usize| {
            (l.clusters.iter())
                .find(|c| c.range.start == i)
                .map_or(l.end_x, |c| c.x)
        };
        (self.lines.iter())
            .filter_map(|l| {
                let (lo, hi) = (r.start.max(l.range.start), r.end.min(l.range.end));
                let on = r.end > l.range.end;
                // At a wrap the line's end is the next line's start: the range begins there.
                let starts_after = l.wraps && r.start >= l.range.end;
                if lo > hi || starts_after || (lo == hi && !on) {
                    return None;
                }
                let x1 = if on { self.width } else { x_of(l, hi) };
                let top_left = Point {
                    x: x_of(l, lo),
                    y: l.y,
                };
                Some((
                    top_left,
                    Point {
                        x: x1,
                        y: l.y + line_height,
                    },
                ))
            })
            .collect()
    }

    /// The start and end of caret `c`'s line.
    pub fn line_bounds(&self, c: Caret) -> (Caret, Caret) {
        let line = &self.lines[self.line_of(c)];
        let at = |offset, upstream| Caret { offset, upstream };
        (at(line.range.start, false), at(line.range.end, line.wraps))
    }
}

/// Slack for float sums when comparing against the width.
const EPS: f32 = 0.01;

/// Ideographs, kana and Hangul: justification may open a gap at a break next to one.
fn cjk(s: &str) -> bool {
    use BreakClass::*;
    s.chars().next().is_some_and(|c| {
        matches!(
            break_property(c as u32),
            Ideographic
                | ConditionalJapaneseStarter
                | HangulLvSyllable
                | HangulLvtSyllable
                | HangulLJamo
                | HangulVJamo
                | HangulTJamo
        )
    })
}

/// Lays `text` out in `width` canvas units: greedy breaks at UAX #14 opportunities (so CJK
/// breaks between characters, with kinsoku), between clusters inside a word wider than the
/// line; non-last lines of a paragraph justified over spaces and CJK break gaps.
pub fn layout(text: &RichText, width: f32, theme: &Theme, m: &dyn Measure) -> Layout {
    let lh = theme.line_height();
    let empty = [Paragraph::default()];
    let paras = match text.paragraphs.as_slice() {
        [] => &empty[..],
        p => p,
    };
    let mut lines = Vec::new();
    let mut para_start = 0;
    for (p, para) in paras.iter().enumerate() {
        let s: String = para.runs.iter().map(|r| r.text.as_str()).collect();
        let ranges: Vec<Range<usize>> = s
            .grapheme_indices(true)
            .map(|(i, g)| i..i + g.len())
            .collect();
        let adv = m.advances(para, &ranges, theme);
        let space: Vec<bool> = ranges
            .iter()
            .map(|r| s[r.clone()].chars().all(char::is_whitespace))
            .collect();
        let wide: Vec<bool> = ranges.iter().map(|r| cjk(&s[r.clone()])).collect();
        // starts[i]: a line may start at cluster i.
        let mut starts = vec![false; ranges.len()];
        let mut i = 0;
        for (b, _) in linebreaks(&s) {
            while i < ranges.len() && ranges[i].start < b {
                i += 1;
            }
            if i < ranges.len() && ranges[i].start == b {
                starts[i] = true;
            }
        }
        let n = ranges.len();
        let mut a = 0;
        loop {
            let (mut run, mut brk, mut end) = (0.0, None, n);
            for i in a..n {
                if i > a && starts[i] {
                    brk = Some(i);
                }
                run += adv[i];
                if i > a && !space[i] && run > width + EPS {
                    end = brk.unwrap_or(i);
                    break;
                }
            }
            // Spaces at the line's end hang: they neither count nor stretch.
            let ink_end = (a..end).rev().find(|&i| !space[i]).map_or(a, |i| i + 1);
            // Cluster i stretches: a space, or the left side of a CJK break gap.
            let stretch = |i: usize| {
                space[i]
                    || (i + 1 < ink_end
                        && starts[i + 1]
                        && !space[i + 1]
                        && (wide[i] || wide[i + 1]))
            };
            let gaps = (a..ink_end).filter(|&i| stretch(i)).count();
            let natural: f32 = adv[a..ink_end].iter().sum();
            let extra = if end < n && gaps > 0 {
                ((width - natural) / gaps as f32).max(0.0)
            } else {
                0.0
            };
            let mut x = 0.0;
            let clusters: Vec<Cluster> = (a..end)
                .map(|i| {
                    let w = adv[i]
                        + if i < ink_end && stretch(i) {
                            extra
                        } else {
                            0.0
                        };
                    let c = Cluster {
                        range: para_start + ranges[i].start..para_start + ranges[i].end,
                        x,
                        width: w,
                    };
                    x += w;
                    c
                })
                .collect();
            let range = match (clusters.first(), clusters.last()) {
                (Some(f), Some(l)) => f.range.start..l.range.end,
                _ => para_start..para_start,
            };
            lines.push(Line {
                para: p,
                para_start,
                range,
                y: lines.len() as f32 * lh,
                clusters,
                wraps: end < n,
                end_x: x.min(width),
            });
            a = end;
            if a >= n {
                break;
            }
        }
        para_start += s.len() + 1;
    }
    Layout {
        height: lines.len() as f32 * lh,
        lines,
        width,
    }
}
