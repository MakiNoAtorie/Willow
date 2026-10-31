//! The code panel's state: open or closed, focused or not, its text, width and scroll, and
//! when the text is regenerated from the document.

use std::cell::{Ref, RefCell};
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::Session;
use super::panel_apply::{Versions, canonical_lines};
use super::panel_direction::Words;
use super::panel_edit::PanelEdit;
use crate::input::{Input, Response};
use crate::syntax::{Token, tokens};
use crate::{Document, LineKind, Rgba, format};

/// The code panel of one window. Starts closed, with no text.
#[derive(Clone, Debug)]
pub struct Panel {
    pub open: bool,
    /// Keys go to the panel, not the canvas.
    pub focused: bool,
    /// Screen pixels. The app gives the canvas the window's width less this while open.
    pub width: f32,
    /// How far the text is scrolled up, screen pixels.
    pub(super) scroll: f32,
    text: String,
    /// The history revision the text was last regenerated for; `None` forces the next.
    pub(super) regenerated: Option<u64>,
    /// The history revision whose document the text holds: Save and recovery write the text
    /// while it is the current one.
    pub(super) held: Option<u64>,
    /// What each line is, as last checked or regenerated; empty until then.
    pub(super) lines: Vec<LineKind>,
    /// The selection has no line in the text yet: show it once regenerated.
    pub(super) seek: bool,
    /// Where each line's rows start, for the text and column count it was made for.
    rows: RefCell<Option<Rows>>,
    pub(super) edit: PanelEdit,
    pub(super) versions: Versions,
    /// The history revision just after the last applied text edit, while later ones join its
    /// undo step.
    pub(super) merge: Option<u64>,
    /// The direction words of the text that last held the document: each shape's stated
    /// anchor and side. Empty for regenerated text, whose words are the sides shapes sit on.
    pub(super) words: Words,
}

impl Panel {
    fn new(width: f32) -> Self {
        Panel {
            open: false,
            focused: false,
            width,
            scroll: 0.0,
            text: String::new(),
            regenerated: None,
            held: None,
            lines: Vec::new(),
            seek: false,
            rows: RefCell::default(),
            edit: PanelEdit::default(),
            versions: Versions::default(),
            merge: None,
            words: Words::new(),
        }
    }

    /// The text the panel shows.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Each logical line's start (byte) and first row, and the row count, at `cols` columns.
#[derive(Clone, Debug)]
pub(super) struct Rows {
    pub(super) cols: usize,
    pub(super) lines: Vec<(usize, usize)>,
    pub(super) total: usize,
}

impl Rows {
    /// `text` is the text these rows were made for with `range` replaced by `len` bytes:
    /// wraps again the lines the replacement touches and shifts the ones after it.
    fn splice(&mut self, text: &str, range: Range<usize>, len: usize) {
        let line = |at: usize| self.lines.partition_point(|&(s, _)| s <= at) - 1;
        let (a, b) = (line(range.start), line(range.end));
        let (start, first) = self.lines[a];
        let rows_after = self.lines.get(b + 1).map_or(self.total, |l| l.1);
        let end = line_end(text, range.start + len);
        let (mut new, mut at, mut row) = (Vec::new(), start, first);
        for l in text[start..end].split('\n') {
            new.push((at, row));
            row += wrap(l, self.cols).len();
            at += l.len() + 1;
        }
        let shift = |v: usize, by: (usize, usize)| v + by.0 - by.1;
        let (bytes, rows) = ((len, range.len()), (row, rows_after));
        for l in &mut self.lines[b + 1..] {
            *l = (shift(l.0, bytes), shift(l.1, rows));
        }
        self.total = shift(self.total, rows);
        self.lines.splice(a..=b, new);
    }
}

/// One soft-wrapped row of panel text in view.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelRow {
    /// The logical line's number, counted from 1, on its first row only.
    pub number: Option<usize>,
    /// Top of the row, screen pixels down from the panel's top.
    pub y: f32,
    /// Byte range into the panel text, line break excluded.
    pub range: Range<usize>,
    /// Colored runs covering `range` in order.
    pub spans: Vec<Span>,
    /// The line is invalid: drawn with `CodeStyle.invalid`'s wavy underline and tint.
    pub invalid: bool,
}

/// A run of panel text in one color; `bold` is `CodeStyle.id_weight`.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub color: Rgba,
    pub bold: bool,
}

