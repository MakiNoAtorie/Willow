//! Text editing: the editing session, caret and text selection, replacing text through the
//! history, IME marked text and style toggles.

use std::ops::Range;
use std::time::Duration;

use super::Session;
use crate::edit::{Style, next_cluster, prev_cluster, word_left, word_right};
use crate::input::{Input, Motion, Point, Response};
use crate::layout::Caret;
use crate::{Shape, ShapeId};

/// The shape or zone title being edited and where its caret is.
#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub id: ShapeId,
    /// `id` is a zone: its title is edited, plain text on one line.
    pub zone: bool,
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
                let at = |i: usize| start + text.floor_char_boundary(i);
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
            Input::Enter => {
                if self.editing.is_some() {
                    self.stop_editing();
                } else if let Some(id) = self.only_selected() {
                    self.start_editing(id);
                } else if self.selected.is_empty() {
                    let Some(id) = self.nearest_view_center() else {
                        return self.respond(false);
                    };
                    self.select([id]);
                } else {
                    return self.respond(false);
                }
                self.respond(true)
            }
            Input::Escape => {
                if self.editing.is_some() {
                    self.stop_editing();
                } else if self.selected.is_empty() {
                    return self.respond(false);
                } else {
                    self.select([]);
                }
                self.respond(true)
            }
            Input::NewLine if self.edited().is_some() => {
                self.replace(self.text_selection(), "\n");
                self.respond(true)
            }
            Input::NewLine => self.respond(false),
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
                let text = super::clipboard::as_text(text);
                let mut text: String = (text.replace("\r\n", "\n").replace('\r', "\n").chars())
                    .filter(|&c| c == '\n' || !c.is_control())
                    .collect();
                if self.edited().is_none() {
                    text = text.replace('\n', " ");
                }
                self.replace(self.text_selection(), &text);
                self.respond(true)
            }
            Input::SelectAll => {
                let Some(s) = self.edited_text() else {
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
        let s = self.edited_text().unwrap_or_default();
        let replace = replace.map(|r| s.floor_char_boundary(r.start)..s.floor_char_boundary(r.end));
        let range = (replace.or(marked)).unwrap_or_else(|| self.text_selection());
        Some((text, self.drop_marked(range)))
    }

    /// The shape or zone being edited, if any, and its caret.
    pub fn editing(&self) -> Option<&Editing> {
        self.editing.as_ref()
    }

    /// How long after `now` the caret next changes (a blink edge, or turning solid); `None`
    /// once nothing will change by itself. The app redraws then, on a timer, not every frame.
    pub fn next_wake(&self, now: Duration) -> Option<Duration> {
        let (c, since) = (&self.theme.caret, now.saturating_sub(self.caret_since()?));
        if since >= c.idle {
            return None;
        }
        let edge = c.blink * (since.as_nanos() / c.blink.as_nanos() + 1) as u32;
        Some(edge.min(c.idle) - since)
    }

    /// When the caret last moved or its text changed: the code panel's while it has focus,
    /// else the edited text's.
    fn caret_since(&self) -> Option<Duration> {
        if self.panel.focused {
            return Some(self.panel.edit.since);
        }
        self.editing.as_ref().map(|e| e.since)
    }

    /// Whether a caret shows at `now`: off during the blink's hidden half.
    pub(super) fn caret_shown(&self, now: Duration) -> bool {
        let Some(since) = self.caret_since() else {
            return true;
        };
        let (c, since) = (&self.theme.caret, now.saturating_sub(since));
        since >= c.idle || (since.as_millis() / c.blink.as_millis()) % 2 == 0
    }

    /// Where `shape`'s text starts as drawn, in canvas units.
    fn text_origin(&self, shape: &Shape) -> Point {
        let (x, y) = self.drawn_at(shape);
        Point {
            x: x as f32 + self.theme.shape.pad_x,
            y: y as f32 + self.theme.shape.pad_top,
        }
    }

    /// The caret to draw at `now`, while editing.
    pub fn caret(&self, now: Duration) -> Option<CaretView> {
        let (e, t) = (self.editing.as_ref()?, self.theme);
        let (pos, height) = if e.zone {
            (self.title_point(e.caret.offset)?, self.title_line())
        } else {
            let shape = self.doc.shapes.iter().find(|s| s.id == e.id)?;
            let (p, o) = (
                self.layout(shape).caret_point(e.caret),
                self.text_origin(shape),
            );
            let pos = Point {
                x: o.x + p.x,
                y: o.y + p.y,
            };
            (pos, t.line_height())
        };
        Some(CaretView {
            pos,
            height,
            visible: self.caret_shown(now),
        })
    }

    /// Top of the caret stop at byte `offset` of the edited text, in canvas units; the line
    /// is `Theme::line_height()` tall (a title's, `CaretView.height`). The app places the IME
    /// candidate window with it.
    pub fn text_point(&self, offset: usize) -> Option<Point> {
        if self.editing.as_ref()?.zone {
            return self.title_point(offset);
        }
        let (shape, _) = self.edited()?;
        let c = Caret {
            offset,
            upstream: false,
        };
        let (p, o) = (self.layout(shape).caret_point(c), self.text_origin(shape));
        Some(Point {
            x: o.x + p.x,
            y: o.y + p.y,
        })
    }

    /// Whether Cut and Copy have something to act on: text selected in the code panel while
    /// it has focus, else in the edited shape, or with nothing edited, selected shapes.
    pub fn can_cut(&self) -> bool {
        if self.panel.focused {
            return !self.panel.selection().is_empty();
        }
        !self.text_selection().is_empty() || (self.editing.is_none() && !self.selected.is_empty())
    }

    pub fn can_copy(&self) -> bool {
        self.can_cut()
    }

    /// Whether Paste has somewhere to go: the code panel, the edited shape or the canvas.
    pub fn can_paste(&self) -> bool {
        true
    }

    /// The shape whose center is nearest the view center; ties go in generation order.
    fn nearest_view_center(&self) -> Option<ShapeId> {
        let c = self.viewport.center();
        let dist = |i: usize| {
            let p = self.drawn_index(i).center();
            (p.x - c.x).hypot(p.y - c.y)
        };
        (self.doc.shapes.iter().enumerate())
            .min_by(|&(i, a), &(j, b)| {
                (dist(i).total_cmp(&dist(j)))
                    .then_with(|| a.id.generation_order().cmp(&b.id.generation_order()))
            })
            .map(|(_, s)| s.id.clone())
    }

    /// Opens an editing session on shape `id`, or zone `id`'s title, caret at the end.
    pub(super) fn start_editing(&mut self, id: ShapeId) {
        let shape = self.doc.shapes.iter().find(|s| s.id == id);
        let end = match shape {
            Some(shape) => shape.text.plain().len(),
            None => match self.doc.zones.iter().find(|z| z.id == id) {
                Some(z) => z.title.len(),
                None => return,
            },
        };
        self.history.begin();
        self.editing = Some(Editing {
            zone: shape.is_none(),
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

    /// The edited shape and its plain text; `None` while a zone title is edited.
    pub(super) fn edited(&self) -> Option<(&Shape, String)> {
        let e = self.editing.as_ref()?;
        let shape = self.doc.shapes.iter().find(|s| s.id == e.id)?;
        Some((shape, shape.text.plain()))
    }

    /// Whether marked text sits in the document: saved data waits until it doesn't.
    pub(super) fn composing(&self) -> bool {
        self.editing.as_ref().is_some_and(|e| e.marked.is_some())
    }

    /// The edited text: the edited shape's plain text or zone's title. What offsets index.
    pub fn edited_text(&self) -> Option<String> {
        let e = self.editing.as_ref()?;
        if e.zone {
            let z = self.doc.zones.iter().find(|z| z.id == e.id)?;
            return Some(z.title.clone());
        }
        self.edited().map(|(_, s)| s)
    }

    /// Top of the caret stop at byte `offset` of the edited title, in canvas units: the top of
    /// its line box, `title_line()` tall, the title's em box centered in it as shape text's is.
    fn title_point(&self, offset: usize) -> Option<Point> {
        let e = self.editing.as_ref()?;
        let z = self.doc.zones.iter().find(|z| z.id == e.id)?;
        let at = self.title_at(z);
        let size = self.title_size();
        let offset = z.title.floor_char_boundary(offset);
        Some(Point {
            x: at.x + self.title_width(&z.title[..offset], size),
            y: at.y,
        })
    }

    /// Under the edited title's `range`, at its em box's bottom: from and to, canvas units.
    pub(crate) fn title_underline(&self, range: Range<usize>) -> Option<(Point, Point)> {
        let (a, b) = (self.title_point(range.start)?, self.title_point(range.end)?);
        let y = a.y + (self.title_line() + self.title_size()) / 2.0;
        Some((Point { y, ..a }, Point { y, ..b }))
    }

    /// The edited title's line height in canvas units.
    fn title_line(&self) -> f32 {
        self.title_size() * self.theme.line_height
    }

    /// The caret nearest screen point `p` in the edited shape.
    pub(super) fn caret_at(&self, p: Point) -> Caret {
        let Some((shape, _)) = self.edited() else {
            return Caret::default();
        };
        self.layout(shape)
            .caret_at(self.viewport.to_canvas(p) - self.text_origin(shape))
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
        let sel = self.text_selection();
        if self.editing.as_ref().is_some_and(|e| e.zone) {
            let (a, b) = (self.title_point(sel.start), self.title_point(sel.end));
            let rect = (a.zip(b)).map(|(a, b)| {
                (
                    a,
                    Point {
                        y: b.y + self.title_line(),
                        ..b
                    },
                )
            });
            return rect.filter(|_| !sel.is_empty()).into_iter().collect();
        }
        let Some((shape, _)) = self.edited() else {
            return Vec::new();
        };
        let (origin, lh) = (self.text_origin(shape), self.theme.line_height());
        let o = |p: Point| Point {
            x: origin.x + p.x,
            y: origin.y + p.y,
        };
        let rects = self.layout(shape).selection_rects(sel, lh);
        rects.into_iter().map(|(a, b)| (o(a), o(b))).collect()
    }

    /// The selected text of the edited shape as plain text (`\n` between paragraphs);
    /// `None` when nothing is selected. What Copy puts on the clipboard.
    pub fn selected_text(&self) -> Option<String> {
        let s = self.edited_text()?;
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
        let s = self.edited_text()?;
        let i = e.caret.offset;
        let sel = if extend { i..i } else { self.text_selection() };
        let at = |offset| Caret {
            offset,
            upstream: false,
        };
        let c = match m {
            Motion::Left if !sel.is_empty() => at(sel.start),
            Motion::Right if !sel.is_empty() => at(sel.end),
            Motion::Left => at(prev_cluster(&s, i)),
            Motion::Right => at(next_cluster(&s, i)),
            Motion::WordLeft => at(word_left(&s, i)),
            Motion::WordRight => at(word_right(&s, i)),
            // A title is one line.
            Motion::Up | Motion::LineStart if e.zone => at(0),
            Motion::Down | Motion::LineEnd if e.zone => at(s.len()),
            Motion::Up | Motion::Down => {
                let l = self.layout(self.edited()?.0);
                let goal = e.goal_x.unwrap_or_else(|| l.caret_point(e.caret).x);
                return Some((l.vertical(e.caret, goal, m == Motion::Down), Some(goal)));
            }
            Motion::LineStart => self.layout(self.edited()?.0).line_bounds(e.caret).0,
            Motion::LineEnd => self.layout(self.edited()?.0).line_bounds(e.caret).1,
        };
        Some((c, None))
    }

    /// Replaces `range` of the edited text with `text`; the caret lands after it.
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let Some(e) = &self.editing else { return };
        let (id, pending, mut end) = (e.id.clone(), e.pending, range.start);
        if e.zone {
            (self.history).change_zone(&mut self.doc, &id, |z| {
                z.title.replace_range(range.clone(), text);
                end = range.start + text.len();
            });
            return self.set_caret(Caret {
                offset: end,
                upstream: false,
            });
        }
        (self.history).change(&mut self.doc, &id, |s| {
            end = s.text.replace(range.clone(), text);
            if let Some((bold, italic)) = pending {
                s.text.set_style(range.start..end, Style::Bold, bold);
                s.text.set_style(range.start..end, Style::Italic, italic);
            }
        });
        self.set_caret(Caret {
            offset: end,
            upstream: false,
        });
    }

    /// Changes the edited text outside the history, in the pending style: only marked text
    /// goes this way.
    fn replace_outside_history(&mut self, range: Range<usize>, text: &str) {
        let Some(e) = &self.editing else { return };
        if e.zone {
            if let Some(z) = self.doc.zones.iter_mut().find(|z| z.id == e.id) {
                z.title.replace_range(range, text);
            }
        } else if let Some(i) = self.doc.shapes.iter().position(|s| s.id == e.id) {
            let s = &mut self.doc.shapes[i];
            let end = s.text.replace(range.clone(), text);
            if let Some((bold, italic)) = e.pending {
                s.text.set_style(range.start..end, Style::Bold, bold);
                s.text.set_style(range.start..end, Style::Italic, italic);
            }
            self.spaced.get_mut().composed.push(i);
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
        let text = self.edited_text().map(|s| s[m.clone()].to_string());
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

    /// Selects `range` of the edited text, caret at its end; ends past the text or inside a
    /// character move back to the nearest boundary.
    pub fn set_text_selection(&mut self, range: Range<usize>) {
        let s = self.edited_text().unwrap_or_default();
        self.set_caret(Caret {
            offset: s.floor_char_boundary(range.start),
            upstream: false,
        });
        if let Some(e) = &mut self.editing {
            e.caret.offset = s.floor_char_boundary(range.end);
        }
    }
}
