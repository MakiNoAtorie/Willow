//! Text editing: the editing session, caret and text selection, replacing text through the
//! history, IME marked text and style toggles.

use std::ops::Range;
use std::time::Duration;

use super::Session;
use crate::edit::{Style, next_cluster, prev_cluster, word_left, word_right};
use crate::input::{Input, Motion, Point, Response};
use crate::layout::Caret;
use crate::{Shape, ShapeId};

/// The shape being edited and where its caret is.
#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub id: ShapeId,
    pub caret: Caret,
    /// The text selection's other end; `caret.offset` when nothing is selected.
    pub anchor: usize,
    /// IME composition shown in the text, not yet committed: no undo step holds it.
    pub marked: Option<Range<usize>>,
    /// Column that up/down keep, set by the first vertical move.
    goal_x: Option<f32>,
    /// (bold, italic) for the next typed text, set by a toggle with nothing selected; any
    /// caret move drops it.
    pending: Option<(bool, bool)>,
    /// Last edit input: the caret blinks from here until `CaretStyle::idle`.
    since: Duration,
}

/// The caret to draw: top in canvas units, one line tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretView {
    pub pos: Point,
    pub height: f32,
    /// Off during the blink's hidden half.
    pub visible: bool,
}

impl Session {
    /// Handles a text-editing input: typing, IME, caret motion, selection, clipboard, style.
    pub(super) fn handle_text(&mut self, input: Input) -> Response {
        match input {
            Input::Text { text, replace } => {
                let Some((text, range)) = self.typing(&text, replace) else {
                    return self.respond(false);
                };
                self.replace(range, &text);
                self.respond(true)
            }
            Input::Mark {
                text,
                replace,
                selected,
            } => {
                let Some((text, range)) = self.typing(&text, replace) else {
                    return self.respond(false);
                };
                // Text the composition replaces outside the marked text is a real edit.
                let pending = self.editing.as_ref().and_then(|e| e.pending);
                self.replace(range.clone(), "");
                if let Some(e) = &mut self.editing {
                    e.pending = pending;
                }
                let start = range.start;
                self.replace_outside_history(start..start, &text);
                let sel = selected.unwrap_or(text.len()..text.len());
                let at = |i: usize| start + i.min(text.len());
                self.set_caret(Caret {
                    offset: at(sel.end),
                    upstream: false,
                });
                if let Some(e) = &mut self.editing {
                    (e.anchor, e.pending) = (at(sel.start), pending);
                    e.marked = (!text.is_empty()).then(|| start..start + text.len());
                }
                self.respond(true)
            }
            Input::Unmark => self.respond(true),
            Input::StartEditing => match self.only_selected() {
                Some(id) if self.editing.is_none() => {
                    self.start_editing(id);
                    self.respond(true)
                }
                _ => self.respond(false),
            },
            Input::StopEditing => {
                let was = self.editing.is_some();
                self.stop_editing();
                self.respond(was)
            }
            Input::Select(m) => {
                let Some((caret, goal)) = self.motion(m, true) else {
                    return self.respond(false);
                };
                self.extend_to(caret, goal);
                self.respond(true)
            }
            Input::Cut => {
                let sel = self.text_selection();
                if sel.is_empty() {
                    return self.respond(false);
                }
                self.replace(sel, "");
                self.respond(true)
            }
            Input::Paste(text) => {
                if self.editing.is_none() {
                    return self.respond(false);
                }
                let text: String = (text.replace("\r\n", "\n").replace('\r', "\n").chars())
                    .filter(|&c| c == '\n' || !c.is_control())
                    .collect();
                self.replace(self.text_selection(), &text);
                self.respond(true)
            }
            Input::SelectAll => {
                let Some((_, s)) = self.edited() else {
                    return self.respond(false);
                };
                self.set_text_selection(0..s.len());
                self.respond(true)
            }
            Input::ToggleBold => {
                self.toggle(Style::Bold);
                self.respond(true)
            }
            Input::ToggleItalic => {
                self.toggle(Style::Italic);
                self.respond(true)
            }
            Input::Move(m) => {
                let Some((caret, goal)) = self.motion(m, false) else {
                    return self.respond(false);
                };
                self.set_caret(caret);
                if let Some(e) = &mut self.editing {
                    e.goal_x = goal;
                }
                self.respond(true)
            }
            Input::Delete(m) => {
                let sel = self.text_selection();
                let range = match self.motion(m, false) {
                    _ if !sel.is_empty() => sel,
                    Some((to, _)) => sel.start.min(to.offset)..sel.start.max(to.offset),
                    None => return self.respond(false),
                };
                self.replace(range, "");
                self.respond(true)
            }
            _ => self.respond(false),
        }
    }