/// `line` in colored runs end to end: its tokens, and plain ink between them.
fn color_spans(line: &str, c: &crate::CodeStyle, ink: Rgba) -> Vec<Span> {
    let mut out = Vec::new();
    let plain = |range: Range<usize>, out: &mut Vec<Span>| {
        if !range.is_empty() {
            out.push(Span {
                range,
                color: ink,
                bold: false,
            });
        }
    };
    let mut at = 0;
    for (range, token) in tokens(line) {
        plain(at..range.start, &mut out);
        at = range.end;
        let color = match token {
            Token::Keyword => c.keyword,
            Token::Id => c.id,
            Token::String => c.string,
            Token::Direction => c.direction,
            Token::Layout => c.layout,
        };
        let bold = token == Token::Id;
        out.push(Span { range, color, bold });
    }
    plain(at..line.len(), &mut out);
    out
}

/// The end of the line that byte `from` is on, line break excluded.
pub(super) fn line_end(s: &str, from: usize) -> usize {
    s[from..].find('\n').map_or(s.len(), |i| from + i)
}

/// Columns a grapheme takes: 2 for East Asian wide characters and emoji, else 1.
pub(super) fn columns(g: &str) -> usize {
    let c = g.chars().next().map_or(0, u32::from);
    let wide = matches!(c, 0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1faff
        | 0x20000..=0x3fffd);
    1 + wide as usize
}

/// Byte ranges of the rows `line` wraps into at `cols` columns: after the last space that
/// fits, else after the last grapheme that fits. An empty line is one empty row.
pub(super) fn wrap(line: &str, cols: usize) -> Vec<Range<usize>> {
    // No grapheme takes more columns than bytes.
    if line.len() <= cols {
        return std::iter::once(0..line.len()).collect();
    }
    // An ASCII line is a column per byte: skip the grapheme segmenter.
    let ascii = line.is_ascii();
    let graphemes: Box<dyn Iterator<Item = (usize, &str)>> = if ascii {
        Box::new((0..line.len()).map(|i| (i, &line[i..i + 1])))
    } else {
        Box::new(line.grapheme_indices(true))
    };
    let mut rows = Vec::new();
    let (mut start, mut col, mut space) = (0, 0, None);
    for (i, g) in graphemes {
        let w = columns(g);
        if col + w > cols && i > start {
            let end = space.filter(|&s| s > start).unwrap_or(i);
            rows.push(start..end);
            col = if ascii {
                i - end
            } else {
                line[end..i].graphemes(true).map(columns).sum()
            };
            (start, space) = (end, None);
        }
        col += w;
        if g == " " {
            space = Some(i + 1);
        }
    }
    rows.push(start..line.len());
    rows
}

/// Canonical text to write from a document: built on the UI thread by
/// `Session::take_regeneration`, `run` anywhere, handed back through `Session::regenerated`.
pub struct Regeneration {
    document: Document,
    heights: Vec<f32>,
    drawn: Vec<(i32, i32)>,
    revision: u64,
    version: u64,
    /// The panel had focus when it was taken.
    focused: bool,
}

impl Regeneration {
    pub fn run(self) -> Regenerated {
        let text = format::save(&self.document, &self.heights, &self.drawn);
        Regenerated {
            lines: canonical_lines(&text, &self.document),
            text,
            revision: self.revision,
            version: self.version,
            focused: self.focused,
        }
    }
}

/// A finished `Regeneration`.
pub struct Regenerated {
    text: String,
    lines: Vec<LineKind>,
    revision: u64,
    version: u64,
    focused: bool,
}

impl Session {
    pub(super) fn new_panel(theme: &crate::Theme) -> Panel {
        Panel::new(theme.code.width)
    }

    /// Sets the panel's width to `width` kept within the theme's minimum and half of
    /// `window_width`; half wins in a window narrower than twice the minimum. A preference,
    /// not part of the document: no history, no undo.
    pub fn set_panel_width(&mut self, width: f32, window_width: f32) {
        self.panel.width = width.max(self.theme.code.min_width).min(window_width / 2.0);
    }

    pub fn panel(&self) -> &Panel {
        &self.panel
    }

    /// Opens the panel, the canvas keeping the keys: a window's open panel carried over to
    /// the document that replaces it. Regenerates unless the text holds the document.
    pub fn show_panel(&mut self) {
        self.panel.open = true;
        if self.held_text().is_none() {
            self.panel.regenerated = None;
        }
    }

    /// Replaces the panel text: what regeneration lands through, and what typing in the panel
    /// writes.
    pub fn set_panel_text(&mut self, text: String) {
        self.panel.text = text;
        // The text as it is now is what the document holds.
        self.panel.versions.now += 1;
        self.panel.held = Some(self.history.revision());
        *self.panel.rows.get_mut() = None;
        self.panel.edit.fit(&self.panel.text);
    }

