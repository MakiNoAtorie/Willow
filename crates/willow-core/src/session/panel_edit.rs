//! Typing in the code panel: a plain-text editor over the panel text, with its caret,
//! selection and IME marked text. Nothing here reaches the document or the history.

use std::ops::Range;
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use super::Session;
use super::editing::CaretView;
use super::panel::{Panel, columns, wrap};
use crate::edit::{next_cluster, prev_cluster, word_left, word_right};
use crate::input::{Button, Input, Motion, Point, Response};
use crate::layout::Caret;

/// The panel's caret and selection. Offsets are bytes into the panel text.
#[derive(Clone, Debug, Default)]
pub(super) struct PanelEdit {
    pub(super) caret: Caret,
    /// The selection's other end; `caret.offset` when nothing is selected.
    pub(super) anchor: usize,
    /// IME composition shown in the text, not yet committed.
    pub(super) marked: Option<Range<usize>>,
    /// Column that up/down keep, set by the first vertical move.
    pub(super) goal: Option<f32>,
    /// When the writer last changed the text, marked text aside.
    pub(super) typed: Option<Duration>,
    /// A left press in the panel is down: moving extends the selection.
    pub(super) drag: bool,
    /// Last caret move or edit: the caret blinks from here until `CaretStyle::idle`.
    pub(super) since: Duration,
}

impl PanelEdit {
    /// Keeps the caret and selection inside new text `s`, dropping any marked text.
    pub(super) fn fit(&mut self, s: &str) {
        self.caret = downstream(s.floor_char_boundary(self.caret.offset));
        (self.anchor, self.marked) = (s.floor_char_boundary(self.anchor), None);
    }
}

impl Panel {
    /// The caret, a byte offset into the panel text.
    pub fn caret(&self) -> usize {
        self.edit.caret.offset
    }

    /// The selection's other end; the caret when nothing is selected.
    pub fn anchor(&self) -> usize {
        self.edit.anchor
    }

    /// IME composition shown in the text, not yet committed.
    pub fn marked(&self) -> Option<Range<usize>> {
        self.edit.marked.clone()
    }

    /// When the writer last changed the text: typing, deleting, pasting or committing IME
    /// text; `None` until then. Marked text alone isn't a change yet.
    pub fn typed_at(&self) -> Option<Duration> {
        self.edit.typed
    }

    /// The selected bytes, in order; empty at the caret when nothing is selected.
    pub fn selection(&self) -> Range<usize> {
        let (a, c) = (self.edit.anchor, self.edit.caret.offset);
        a.min(c)..a.max(c)
    }
}

impl Session {
    /// A press, double- or triple-click in the panel, which has focus now. A left press puts
    /// the caret there (Shift: extends the selection) and starts a drag; double-click selects
    /// the word there, triple-click its line.
    pub(super) fn panel_press(&mut self, input: &Input) -> Response {
        let mut on = None;
        match *input {
            Input::PointerDown {
                pos,
                button: Button::Left,
                mods,
            } => {
                let (row, col) = self.panel_hit(pos);
                let c = self.panel_stop(row, col, true);
                let e = &mut self.panel.edit;
                (e.caret, e.goal, e.drag) = (c, None, true);
                if !mods.shift {
                    e.anchor = c.offset;
                }
                e.since = self.now;
                on = Some(c.offset);
            }
            Input::DoubleClick { pos } | Input::TripleClick { pos } => {
                let (row, col) = self.panel_hit(pos);
                let i = self.panel_stop(row, col, false).offset;
                let s = self.panel.text();
                let start = s[..i].rfind('\n').map_or(0, |n| n + 1);
                let end = line_end(s, start);
                let range = if let Input::TripleClick { .. } = input {
                    start..(end + 1).min(s.len())
                } else {
                    let word = (s[start..end].split_word_bound_indices())
                        .map(|(j, w)| start + j..start + j + w.len())
                        .find(|w| i < w.end || w.end == end);
                    word.unwrap_or(i..i)
                };
                let e = &mut self.panel.edit;
                (e.anchor, e.caret, e.goal) = (range.start, downstream(range.end), None);
                e.since = self.now;
                // A triple-click's caret is on the next line.
                on = Some(i);
            }
            _ => {}
        }
        if let Some(at) = on {
            self.panel_follow_caret(at);
        }
        self.respond(true)
    }

