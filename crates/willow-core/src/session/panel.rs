//! The code panel's state: open or closed, focused or not, its text, width and scroll, and
//! when the text is regenerated from the document.

use std::cell::{Ref, RefCell};
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::Session;
use crate::input::{Input, Response};
use crate::syntax::{Token, tokens};
use crate::{Document, Rgba, format};

/// The code panel of one window. Starts closed, with no text.
#[derive(Clone, Debug)]
pub struct Panel {
    pub open: bool,
    /// Keys go to the panel, not the canvas.
    pub focused: bool,
    /// Screen pixels. The app gives the canvas the window's width less this while open.
    pub width: f32,
    /// How far the text is scrolled up, screen pixels.
    scroll: f32,
    text: String,
    /// The history revision the text was last regenerated for; `None` forces the next.
    regenerated: Option<u64>,
    /// Where each line's rows start, for the text and column count it was made for.
    rows: RefCell<Option<Rows>>,
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
            rows: RefCell::default(),
        }
    }

    /// The text the panel shows.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Each logical line's start (byte) and first row, and the row count, at `cols` columns.
#[derive(Clone, Debug)]
struct Rows {
    cols: usize,
    lines: Vec<(usize, usize)>,
    total: usize,
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

/// Columns a grapheme takes: 2 for East Asian wide characters and emoji, else 1.
fn columns(g: &str) -> usize {
    let c = g.chars().next().map_or(0, u32::from);
    let wide = matches!(c, 0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff
        | 0xfe30..=0xfe4f | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1faff
        | 0x20000..=0x3fffd);
    1 + wide as usize
}

/// Byte ranges of the rows `line` wraps into at `cols` columns: after the last space that
/// fits, else after the last grapheme that fits. An empty line is one empty row.
fn wrap(line: &str, cols: usize) -> Vec<Range<usize>> {
    let mut rows = Vec::new();
    let (mut start, mut col, mut space) = (0, 0, None);
    for (i, g) in line.grapheme_indices(true) {
        let w = columns(g);
        if col + w > cols && i > start {
            let end = space.filter(|&s| s > start).unwrap_or(i);
            rows.push(start..end);
            col = line[end..i].graphemes(true).map(columns).sum();
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
    revision: u64,
}

impl Regeneration {
    pub fn run(self) -> Regenerated {
        Regenerated {
            text: format::save(&self.document, &self.heights),
            revision: self.revision,
        }
    }
}

/// A finished `Regeneration`.
pub struct Regenerated {
    text: String,
    revision: u64,
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

    /// Replaces the panel text: what regeneration lands through, and what typing in the panel
    /// writes.
    pub fn set_panel_text(&mut self, text: String) {
        self.panel.text = text;
        *self.panel.rows.get_mut() = None;
    }

    /// Text columns in a row at the panel's width.
    fn columns(&self) -> usize {
        let c = &self.theme.code;
        let room = self.panel.width - c.border_width - c.gutter_width - c.pad_right;
        (room / (c.advance * c.font_size)).floor().max(1.0) as usize
    }

    /// Where each line's rows start, made again after the text or width changed.
    // ponytail: wraps the whole text after every change; per-line row counts if typing in
    // a 10,000-shape document misses its frame budget.
    fn rows(&self) -> Ref<'_, Rows> {
        let cols = self.columns();
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
    fn max_scroll(&self) -> f32 {
        let c = &self.theme.code;
        let content = c.pad_top + self.rows().total as f32 * c.row_height() + c.pad_bottom;
        (content - self.viewport.size.y).max(0.0)
    }

    /// The rows in view, top to bottom, colored. The panel is as tall as the canvas.
    pub fn panel_rows(&self) -> Vec<PanelRow> {
        let (c, ink, text) = (&self.theme.code, self.theme.ink, &self.panel.text);
        let (rh, height) = (c.row_height(), self.viewport.size.y);
        let scroll = self.panel.scroll.min(self.max_scroll());
        let rows = self.rows();
        let first = ((scroll - c.pad_top) / rh).floor().max(0.0) as usize;
        let from = rows.lines.partition_point(|&(_, r)| r <= first) - 1;
        let mut out = Vec::new();
        for (n, &(start, row)) in rows.lines.iter().enumerate().skip(from) {
            if c.pad_top + row as f32 * rh - scroll >= height {
                break;
            }
            let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
            let spans = color_spans(&text[start..end], c, ink);
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
            revision,
        })
    }

    /// Shows a regeneration's text, unless the document has changed since it was taken (a
    /// newer one is due).
    pub fn regenerated(&mut self, r: Regenerated) {
        if r.revision == self.history.revision() {
            self.set_panel_text(r.text);
        }
    }

    /// Keys go to the panel: the shape being edited, a menu and the Tab picker let go.
    fn focus_panel(&mut self) {
        self.stop_editing();
        (self.menu, self.tab_picker) = (None, false);
        self.panel.focused = true;
    }

    /// `input` as the panel takes it first: `None` goes on to the canvas. While the panel has
    /// focus, only pointer input, view changes, undo and redo reach the canvas.
    pub(super) fn panel_input(&mut self, input: &Input) -> Option<Response> {
        match input {
            Input::TogglePanel if self.panel.open => {
                (self.panel.open, self.panel.focused) = (false, false);
            }
            Input::TogglePanel => {
                self.panel.open = true;
                self.panel.regenerated = None;
                self.focus_panel();
            }
            Input::FocusPanel => self.focus_panel(),
            &Input::ScrollPanel(dy) => {
                let max = self.max_scroll();
                self.panel.scroll = (self.panel.scroll.min(max) - dy).clamp(0.0, max);
            }
            _ if !self.panel.focused => return None,
            Input::PointerDown { .. } => {
                self.panel.focused = false;
                return None;
            }
            Input::Escape => self.panel.focused = false,
            Input::PointerMove { .. }
            | Input::PointerUp { .. }
            | Input::DoubleClick { .. }
            | Input::PointerLeft
            | Input::Wheel { .. }
            | Input::Pinch { .. }
            | Input::ResetZoom
            | Input::Undo
            | Input::Redo
            | Input::FocusLost
            | Input::ModifiersChanged
            | Input::KeyUp(_)
            | Input::MenuChoose(_)
            | Input::CloseMenu => return None,
            // Canvas commands and text input: the panel's keys.
            _ => return Some(self.respond(false)),
        }
        Some(self.respond(true))
    }
}