    /// The shared start of `Text` and `Mark`: drops control characters, starts editing the
    /// selected shape on anything but whitespace, and takes the marked text out. Returns the
    /// text and the range it replaces: `replace`, else the marked text, else the text
    /// selection; `None` when there is nothing to type into.
    fn typing(
        &mut self,
        text: &str,
        mut replace: Option<Range<usize>>,
    ) -> Option<(String, Range<usize>)> {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.editing.is_none() {
            match self.only_selected() {
                Some(id) if !text.trim().is_empty() => self.start_editing(id),
                _ => return None,
            }
            // A range given before editing started indexes nothing: append.
            replace = None;
        }
        let marked = self.editing.as_ref().and_then(|e| e.marked.clone());
        let range = (replace.or(marked)).unwrap_or_else(|| self.text_selection());
        Some((text, self.drop_marked(range)))
    }

    /// The shape being edited, if any, and its caret.
    pub fn editing(&self) -> Option<&Editing> {
        self.editing.as_ref()
    }

    /// How long after `now` the caret next changes (a blink edge, or turning solid); `None`
    /// once nothing will change by itself. The app redraws then, on a timer, not every frame.
    pub fn next_wake(&self, now: Duration) -> Option<Duration> {
        let e = self.editing.as_ref()?;
        let (c, since) = (&self.theme.caret, now.saturating_sub(e.since));
        if since >= c.idle {
            return None;
        }
        let edge = c.blink * (since.as_nanos() / c.blink.as_nanos() + 1) as u32;
        Some(edge.min(c.idle) - since)
    }

    fn blinking(&self, now: Duration) -> bool {
        (self.editing.as_ref()).is_some_and(|e| now.saturating_sub(e.since) < self.theme.caret.idle)
    }

    /// The caret to draw at `now`, while editing.
    pub fn caret(&self, now: Duration) -> Option<CaretView> {
        let e = self.editing.as_ref()?;
        let shape = self.doc.shapes.iter().find(|s| s.id == e.id)?;
        let p = self.layout(shape).caret_point(e.caret);
        let (t, since) = (self.theme, now.saturating_sub(e.since));
        Some(CaretView {
            pos: Point {
                x: shape.x as f32 + t.shape.pad_x + p.x,
                y: shape.y as f32 + t.shape.pad_y + p.y,
            },
            height: t.line_height(),
            visible: !self.blinking(now)
                || (since.as_millis() / t.caret.blink.as_millis()) % 2 == 0,
        })
    }

    /// Top of the caret stop at byte `offset` of the edited text, in canvas units; the line
    /// is `Theme::line_height()` tall. The app places the IME candidate window with it.
    pub fn text_point(&self, offset: usize) -> Option<Point> {
        let (shape, _) = self.edited()?;
        let c = Caret {
            offset,
            upstream: false,
        };
        let p = self.layout(shape).caret_point(c);
        let pad = &self.theme.shape;
        Some(Point {
            x: shape.x as f32 + pad.pad_x + p.x,
            y: shape.y as f32 + pad.pad_y + p.y,
        })
    }

    /// Whether Cut and Copy have something to act on: text selected in the edited shape.
    pub fn can_cut(&self) -> bool {
        !self.text_selection().is_empty()
    }

    pub fn can_copy(&self) -> bool {
        self.can_cut()
    }

