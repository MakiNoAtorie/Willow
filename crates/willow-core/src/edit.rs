//! Text editing on rich text, addressed by UTF-8 byte offsets into the plain text
//! (paragraphs joined by `\n`), and the cluster and word boundaries caret motion uses.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::{Paragraph, RichText, Run};

/// One byte's (bold, italic).
type Flags = (bool, bool);

/// A style a toggle turns on or off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Bold,
    Italic,
}

impl Style {
    pub(crate) fn of(self, f: &mut Flags) -> &mut bool {
        match self {
            Style::Bold => &mut f.0,
            Style::Italic => &mut f.1,
        }
    }
}

impl RichText {
    /// The text with paragraphs joined by `\n`: what every offset indexes.
    pub fn plain(&self) -> String {
        let paras: Vec<String> = (self.paragraphs.iter())
            .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect())
            .collect();
        paras.join("\n")
    }

    /// Replaces byte range `r` with `text` (`\n` splits a paragraph); inserted text takes the
    /// style of the text before it. Returns the offset after the inserted text. A range off
    /// the text or off a char boundary changes nothing.
    pub(crate) fn replace(&mut self, r: Range<usize>, text: &str) -> usize {
        // ponytail: rebuilds the whole text per edit (O(len)); fine for shape-sized text.
        let (mut s, mut styles) = self.flatten();
        if r.start > r.end || !s.is_char_boundary(r.start) || !s.is_char_boundary(r.end) {
            return r.start.min(s.len());
        }
        let style = (r.start.checked_sub(1))
            .filter(|&i| s.as_bytes()[i] != b'\n')
            .or((r.end < s.len()).then_some(r.end))
            .map_or_else(Default::default, |i| styles[i]);
        s.replace_range(r.clone(), text);
        styles.splice(r.clone(), std::iter::repeat_n(style, text.len()));
        *self = build(&s, &styles);
        r.start + text.len()
    }

    /// The style typing at `i` takes: that of the text before it, else after it.
    pub(crate) fn style_at(&self, i: usize) -> (bool, bool) {
        let (s, styles) = self.flatten();
        (i.checked_sub(1))
            .filter(|&j| s.as_bytes()[j] != b'\n')
            .or((i < s.len()).then_some(i))
            .map_or_else(Default::default, |j| styles[j])
    }

    /// Whether all of `r` has `style`.
    pub(crate) fn all_styled(&self, r: Range<usize>, style: Style) -> bool {
        let mut styles = self.flatten().1;
        (styles.get_mut(r)).is_some_and(|b| b.iter_mut().all(|f| *style.of(f)))
    }

    /// Turns `style` on or off on `r` and merges runs.
    pub(crate) fn set_style(&mut self, r: Range<usize>, style: Style, on: bool) {
        let (s, mut styles) = self.flatten();
        for f in styles.get_mut(r).into_iter().flatten() {
            *style.of(f) = on;
        }
        *self = build(&s, &styles);
    }

    /// Plain text and one style per byte (`\n` gets a default style).
    fn flatten(&self) -> (String, Vec<Flags>) {
        let mut styles = Vec::new();
        for (i, p) in self.paragraphs.iter().enumerate() {
            if i > 0 {
                styles.push(Flags::default());
            }
            for r in &p.runs {
                styles.extend(std::iter::repeat_n((r.bold, r.italic), r.text.len()));
            }
        }
        (self.plain(), styles)
    }
}

/// Rich text from plain text and per-byte styles: same-style bytes merged into runs, no
/// empty runs, and no paragraphs at all for empty text.
pub(crate) fn build(s: &str, styles: &[(bool, bool)]) -> RichText {
    if s.is_empty() {
        return RichText::default();
    }
    let mut start = 0;
    let paragraphs = s
        .split('\n')
        .map(|p| {
            let mut runs: Vec<Run> = Vec::new();
            for (i, ch) in p.char_indices() {
                let (bold, italic) = styles[start + i];
                match runs.last_mut() {
                    Some(r) if (r.bold, r.italic) == (bold, italic) => r.text.push(ch),
                    _ => runs.push(Run {
                        text: ch.into(),
                        bold,
                        italic,
                    }),
                }
            }
            start += p.len() + 1;
            Paragraph { runs }
        })
        .collect();
    RichText { paragraphs }
}

/// The cluster boundary before `i` (0 at the start).
pub(crate) fn prev_cluster(s: &str, i: usize) -> usize {
    s[..i].grapheme_indices(true).next_back().map_or(0, |g| g.0)
}

/// The cluster boundary after `i` (the end at the end).
pub(crate) fn next_cluster(s: &str, i: usize) -> usize {
    s[i..].graphemes(true).next().map_or(i, |g| i + g.len())
}

fn is_word(w: &str) -> bool {
    w.chars().any(char::is_alphanumeric)
}

/// The start of the word before `i`.
pub(crate) fn word_left(s: &str, i: usize) -> usize {
    (s[..i].split_word_bound_indices().rev())
        .find(|w| is_word(w.1))
        .map_or(0, |w| w.0)
}

/// The end of the word after `i`.
pub(crate) fn word_right(s: &str, i: usize) -> usize {
    (s[i..].split_word_bound_indices())
        .find(|w| is_word(w.1))
        .map_or(s.len(), |w| i + w.0 + w.1.len())
}