    /// Replaces `range` of the panel text with `text` as `set_panel_text` does, wrapping again
    /// only the lines it touches.
    pub(super) fn splice_panel_text(&mut self, range: Range<usize>, text: &str) {
        let rows = self.panel.rows.get_mut().take();
        let mut all = std::mem::take(&mut self.panel.text);
        all.replace_range(range.clone(), text);
        self.set_panel_text(all);
        if let Some(mut rows) = rows {
            rows.splice(&self.panel.text, range, text.len());
            *self.panel.rows.get_mut() = Some(rows);
        }
    }

    /// Text columns in a row at the panel's width.
    fn row_columns(&self) -> usize {
        let c = &self.theme.code;
        let room = self.panel.width - c.border_width - c.gutter_width - c.pad_right;
        (room / c.column()).floor().max(1.0) as usize
    }

    /// Where each line's rows start, made again after the width changed or new text was set;
    /// typing (`splice_panel_text`) keeps them.
    // ponytail: new text and each width step wrap the whole text (about 3 ms at 10,000
    // shapes); wrap off the UI thread with the regeneration if that shows in frame times.
    pub(super) fn rows(&self) -> Ref<'_, Rows> {
        let cols = self.row_columns();
        if self
            .panel
            .rows
            .borrow()
            .as_ref()
            .is_none_or(|r| r.cols != cols)
        {
            let (mut lines, mut start, mut total) = (Vec::new(), 0, 0);
            for line in self.panel.text.split('\n') {
                lines.push((start, total));
                total += wrap(line, cols).len();
                start += line.len() + 1;
            }
            *self.panel.rows.borrow_mut() = Some(Rows { cols, lines, total });
        }
        Ref::map(self.panel.rows.borrow(), |r| {
            r.as_ref().expect("made above")
        })
    }

    /// The scroll that keeps the last row above the bottom padding, or 0 when all fits.
    pub(super) fn max_scroll(&self) -> f32 {
        let c = &self.theme.code;
        let content = c.pad_top + self.rows().total as f32 * c.row_height() + c.pad_bottom;
        (content - self.viewport.size.y).max(0.0)
    }

    /// How far the text is scrolled now: the writer's scroll, kept within what the text allows.
    pub(super) fn panel_scroll(&self) -> f32 {
        self.panel.scroll.min(self.max_scroll())
    }

    /// The rows in view, top to bottom, colored. The panel is as tall as the canvas.
    pub fn panel_rows(&self) -> Vec<PanelRow> {
        let (c, ink, text) = (&self.theme.code, self.theme.ink, &self.panel.text);
        let (rh, height) = (c.row_height(), self.viewport.size.y);
        let scroll = self.panel_scroll();
        let rows = self.rows();
        let first = ((scroll - c.pad_top) / rh).floor().max(0.0) as usize;
        let from = rows.lines.partition_point(|&(_, r)| r <= first) - 1;
        let mut out = Vec::new();
        for (n, &(start, row)) in rows.lines.iter().enumerate().skip(from) {
            if c.pad_top + row as f32 * rh - scroll >= height {
                break;
            }
            let end = line_end(text, start);
            let kind = self.panel.lines.get(n);
            let invalid = kind == Some(&LineKind::Invalid);
            let spans = match kind {
                // An ignored line is all in the `@layout` gray.
                Some(LineKind::Ignored) => vec![Span {
                    range: 0..end - start,
                    color: c.layout,
                    bold: false,
                }],
                _ => color_spans(&text[start..end], c, ink),
            };
            for (k, r) in wrap(&text[start..end], rows.cols).into_iter().enumerate() {
                let y = c.pad_top + (row + k) as f32 * rh - scroll;
                if y + rh <= 0.0 || y >= height {
                    continue;
                }
                let spans = (spans.iter())
                    .filter(|s| s.range.start < r.end && r.start < s.range.end)
                    .map(|s| Span {
                        range: start + s.range.start.max(r.start)..start + s.range.end.min(r.end),
                        ..s.clone()
                    })
                    .collect();
                let number = (k == 0).then_some(n + 1);
                let range = start + r.start..start + r.end;
                out.push(PanelRow {
                    number,
                    y,
                    range,
                    spans,
                    invalid,
                });
            }
        }
        out
    }

    /// The regeneration due now, if any: while the panel is open, once after each finished
    /// undo step (none while an editing session or other group is open), and on opening. The
    /// app runs it off the UI thread and hands the result to `regenerated`.
    pub fn take_regeneration(&mut self) -> Option<Regeneration> {
        let revision = self.history.revision();
        if !self.panel.open || self.history.is_open() || self.panel.regenerated == Some(revision) {
            return None;
        }
        self.panel.regenerated = Some(revision);
        Some(Regeneration {
            document: self.doc.clone(),
            heights: self.heights(),
            drawn: self.drawn_positions(),
            revision,
            version: self.panel.versions.now,
            focused: self.panel.focused,
        })
    }

    /// Shows a regeneration's text, unless the document has changed since it was taken (a
    /// newer one is due), the panel text has (nothing rewrites what the writer typed), or the
    /// panel has taken focus since (nothing moves the text under a caret just placed; the
    /// first key regenerates).
    pub fn regenerated(&mut self, r: Regenerated) {
        let focus_taken = self.panel.focused && !r.focused;
        if r.revision == self.history.revision()
            && r.version == self.panel.versions.now
            && !focus_taken
        {
            self.set_panel_text(r.text);
            self.panel.lines = r.lines;
            self.text_shown();
            self.settle_text(true);
            self.panel_seek_selection();
        }
    }

    /// Opens the panel on `text`, checked as `lines` and stating `words`, as what holds the
    /// document now: an invalid file loaded. Nothing regenerates it until the document changes.
    pub(super) fn hold_text(&mut self, text: String, lines: Vec<LineKind>, words: Words) {
        let now = Some(self.history.revision());
        self.set_panel_text(text);
        let p = &mut self.panel;
        (p.open, p.lines, p.held, p.regenerated) = (true, lines, now, now);
        p.words = words;
        self.text_shown();
        self.settle_text(false);
    }

    /// Lets go of any held text: the next regeneration is due, and Save and recovery write
    /// the document.
    pub(super) fn release_text(&mut self) {
        let p = &mut self.panel;
        (p.lines, p.held, p.regenerated) = (Vec::new(), None, None);
        self.settle_text(true);
    }

    /// The panel text, while it holds the document as it is now.
    pub(super) fn held_text(&self) -> Option<&str> {
        (self.panel.held == Some(self.history.revision())).then_some(&self.panel.text)
    }

    /// Keys go to the panel: the shape being edited, a menu and the Tab picker let go.
    fn focus_panel(&mut self) {
        self.stop_editing();
        (self.menu, self.tab_picker) = (None, false);
        self.panel.focused = true;
        self.panel.edit.since = self.now;
    }

    /// `input` as the panel takes it first: `None` goes on to the canvas. While the panel has
    /// focus, only pointer input, view changes, undo and redo (pending text applied first)
    /// reach the canvas.
    pub(super) fn panel_input(&mut self, input: &Input) -> Option<Response> {
        match input {
            Input::TogglePanel if self.panel.open => {
                self.leave_panel();
                self.panel.open = false;
            }
            Input::TogglePanel => {
                self.show_panel();
                self.focus_panel();
            }
            Input::FocusPanel => self.focus_panel(),
            // Positions past the canvas's right edge are in the panel.
            Input::PointerDown { pos, .. }
            | Input::DoubleClick { pos }
            | Input::TripleClick { pos }
                if self.panel.open && pos.x >= self.viewport.size.x =>
            {
                self.focus_panel();
                let at = self.panel.caret();
                let r = self.panel_press(input);
                self.caret_moved(at);
                return Some(r);
            }
            Input::PointerMove { pos, .. } if self.panel.edit.drag => {
                let at = self.panel.caret();
                let r = self.panel_drag(*pos);
                self.caret_moved(at);
                return Some(r);
            }
            Input::PointerUp { .. } if self.panel.edit.drag => self.panel.edit.drag = false,
            &Input::ScrollPanel(dy) => {
                let max = self.max_scroll();
                self.panel.scroll = (self.panel.scroll.min(max) - dy).clamp(0.0, max);
            }
            _ if !self.panel.focused => return None,
            Input::PointerDown { .. } => {
                self.leave_panel();
                return None;
            }
            Input::Escape => self.leave_panel(),
            Input::PointerMove { .. }
            | Input::PointerUp { .. }
            | Input::DoubleClick { .. }
            | Input::PointerLeft
            | Input::Wheel { .. }
            | Input::Pinch { .. }
            | Input::ResetZoom
            | Input::ZoomIn
            | Input::ZoomOut
            | Input::FocusLost
            | Input::ModifiersChanged
            | Input::KeyUp(_)
            | Input::MenuChoose(_)
            | Input::CloseMenu => return None,
            Input::Undo | Input::Redo => {
                self.flush_text();
                return None;
            }
            // Canvas commands and text input: the panel's keys.
            _ => {
                self.fresh_text();
                self.before_text_key(input);
                let at = self.panel.caret();
                let r = self.panel_key(input);
                self.after_text_key(input, at);
                return Some(r);
            }
        }
        Some(self.respond(true))
    }
}
