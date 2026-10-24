//! Shape text on the GPUI side: the text-measuring function the core lays out with, glyphs
//! painted one by one where the core put their clusters, and the platform text input adapter.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    Bounds, Context, EntityInputHandler, Font, FontStyle, FontWeight, LineLayout, Pixels, TextRun,
    UTF16Selection, Window, WindowTextSystem, point, px, size,
};
use willow_core::{Input, Layout, Measure, Paragraph, RichText, Theme};

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

/// The core's text-measuring function, backed by the window's text system.
pub(crate) struct TextMeasure(pub Arc<WindowTextSystem>);

impl Measure for TextMeasure {
    fn advances(&self, para: &Paragraph, clusters: &[Range<usize>], t: &Theme) -> Vec<f32> {
        let x = x_by_byte(&shape(&self.0, para, t));
        clusters.iter().map(|c| x[c.end] - x[c.start]).collect()
    }
}

/// Paints `text` laid out as `layout`, with the text's top-left at `origin` (screen), scaled
/// by zoom `z`. Each glyph keeps its offset inside its cluster; the cluster goes where the
/// core placed it.
pub(crate) fn paint_text(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    text: &RichText,
    layout: &Layout,
    t: &Theme,
    z: f32,
) {
    let ts = window.text_system().clone();
    // Quantised to half pixels to bound glyph atlas entries across zoom levels.
    let font_size = px((t.font_size * z * 2.0).round() / 2.0);
    let ink = color(t.ink);
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
    /// The edited shape's plain text; empty when not editing.
    fn edited_text(&self) -> String {
        let s = &self.session;
        let id = s.editing().map(|e| &e.id);
        (s.document().shapes.iter())
            .find(|sh| Some(&sh.id) == id)
            .map_or_else(String::new, |sh| sh.text.plain())
    }
}

// ponytail: IME marked text isn't shown or tracked (#13); a commit arrives as plain text.
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
        let (caret, anchor) = (self.session.editing())
            .map_or((0, 0), |e| (to16(&s, e.caret.offset), to16(&s, e.anchor)));
        Some(UTF16Selection {
            range: caret.min(anchor)..caret.max(anchor),
            reversed: caret < anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

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
        _: Option<Range<usize>>,
        _: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let c = self.session.caret(self.start.elapsed())?;
        let vp = self.session.viewport();
        let p = vp.to_screen(c.pos);
        Some(Bounds::new(
            self.canvas_origin + point(px(p.x), px(p.y)),
            size(px(1.0), px(c.height * vp.zoom)),
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
