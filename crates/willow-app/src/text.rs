//! Shape text on the GPUI side: the text-measuring function the core lays out with, glyphs
//! painted one by one where the core put their clusters, and the platform text input adapter.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    Bounds, Context, EntityInputHandler, Font, FontStyle, FontWeight, LineLayout, Pixels, TextRun,
    UTF16Selection, Window, WindowTextSystem, point, px, size,
};
use willow_core::{Input, Layout, Measure, Paragraph, RichText, Run, Theme};

use crate::{Willow, color};

fn font(t: &Theme, bold: bool, italic: bool) -> Font {
    let mut f = gpui::font(t.font);
    if bold {
        f.weight = FontWeight::BOLD;
    }
    if italic {
        f.style = FontStyle::Italic;
    }
    f
}

/// `para` shaped as one unwrapped line at the theme's (unzoomed) size.
fn shape(ts: &WindowTextSystem, para: &Paragraph, t: &Theme) -> Arc<LineLayout> {
    let text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
    let runs: Vec<TextRun> = (para.runs.iter())
        .filter(|r| !r.text.is_empty())
        .map(|r| TextRun {
            len: r.text.len(),
            font: font(t, r.bold, r.italic),
            color: color(t.ink),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect();
    ts.layout_line(&text, px(t.font_size), &runs, None)
}

/// x at every byte offset of the shaped line (`len + 1` entries). Offsets inside a glyph
/// cluster (ligature tails, multi-byte chars) take the next glyph's x.
fn x_by_byte(line: &LineLayout) -> Vec<f32> {
    let mut x = vec![f32::NAN; line.len + 1];
    x[line.len] = line.width.into();
    for g in line.runs.iter().flat_map(|r| &r.glyphs) {
        let gx: f32 = g.position.x.into();
        // The first glyph of a cluster has the cluster's x; marks after it don't move it left.
        if x[g.index].is_nan() || gx < x[g.index] {
            x[g.index] = gx;
        }
    }
    for i in (0..line.len).rev() {
        if x[i].is_nan() {
            x[i] = x[i + 1];
        }
    }
    x
}

/// The core's text-measuring function, backed by the window's text system. Shapes each word
/// (and each run of spaces) of a style run on its own and keeps it, so a paragraph of known
/// words costs no shaping: the first save after opening a big file measures every shape.
// ponytail: the word cache never shrinks; bound it if a session's vocabulary ever shows in RSS.
pub(crate) struct TextMeasure {
    ts: Arc<WindowTextSystem>,
    words: RefCell<Words>,
}

/// x at every byte of a word shaped alone, per style (`bold as usize * 2 + italic`), for one
/// font and size.
#[derive(Default)]
struct Words {
    font: (&'static str, f32),
    by_style: [HashMap<String, Box<[f32]>>; 4],
}

impl TextMeasure {
    pub(crate) fn new(ts: Arc<WindowTextSystem>) -> Self {
        let words = RefCell::default();
        TextMeasure { ts, words }
    }
}

impl Measure for TextMeasure {
    fn advances(&self, para: &Paragraph, clusters: &[Range<usize>], t: &Theme) -> Vec<f32> {
        let mut words = self.words.borrow_mut();
        if words.font != (t.font, t.font_size) {
            *words = Words {
                font: (t.font, t.font_size),
                ..Default::default()
            };
        }
        // x at every byte of the paragraph: its words' x, end to end.
        let len: usize = para.runs.iter().map(|r| r.text.len()).sum();
        let mut x = Vec::with_capacity(len + 1);
        x.push(0.0);
        for r in &para.runs {
            let words = &mut words.by_style[r.bold as usize * 2 + r.italic as usize];
            let mut rest = r.text.as_str();
            while let Some(c) = rest.chars().next() {
                let len = (rest.find(|d: char| (d == ' ') != (c == ' '))).unwrap_or(rest.len());
                let word = &rest[..len];
                rest = &rest[len..];
                if !words.contains_key(word) {
                    let (text, bold, italic) = (word.into(), r.bold, r.italic);
                    let runs = vec![Run { text, bold, italic }];
                    let wx = x_by_byte(&shape(&self.ts, &Paragraph { runs }, t));
                    words.insert(word.into(), wx.into());
                }
                let (start, wx) = (*x.last().unwrap(), &words[word]);
                x.extend(wx[1..].iter().map(|wx| start + wx));
            }
        }
        clusters.iter().map(|c| x[c.end] - x[c.start]).collect()
    }
}

/// Paints `text` laid out as `layout`, with the text's top-left at `origin` (screen), scaled
/// by zoom `z`, in ink at `opacity`. Each glyph keeps its offset inside its cluster; the cluster goes where the
/// core placed it.
pub(crate) fn paint_text(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    text: &RichText,
    layout: &Layout,
    t: &Theme,
    z: f32,
    opacity: f32,
) {
    let ts = window.text_system().clone();
    // Quantised to half pixels to bound glyph atlas entries across zoom levels.
    let font_size = px((t.font_size * z * 2.0).round() / 2.0);
    let ink = color(t.ink).opacity(opacity);
    let mut shaped: Option<(usize, Arc<LineLayout>, Vec<f32>)> = None;
    for line in &layout.lines {
        let Some(para) = text.paragraphs.get(line.para) else {
            continue;
        };
        if shaped.as_ref().is_none_or(|s| s.0 != line.para) {
            let l = shape(&ts, para, t);
            let x = x_by_byte(&l);
            shaped = Some((line.para, l, x));
        }
        let Some((_, l, x_by)) = &shaped else {
            continue;
        };
        let (a, d): (f32, f32) = (l.ascent.into(), l.descent.into());
        let baseline = line.y + (t.line_height() - (a + d)) / 2.0 + a;
        for run in &l.runs {
            for g in &run.glyphs {
                let at = line.para_start + g.index;
                if !line.range.contains(&at) {
                    continue;
                }
                let i = (line.clusters)
                    .partition_point(|c| c.range.start <= at)
                    .saturating_sub(1);
                let c = &line.clusters[i];
                let gx: f32 = g.position.x.into();
                let x = c.x + gx - x_by[c.range.start - line.para_start];
                let o = origin + point(px(x * z), px(baseline * z));
                // A missing glyph only loses that glyph; nothing to recover.
                let _ = if g.is_emoji {
                    window.paint_emoji(o, run.font_id, g.id, font_size)
                } else {
                    window.paint_glyph(o, run.font_id, g.id, font_size, ink)
                };
            }
        }
    }
}

/// Paints one line of plain `text` in the theme font, `bold` or regular, at `size` (screen px),
/// in ink: its glyphs centered in a `Theme.line_height` times `size` line box whose top-left is
/// `origin`, as shape text's are, with `spacing` screen px after each glyph.
pub(crate) fn paint_line(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    text: &str,
    t: &Theme,
    bold: bool,
    size: f32,
    spacing: f32,
) {
    if text.is_empty() {
        return;
    }
    let mut font = gpui::font(t.font);
    // Asking for 600 itself benched about 1 ms slower per frame than Bold.
    font.weight = if bold {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let run = TextRun {
        len: text.len(),
        font,
        color: color(t.ink),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    // Quantised to half pixels, as shape text is.
    let size = px((size * 2.0).round() / 2.0);
    let l = window.text_system().layout_line(text, size, &[run], None);
    let ink = color(t.ink);
    let (a, d): (f32, f32) = (l.ascent.into(), l.descent.into());
    let baseline = px((f32::from(size) * t.line_height - (a + d)) / 2.0 + a);
    // ponytail: spacing counts glyphs, the core's measure counts graphemes; they differ only
    // for combining marks, off by one letter-space there.
    let mut n = 0.0;
    for run in &l.runs {
        for g in &run.glyphs {
            let o = origin + point(g.position.x + px(n * spacing), baseline);
            n += 1.0;
            // A missing glyph only loses that glyph; nothing to recover.
            let _ = if g.is_emoji {
                window.paint_emoji(o, run.font_id, g.id, size)
            } else {
                window.paint_glyph(o, run.font_id, g.id, size, ink)
            };
        }
    }
}

/// Underlines byte range `marked` (IME composition) of text laid out as `layout`, with the
/// text's top-left at `origin` (screen), scaled by zoom `z`: one stroke per line it spans.
pub(crate) fn paint_marked(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    layout: &Layout,
    marked: Range<usize>,
    t: &Theme,
    z: f32,
) {
    // Just under the em box, centered in the line.
    let below = (t.line_height() + t.font_size) / 2.0;
    for line in &layout.lines {
        let mut span = (line.clusters.iter())
            .filter(|c| c.range.start < marked.end && marked.start < c.range.end)
            .map(|c| (c.x, c.x + c.width));
        let Some(first) = span.next() else { continue };
        let (x0, x1) = span.fold(first, |(a, _), (_, b)| (a, b));
        let p = origin + point(px(x0 * z), px((line.y + below) * z));
        let b = Bounds::new(p, size(px((x1 - x0) * z), px(t.caret.marked_underline)));
        window.paint_quad(gpui::fill(b, color(t.ink)));
    }
}

/// UTF-8 offset to UTF-16 offset in `s`.
fn to16(s: &str, i: usize) -> usize {
    s[..i.min(s.len())].encode_utf16().count()
}

/// UTF-16 offset to UTF-8 offset in `s` (rounded forward to a char boundary).
fn from16(s: &str, u: usize) -> usize {
    let mut n = 0;
    for (i, ch) in s.char_indices() {
        if n >= u {
            return i;
        }
        n += ch.len_utf16();
    }
    s.len()
}

impl Willow {
    /// The code panel's text while it has focus, else the edited shape's plain text or zone's
    /// title; empty when not editing.
    // ponytail: hands the IME the whole panel text; slice around the caret if a 10,000-shape
    // document makes IME keystrokes slow.
    fn edited_text(&self) -> Cow<'_, str> {
        if self.session.panel().focused {
            return Cow::Borrowed(self.session.panel().text());
        }
        Cow::Owned(self.session.edited_text().unwrap_or_default())
    }

    /// The caret and the selection's other end, bytes into `edited_text`.
    fn caret_anchor(&self) -> (usize, usize) {
        let p = self.session.panel();
        if p.focused {
            return (p.caret(), p.anchor());
        }
        (self.session.editing()).map_or((0, 0), |e| (e.caret.offset, e.anchor))
    }
}

impl EntityInputHandler for Willow {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let s = self.edited_text();
        let r = from16(&s, range.start)..from16(&s, range.end);
        *actual = Some(to16(&s, r.start)..to16(&s, r.end));
        Some(s[r].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let s = self.edited_text();
        let (caret, anchor) = self.caret_anchor();
        let (caret, anchor) = (to16(&s, caret), to16(&s, anchor));
        Some(UTF16Selection {
            range: caret.min(anchor)..caret.max(anchor),
            reversed: caret < anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let m = if self.session.panel().focused {
            self.session.panel().marked()?
        } else {
            self.session.editing()?.marked.clone()?
        };
        let s = self.edited_text();
        Some(to16(&s, m.start)..to16(&s, m.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.input(Input::Unmark, cx);
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let s = self.edited_text();
        let replace = range.map(|r| from16(&s, r.start)..from16(&s, r.end));
        let text = text.to_string();
        self.input(Input::Text { text, replace }, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let s = self.edited_text();
        let replace = range.map(|r| from16(&s, r.start)..from16(&s, r.end));
        // The IME's selection is UTF-16 into the new marked text.
        let selected = selected.map(|r| from16(text, r.start)..from16(text, r.end));
        let text = text.to_string();
        self.input(
            Input::Mark {
                text,
                replace,
                selected,
            },
            cx,
        );
    }

    /// Screen bounds of the range's start, as wide as the range while it stays on one line:
    /// macOS puts the IME candidate window under it.
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        element: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let s = self.edited_text();
        let (a, b) = (from16(&s, range.start), from16(&s, range.end));
        // The panel registers its text area as the element: its points are relative to it.
        if self.session.panel().focused {
            let (a, b) = (self.session.panel_point(a), self.session.panel_point(b));
            let width = if a.y == b.y { b.x - a.x } else { 0.0 };
            let height = self.session.theme().code.row_height();
            return Some(Bounds::new(
                element.origin + point(px(a.x), px(a.y)),
                size(px(width.max(1.0)), px(height)),
            ));
        }
        let (a, b) = (self.session.text_point(a)?, self.session.text_point(b)?);
        let vp = self.session.viewport();
        let width = if a.y == b.y {
            (b.x - a.x) * vp.zoom
        } else {
            0.0
        };
        let p = vp.to_screen(a);
        // The caret's line: a zone title's differs from a shape's.
        let height = self.session.caret(self.start.elapsed())?.height * vp.zoom;
        Some(Bounds::new(
            self.canvas_origin + point(px(p.x), px(p.y)),
            size(px(width.max(1.0)), px(height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use gpui::PlatformTextSystem;
    use gpui_apple::AppleTextSystem;
    use willow_core::THEMES;

    use super::*;

    #[test]
    fn every_theme_font_has_its_own_face_for_each_style() {
        let ts = AppleTextSystem::new();
        ts.add_fonts(crate::bundled_fonts()).unwrap();
        for t in THEMES {
            let styles = [(false, false), (true, false), (false, true), (true, true)];
            let mut ids = styles.map(|(b, i)| ts.font_id(&font(t, b, i)).unwrap());
            ids.sort_by_key(|id| id.0);
            assert!(ids.windows(2).all(|w| w[0] != w[1]), "{}: {ids:?}", t.name);
        }
    }
}