    /// Whether Paste has somewhere to go: a shape being edited.
    pub fn can_paste(&self) -> bool {
        self.editing.is_some()
    }

    /// Opens an editing session on `id` with the caret at the end of its text.
    pub(super) fn start_editing(&mut self, id: ShapeId) {
        let Some(shape) = self.doc.shapes.iter().find(|s| s.id == id) else {
            return;
        };
        let end = shape.text.plain().len();
        self.history.begin();
        self.editing = Some(Editing {
            id,
            caret: Caret {
                offset: end,
                upstream: false,
            },
            anchor: end,
            marked: None,
            goal_x: None,
            pending: None,
            since: self.now,
        });
    }

    /// Ends the editing session, if any: its changes become one undo step.
    pub(super) fn stop_editing(&mut self) {
        if self.editing.take().is_some() {
            self.history.end(&self.doc);
            self.over_text = false;
        }
    }

    /// The edited shape and its plain text.
    pub(super) fn edited(&self) -> Option<(&Shape, String)> {
        let e = self.editing.as_ref()?;
        let shape = self.doc.shapes.iter().find(|s| s.id == e.id)?;
        Some((shape, shape.text.plain()))
    }

    /// The caret nearest screen point `p` in the edited shape.
    pub(super) fn caret_at(&self, p: Point) -> Caret {
        let Some((shape, _)) = self.edited() else {
            return Caret::default();
        };
        let (c, pad) = (self.viewport.to_canvas(p), &self.theme.shape);
        self.layout(shape).caret_at(Point {
            x: c.x - shape.x as f32 - pad.pad_x,
            y: c.y - shape.y as f32 - pad.pad_y,
        })
    }

    /// Puts the caret at `c` with nothing selected, and restarts the blink.
    pub(super) fn set_caret(&mut self, c: Caret) {
        if let Some(e) = &mut self.editing {
            (e.caret, e.anchor, e.goal_x, e.pending, e.since) = (c, c.offset, None, None, self.now);
        }
    }

    /// Moves the caret to `c` keeping the anchor, so the selection grows or shrinks.
    pub(super) fn extend_to(&mut self, c: Caret, goal_x: Option<f32>) {
        if let Some(e) = &mut self.editing {
            (e.caret, e.goal_x, e.pending, e.since) = (c, goal_x, None, self.now);
        }
    }

    /// Text selection highlight in canvas units, one (top-left, bottom-right) per line.
    pub fn selection_rects(&self) -> Vec<(Point, Point)> {
        let Some((shape, _)) = self.edited() else {
            return Vec::new();
        };
        let (pad, lh) = (&self.theme.shape, self.theme.line_height());
        let o = |p: Point| Point {
            x: shape.x as f32 + pad.pad_x + p.x,
            y: shape.y as f32 + pad.pad_y + p.y,
        };
        let rects = self
            .layout(shape)
            .selection_rects(self.text_selection(), lh);
        rects.into_iter().map(|(a, b)| (o(a), o(b))).collect()
    }

    /// The selected text of the edited shape as plain text (`\n` between paragraphs);
    /// `None` when nothing is selected. What Copy puts on the clipboard.
    pub fn selected_text(&self) -> Option<String> {
        let (_, s) = self.edited()?;
        let r = self.text_selection();
        (!r.is_empty()).then(|| s[r].to_string())
    }

    fn text_selection(&self) -> Range<usize> {
        self.editing.as_ref().map_or(0..0, |e| {
            e.anchor.min(e.caret.offset)..e.anchor.max(e.caret.offset)
        })
    }