    /// The left button moved to `pos` while down in the panel: the selection follows it.
    // ponytail: no scrolling while dragging past the panel's top or bottom; add it if
    // selecting more than a screenful by mouse is missed.
    pub(super) fn panel_drag(&mut self, pos: Point) -> Response {
        let (row, col) = self.panel_hit(pos);
        self.panel.edit.caret = self.panel_stop(row, col, true);
        self.panel.edit.since = self.now;
        self.panel_follow_caret(self.panel.edit.caret.offset);
        self.respond(true)
    }

    /// The row (unclamped) and column at canvas-relative `pos`.
    fn panel_hit(&self, pos: Point) -> (usize, f32) {
        let c = &self.theme.code;
        let scroll = self.panel.scroll.min(self.max_scroll());
        let x = pos.x - self.viewport.size.x - c.border_width - c.gutter_width;
        let row = (pos.y + scroll - c.pad_top) / c.row_height();
        (row.max(0.0) as usize, x / (c.advance * c.font_size))
    }

    /// A key or text input while the panel has focus.
    pub(super) fn panel_key(&mut self, input: &Input) -> Response {
        match input {
            Input::Text { text, replace } => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                self.panel_replace(self.panel_typing(replace), &text);
            }
            Input::Mark {
                text,
                replace,
                selected,
            } => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                let range = self.panel_typing(replace);
                self.splice_panel_text(range.clone(), &text);
                let at = |i: usize| range.start + text.floor_char_boundary(i);
                let sel = selected.clone().unwrap_or(text.len()..text.len());
                let e = &mut self.panel.edit;
                (e.caret, e.anchor, e.goal) = (downstream(at(sel.end)), at(sel.start), None);
                e.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
            }
            // `dispatch` has committed it.
            Input::Unmark => {}
            // The app has put `clipboard_text` on the clipboard.
            Input::Cut => self.panel_replace(self.panel.selection(), ""),
            Input::Paste(text) => {
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                let text: String = (text.replace('\t', "  ").chars())
                    .filter(|&c| c == '\n' || !c.is_control())
                    .collect();
                self.panel_replace(self.panel.selection(), &text);
            }
            Input::Tab => self.panel_replace(self.panel.selection(), "  "),
            Input::Enter | Input::NewLine => {
                // The new line starts with the spaces that start this one, up to the caret.
                let (s, sel) = (self.panel.text(), self.panel.selection());
                let before = &s[s[..sel.start].rfind('\n').map_or(0, |i| i + 1)..sel.start];
                let indent = before.len() - before.trim_start_matches(' ').len();
                let text = format!("\n{}", &before[..indent]);
                self.panel_replace(sel, &text);
            }
            &Input::Move(m) => {
                let (c, goal) = self.panel_motion(m, false);
                let e = &mut self.panel.edit;
                (e.caret, e.anchor, e.goal) = (c, c.offset, goal);
            }
            &Input::Select(m) => {
                let (c, goal) = self.panel_motion(m, true);
                (self.panel.edit.caret, self.panel.edit.goal) = (c, goal);
            }
            &Input::Delete(m) => {
                let mut sel = self.panel.selection();
                if sel.is_empty() {
                    let to = self.panel_motion(m, false).0.offset;
                    sel = sel.start.min(to)..sel.start.max(to);
                }
                self.panel_replace(sel, "");
            }
            Input::SelectAll => {
                let end = downstream(self.panel.text().len());
                let e = &mut self.panel.edit;
                (e.anchor, e.caret, e.goal) = (0, end, None);
            }
            _ => return self.respond(false),
        }
        self.panel.edit.since = self.now;
        self.panel_reveal();
        // Typing waits for the next check, which `lines` is stale until.
        if matches!(input, Input::Move(_) | Input::Select(_)) {
            self.panel_follow_caret(self.panel.edit.caret.offset);
        }
        self.respond(true)
    }

    /// Scrolls the least that shows the caret's row between the top and bottom padding.
    fn panel_reveal(&mut self) {
        self.panel_show_row(self.panel_spot(self.panel.edit.caret).0);
    }

    /// The panel caret to draw at `now` while the panel has focus, in `PanelRow`
    /// coordinates (x from inside the hairline), one row tall.
    pub fn panel_caret(&self, now: Duration) -> Option<CaretView> {
        self.panel.focused.then(|| CaretView {
            pos: self.panel_xy(self.panel.edit.caret),
            height: self.theme.code.row_height(),
            visible: self.caret_shown(now),
        })
    }

    /// Top of the caret stop at byte `offset` of the panel text, as `panel_caret`. The app
    /// places the IME candidate window with it.
    pub fn panel_point(&self, offset: usize) -> Point {
        let offset = self.panel.text().floor_char_boundary(offset);
        self.panel_xy(downstream(offset))
    }

    /// Highlight for byte range `range` on the rows in view, one (top-left, bottom-right)
    /// per row, as `panel_caret`. A line break in the range is one more column.
    pub fn panel_rects(&self, range: Range<usize>) -> Vec<(Point, Point)> {
        let (c, s) = (&self.theme.code, self.panel.text());
        let cw = c.advance * c.font_size;
        let cols = |a: usize, b: usize| s[a..b].graphemes(true).map(columns).sum::<usize>();
        let mut out = Vec::new();
        for r in self.panel_rows() {
            let (a, b) = (range.start.max(r.range.start), range.end.min(r.range.end));
            let newline = range.end > r.range.end && s[r.range.end..].starts_with('\n');
            if a > b || (a == b && !newline) {
                continue;
            }
            let x = |col: usize| c.gutter_width + col as f32 * cw;
            let from = cols(r.range.start, a);
            let to = from + cols(a, b) + newline as usize;
            out.push((
                Point { x: x(from), y: r.y },
                Point {
                    x: x(to),
                    y: r.y + c.row_height(),
                },
            ));
        }
        out
    }

    /// Where caret `c` is drawn, as `panel_caret`.
    fn panel_xy(&self, c: Caret) -> Point {
        let code = &self.theme.code;
        let (row, col) = self.panel_spot(c);
        let scroll = self.panel.scroll.min(self.max_scroll());
        Point {
            x: code.gutter_width + col as f32 * code.advance * code.font_size,
            y: code.pad_top + row as f32 * code.row_height() - scroll,
        }
    }

    /// Where motion `m` takes the caret, and the column up/down keep. Unless `extend`ing,
    /// left and right collapse a selection to its edge. Up from the first row goes to the
    /// start, down from the last to the end.
    fn panel_motion(&self, m: Motion, extend: bool) -> (Caret, Option<f32>) {
        let (s, c) = (self.panel.text(), self.panel.edit.caret);
        let sel = self.panel.selection();
        let i = c.offset;
        if let Motion::Up | Motion::Down = m {
            let (row, col) = self.panel_spot(c);
            let goal = self.panel.edit.goal.unwrap_or(col as f32);
            let c = match m {
                Motion::Up if row == 0 => downstream(0),
                Motion::Up => self.panel_stop(row - 1, goal, true),
                _ if row + 1 == self.rows().total => downstream(s.len()),
                _ => self.panel_stop(row + 1, goal, true),
            };
            return (c, Some(goal));
        }
        let c = match m {
            Motion::Left if !extend && !sel.is_empty() => downstream(sel.start),
            Motion::Right if !extend && !sel.is_empty() => downstream(sel.end),
            Motion::Left => downstream(prev_cluster(s, i)),
            Motion::Right => downstream(next_cluster(s, i)),
            Motion::WordLeft => downstream(word_left(s, i)),
            Motion::WordRight => downstream(word_right(s, i)),
            Motion::LineStart => self.panel_stop(self.panel_spot(c).0, 0.0, true),
            Motion::LineEnd => self.panel_stop(self.panel_spot(c).0, f32::INFINITY, true),
            Motion::Up | Motion::Down => c,
        };
        (c, None)
    }

    /// The soft-wrapped row caret `c` is on, counted over the whole text, and its column.
    fn panel_spot(&self, c: Caret) -> (usize, usize) {
        let (s, rows) = (self.panel.text(), self.rows());
        let n = rows.lines.partition_point(|&(start, _)| start <= c.offset) - 1;
        let (start, first) = rows.lines[n];
        let line = &s[start..line_end(s, start)];
        let wrapped = wrap(line, rows.cols);
        let i = c.offset - start;
        let mut k = (wrapped.iter()).rposition(|r| r.start <= i).unwrap_or(0);
        if c.upstream && k > 0 && wrapped[k].start == i {
            k -= 1;
        }
        let col = line[wrapped[k].start..i].graphemes(true).map(columns).sum();
        (first + k, col)
    }

    /// The caret stop on row `row` (clamped to the text's rows) `nearest` column `col`, else
    /// before the character there. A stop at the end of a row that wraps stays on that row.
    fn panel_stop(&self, row: usize, col: f32, nearest: bool) -> Caret {
        let (s, rows) = (self.panel.text(), self.rows());
        let row = row.min(rows.total - 1);
        let n = rows.lines.partition_point(|&(_, first)| first <= row) - 1;
        let (start, first) = rows.lines[n];
        let wrapped = wrap(&s[start..line_end(s, start)], rows.cols);
        let k = row - first;
        let r = &wrapped[k];
        let (mut at, mut x) = (start + r.start, 0.0);
        for g in s[start + r.start..start + r.end].graphemes(true) {
            let w = columns(g) as f32;
            if col < x + if nearest { w / 2.0 } else { w } {
                break;
            }
            (at, x) = (at + g.len(), x + w);
        }
        Caret {
            offset: at,
            upstream: k + 1 < wrapped.len() && at == start + r.end,
        }
    }

    /// The selected panel text; `None` when nothing is selected.
    pub(super) fn panel_selected_text(&self) -> Option<String> {
        let sel = self.panel.selection();
        (!sel.is_empty()).then(|| self.panel.text()[sel].to_string())
    }

    /// What typed or marked text replaces: `replace` (floored to characters), else the
    /// marked text, else the selection.
    fn panel_typing(&self, replace: &Option<Range<usize>>) -> Range<usize> {
        let s = self.panel.text();
        let floor = |r: &Range<usize>| s.floor_char_boundary(r.start)..s.floor_char_boundary(r.end);
        (replace.as_ref().map(floor).or(self.panel.marked())).unwrap_or(self.panel.selection())
    }

    /// Commits the marked text as it is.
    pub(super) fn panel_unmark(&mut self) {
        if self.panel.edit.marked.take().is_some() {
            self.panel.edit.typed = Some(self.now);
        }
    }

    /// Replaces `range` of the panel text with `text`, caret after it: a change the writer
    /// typed, unless both are empty.
    fn panel_replace(&mut self, range: Range<usize>, text: &str) {
        let changed = !range.is_empty() || !text.is_empty();
        self.splice_panel_text(range.clone(), text);
        let at = range.start + text.len();
        let e = &mut self.panel.edit;
        (e.caret, e.anchor, e.marked, e.goal) = (downstream(at), at, None, None);
        if changed {
            e.typed = Some(self.now);
        }
    }
}

/// The end of the line starting at byte `start`, line break excluded.
fn line_end(s: &str, start: usize) -> usize {
    s[start..].find('\n').map_or(s.len(), |i| start + i)
}

fn downstream(offset: usize) -> Caret {
    Caret {
        offset,
        upstream: false,
    }
}