    /// Where motion `m` takes the caret, and the column up/down keep. Unless `extend`ing,
    /// left and right collapse a selection to its edge.
    fn motion(&self, m: Motion, extend: bool) -> Option<(Caret, Option<f32>)> {
        let e = self.editing.as_ref()?;
        let (shape, s) = self.edited()?;
        let i = e.caret.offset;
        let sel = if extend { i..i } else { self.text_selection() };
        let at = |offset| Caret {
            offset,
            upstream: false,
        };
        let l = self.layout(shape);
        let c = match m {
            Motion::Left if !sel.is_empty() => at(sel.start),
            Motion::Right if !sel.is_empty() => at(sel.end),
            Motion::Left => at(prev_cluster(&s, i)),
            Motion::Right => at(next_cluster(&s, i)),
            Motion::WordLeft => at(word_left(&s, i)),
            Motion::WordRight => at(word_right(&s, i)),
            Motion::Up | Motion::Down => {
                let goal = e.goal_x.unwrap_or_else(|| l.caret_point(e.caret).x);
                return Some((l.vertical(e.caret, goal, m == Motion::Down), Some(goal)));
            }
            Motion::LineStart => l.line_bounds(e.caret).0,
            Motion::LineEnd => l.line_bounds(e.caret).1,
        };
        Some((c, None))
    }

    /// Replaces `range` of the edited text with `text`; the caret lands after it.
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let Some(e) = &self.editing else { return };
        let (id, pending, mut end) = (e.id.clone(), e.pending, range.start);
        let old = self.rect_of(&id);
        (self.history).change(&mut self.doc, &id, |s| {
            end = s.text.replace(range.clone(), text);
            if let Some((bold, italic)) = pending {
                s.text.set_style(range.start..end, Style::Bold, bold);
                s.text.set_style(range.start..end, Style::Italic, italic);
            }
        });
        if let Some(old) = old {
            self.make_room(&id, old);
        }
        self.set_caret(Caret {
            offset: end,
            upstream: false,
        });
    }

    /// Changes the edited text outside the history, in the pending style: only marked text
    /// goes this way.
    fn replace_outside_history(&mut self, range: Range<usize>, text: &str) {
        let Some(e) = &self.editing else { return };
        if let Some(s) = self.doc.shapes.iter_mut().find(|s| s.id == e.id) {
            let end = s.text.replace(range.clone(), text);
            if let Some((bold, italic)) = e.pending {
                s.text.set_style(range.start..end, Style::Bold, bold);
                s.text.set_style(range.start..end, Style::Italic, italic);
            }
        }
    }

    /// Takes the marked text out of the document; returns `r` as it indexes the text without it.
    fn drop_marked(&mut self, r: Range<usize>) -> Range<usize> {
        let Some(m) = self.editing.as_mut().and_then(|e| e.marked.take()) else {
            return r;
        };
        self.replace_outside_history(m.clone(), "");
        let f = |i: usize| {
            if i >= m.end {
                i - m.len()
            } else {
                i.min(m.start)
            }
        };
        f(r.start)..f(r.end)
    }

    /// Commits the marked text as it is, through the history.
    pub(super) fn unmark(&mut self) {
        let Some(m) = self.editing.as_ref().and_then(|e| e.marked.clone()) else {
            return;
        };
        let text = self.edited().map(|(_, s)| s[m.clone()].to_string());
        let at = self.drop_marked(m.start..m.start);
        self.replace(at, &text.unwrap_or_default());
    }

    /// Toggles `style` on the text selection, or in the pending style.
    fn toggle(&mut self, style: Style) {
        let Some(e) = &self.editing else { return };
        let (id, sel) = (e.id.clone(), self.text_selection());
        if sel.is_empty() {
            let Some((shape, _)) = self.edited() else {
                return;
            };
            let mut st = e.pending.unwrap_or_else(|| shape.text.style_at(sel.start));
            *style.of(&mut st) ^= true;
            if let Some(e) = &mut self.editing {
                e.pending = Some(st);
            }
            return;
        }
        (self.history).change(&mut self.doc, &id, |s| {
            let on = !s.text.all_styled(sel.clone(), style);
            s.text.set_style(sel, style, on);
        });
    }

    /// Selects `range` of the edited text, caret at its end.
    pub fn set_text_selection(&mut self, range: Range<usize>) {
        self.set_caret(Caret {
            offset: range.start,
            upstream: false,
        });
        if let Some(e) = &mut self.editing {
            e.caret.offset = range.end;
        }
    }
}
