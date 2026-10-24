//! The editor session: one per window. Owns the document and the viewport, takes plain input
//! events with the time passed in, and tells the app whether to redraw and which cursor to show.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use crate::edit::{next_cluster, prev_cluster, word_left, word_right};
use crate::history::History;
use crate::layout::{Caret, Layout, Measure, layout};
use crate::zoom::{Zoom, ZoomPill};
use crate::{Document, RichText, Shape, ShapeId, Theme};

/// A point in screen pixels (relative to the canvas's top-left) or in canvas units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Cmd on macOS, the Windows key on Windows.
    pub platform: bool,
}

/// Keys the session cares about; the app drops the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Space,
}

/// Where a caret motion goes. The app maps platform keys to these (word jumps are Alt+arrow
/// on macOS, Ctrl+arrow on Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    /// One line up or down, keeping the column.
    Up,
    Down,
    LineStart,
    LineEnd,
}

/// One input event. Positions are screen pixels relative to the canvas's top-left.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    PointerDown {
        pos: Point,
        button: Button,
        mods: Modifiers,
    },
    PointerMove {
        pos: Point,
        mods: Modifiers,
    },
    PointerUp {
        pos: Point,
        button: Button,
        mods: Modifiers,
    },
    /// Wheel or trackpad scroll at `pos`: how far the content should move, in screen pixels.
    Wheel {
        pos: Point,
        delta: Point,
        mods: Modifiers,
    },
    KeyDown(Key),
    KeyUp(Key),
    /// Trackpad pinch around `pos`: `delta` 0.1 scales by 1.1.
    Pinch {
        pos: Point,
        delta: f32,
    },
    /// Ctrl/Cmd+0: back to 100% around the canvas center.
    ResetZoom,
    /// Undoes the last document change. Viewport and selection are untouched.
    Undo,
    Redo,
    /// Typed text, replacing byte range `replace` of the edited text, or else the text
    /// selection. On a selected shape that isn't being edited, anything but a space starts
    /// editing and is appended. Control characters are dropped.
    Text {
        text: String,
        replace: Option<Range<usize>>,
    },
    /// F2: edit the selected shape, caret at the end.
    StartEditing,
    /// Enter or Esc: stop editing, keep the shape selected.
    StopEditing,
    /// Ctrl/Cmd+B: toggles bold on the text selection, or for the next typed text.
    ToggleBold,
    /// Ctrl/Cmd+I: the same for italic.
    ToggleItalic,
    Move(Motion),
    /// Deletes the text selection, or else from the caret to where the motion goes.
    Delete(Motion),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Arrow,
    /// Space held, ready to pan.
    Grab,
    /// Panning.
    Grabbing,
    /// Over the text being edited.
    Text,
}

/// What the app does after an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub redraw: bool,
    pub cursor: Cursor,
    /// An animation is running: keep requesting frames.
    pub animating: bool,
}

/// Which part of the canvas the screen shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Canvas point at the screen's top-left.
    pub origin: Point,
    /// Screen pixels per canvas unit.
    pub zoom: f32,
    /// Screen size in pixels.
    pub size: Point,
}

impl Viewport {
    pub fn to_screen(&self, p: Point) -> Point {
        Point {
            x: (p.x - self.origin.x) * self.zoom,
            y: (p.y - self.origin.y) * self.zoom,
        }
    }

    pub fn to_canvas(&self, p: Point) -> Point {
        Point {
            x: p.x / self.zoom + self.origin.x,
            y: p.y / self.zoom + self.origin.y,
        }
    }

    /// Moves the content by `d` screen pixels.
    fn pan(&mut self, d: Point) {
        self.origin.x -= d.x / self.zoom;
        self.origin.y -= d.y / self.zoom;
    }
}

/// The shape being edited and where its caret is.
#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub id: ShapeId,
    pub caret: Caret,
    /// The text selection's other end; `caret.offset` when nothing is selected.
    pub anchor: usize,
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

/// Screen pixels the pointer may wander before a press counts as a drag.
const DRAG_START: f32 = 3.0;

/// A pointer gesture in progress.
#[derive(Clone, Copy, Debug)]
enum Gesture {
    /// `click` is the press point while a left press on empty canvas could still be a click,
    /// which deselects on release.
    Pan {
        button: Button,
        last: Point,
        click: Option<Point>,
    },
    /// Left press on the shape at `index`. Once dragged past `DRAG_START`, `to` is the
    /// previewed top-left; release then moves the shape there instead of selecting it.
    Press {
        index: usize,
        from: Point,
        to: Option<(i32, i32)>,
    },
    /// Left press in the text being edited: never moves the shape.
    // ponytail: dragging will select text (#11).
    Text,
}

/// A selection ring easing from `from` to `to` (0 hidden, 1 shown), starting at `since`.
#[derive(Clone, Debug)]
struct Ring {
    id: ShapeId,
    from: f32,
    to: f32,
    since: Duration,
}

impl Ring {
    fn progress(&self, now: Duration, fade: Duration) -> f32 {
        (now.saturating_sub(self.since).as_secs_f32() / fade.as_secs_f32()).min(1.0)
    }

    fn value(&self, now: Duration, fade: Duration) -> f32 {
        let p = self.progress(now, fade);
        // ponytail: quadratic ease-out stands in for CSS `ease-out`; use the theme's cubic-bezier
        // once the core has one (#7's zoom easing).
        self.from + (self.to - self.from) * (1.0 - (1.0 - p) * (1.0 - p))
    }
}

/// A shape's layout and the text and width it was made for.
type Cached = (RichText, u32, Rc<Layout>);

pub struct Session {
    doc: Document,
    history: History,
    theme: &'static Theme,
    measure: Box<dyn Measure>,
    layouts: RefCell<HashMap<ShapeId, Cached>>,
    viewport: Viewport,
    gesture: Option<Gesture>,
    space_held: bool,
    zoom: Zoom,
    selected: Option<ShapeId>,
    editing: Option<Editing>,
    /// The pointer is over the text being edited.
    over_text: bool,
    rings: Vec<Ring>,
    /// Time of the last event.
    now: Duration,
}

impl Session {
    /// `measure` is the app's text-measuring function.
    pub fn new(doc: Document, theme: &'static Theme, measure: Box<dyn Measure>) -> Self {
        Session {
            doc,
            history: History::default(),
            theme,
            measure,
            layouts: RefCell::default(),
            viewport: Viewport {
                origin: Point::default(),
                zoom: 1.0,
                size: Point::default(),
            },
            gesture: None,
            space_held: false,
            zoom: Zoom::default(),
            selected: None,
            editing: None,
            over_text: false,
            rings: Vec::new(),
            now: Duration::ZERO,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn theme(&self) -> &'static Theme {
        self.theme
    }

    /// The selected shape (at most one in Phase 2).
    pub fn selection(&self) -> Option<&ShapeId> {
        self.selected.as_ref()
    }

    /// The shape being edited, if any, and its caret.
    pub fn editing(&self) -> Option<&Editing> {
        self.editing.as_ref()
    }

    /// `shape`'s text layout, positioned from the text's top-left inside the padding. Cached
    /// per shape; recomputed only when its text or width changed.
    pub fn layout(&self, shape: &Shape) -> Rc<Layout> {
        let mut cache = self.layouts.borrow_mut();
        if let Some((text, width, l)) = cache.get(&shape.id)
            && *text == shape.text
            && *width == shape.width
        {
            return l.clone();
        }
        let width = (shape.width as f32 - 2.0 * self.theme.shape.pad_x).max(0.0);
        let l = Rc::new(layout(&shape.text, width, self.theme, &*self.measure));
        cache.insert(
            shape.id.clone(),
            (shape.text.clone(), shape.width, l.clone()),
        );
        l
    }

    /// Drawn height in canvas units: text, padding and extra height.
    pub fn shape_height(&self, shape: &Shape) -> f32 {
        self.layout(shape).height + 2.0 * self.theme.shape.pad_y + shape.extra_height as f32
    }

    /// Selection rings to draw at `now`: shape and ring opacity, 0 to 1. Includes rings
    /// still fading out after a deselect.
    pub fn selection_rings(&self, now: Duration) -> Vec<(&ShapeId, f32)> {
        let fade = self.theme.selection.fade;
        self.rings
            .iter()
            .filter(|r| r.to > 0.0 || r.progress(now, fade) < 1.0)
            .map(|r| (&r.id, r.value(now, fade)))
            .collect()
    }

    /// Whether anything is still animating at `now`; the app keeps requesting frames until not.
    pub fn animating(&self, now: Duration) -> bool {
        let fade = self.theme.selection.fade;
        self.rings.iter().any(|r| r.progress(now, fade) < 1.0)
            || self.zoom.running(self.theme, now)
            || self.blinking(now)
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

    fn select(&mut self, id: Option<ShapeId>) {
        if id == self.selected {
            return;
        }
        let (now, fade) = (self.now, self.theme.selection.fade);
        let old = std::mem::replace(&mut self.selected, id.clone());
        for (id, to) in [(old, 0.0), (id, 1.0)] {
            let Some(id) = id else { continue };
            let from = match self.rings.iter().position(|r| r.id == id) {
                Some(i) => self.rings.swap_remove(i).value(now, fade),
                None => 0.0,
            };
            self.rings.push(Ring {
                id,
                from,
                to,
                since: now,
            });
        }
        // Drop rings that have finished fading out.
        self.rings
            .retain(|r| r.to > 0.0 || r.progress(now, fade) < 1.0);
    }

    /// The shape being dragged and its previewed top-left; the document still holds the old one.
    pub fn drag_preview(&self) -> Option<(&ShapeId, i32, i32)> {
        match self.gesture {
            Some(Gesture::Press {
                index,
                to: Some((x, y)),
                ..
            }) => Some((&self.doc.shapes[index].id, x, y)),
            _ => None,
        }
    }

    /// Moves a shape's top-left to (x, y): one discrete change to the document.
    pub fn move_shape(&mut self, id: &ShapeId, x: i32, y: i32) {
        self.history
            .change(&mut self.doc, id, |s| (s.x, s.y) = (x, y));
    }

    /// Whether there is a document change to undo; the Undo menu item enables from it.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    /// The canvas is now `width` x `height` screen pixels. The first call centers the first shape
    /// on whole pixels; later calls keep the top-left in place.
    pub fn resize(&mut self, width: f32, height: f32) -> Response {
        let first = self.viewport.size == Point::default();
        self.viewport.size = Point {
            x: width,
            y: height,
        };
        if first && let Some(s) = self.doc.shapes.first() {
            let (w, h) = (s.width as f32, self.shape_height(s));
            self.viewport.origin = Point {
                x: s.x as f32 - ((width - w) / 2.0).round(),
                y: s.y as f32 - ((height - h) / 2.0).round(),
            };
        }
        self.respond(true)
    }

    /// Handles one input event at time `now` (any monotonic clock).
    pub fn handle(&mut self, input: Input, now: Duration) -> Response {
        self.now = now;
        match input {
            Input::PointerDown { pos, button, .. } => {
                let hit = self.shape_at(pos);
                if self.gesture.is_none()
                    && let Some(e) = &self.editing
                {
                    // Panning is ignored while editing; a left press leaves editing unless it
                    // lands in the edited text.
                    if button != Button::Left {
                        return self.respond(false);
                    }
                    if hit.is_some_and(|i| self.doc.shapes[i].id == e.id) {
                        let caret = self.caret_at(pos);
                        self.set_caret(caret);
                        self.gesture = Some(Gesture::Text);
                        return self.respond(true);
                    }
                    self.stop_editing();
                    if hit.is_none() {
                        return self.respond(true);
                    }
                }
                let gesture = match (button, hit) {
                    _ if self.gesture.is_some() => None,
                    (Button::Left, Some(index)) if !self.space_held => Some(Gesture::Press {
                        index,
                        from: pos,
                        to: None,
                    }),
                    (Button::Left | Button::Middle, hit) => Some(Gesture::Pan {
                        button,
                        last: pos,
                        click: (button == Button::Left && hit.is_none()).then_some(pos),
                    }),
                    (Button::Right, _) => None,
                };
                let pans = matches!(gesture, Some(Gesture::Pan { .. }));
                if gesture.is_some() {
                    self.gesture = gesture;
                }
                self.respond(pans)
            }
            Input::PointerMove { pos, .. } => match &mut self.gesture {
                Some(Gesture::Pan { last, click, .. }) => {
                    if click.is_some_and(|c| (pos.x - c.x).hypot(pos.y - c.y) >= DRAG_START) {
                        *click = None;
                    }
                    let d = Point {
                        x: pos.x - last.x,
                        y: pos.y - last.y,
                    };
                    *last = pos;
                    self.viewport.pan(d);
                    self.respond(true)
                }
                Some(Gesture::Press { index, from, to }) => {
                    let d = Point {
                        x: pos.x - from.x,
                        y: pos.y - from.y,
                    };
                    if to.is_none() && d.x.hypot(d.y) < DRAG_START {
                        return self.respond(false);
                    }
                    let (s, z) = (&self.doc.shapes[*index], self.viewport.zoom);
                    *to = Some((
                        (s.x as f32 + d.x / z).round() as i32,
                        (s.y as f32 + d.y / z).round() as i32,
                    ));
                    self.respond(true)
                }
                Some(Gesture::Text) => self.respond(false),
                None => {
                    let over = self.editing.as_ref().is_some_and(|e| {
                        (self.shape_at(pos)).is_some_and(|i| self.doc.shapes[i].id == e.id)
                    });
                    let changed = over != self.over_text;
                    self.over_text = over;
                    self.respond(changed)
                }
            },
            Input::PointerUp { button, .. } => {
                match self.gesture {
                    Some(Gesture::Pan {
                        button: b, click, ..
                    }) if b == button => {
                        self.gesture = None;
                        if click.is_some() {
                            self.select(None);
                        }
                    }
                    Some(Gesture::Press { index, to, from }) if button == Button::Left => {
                        self.gesture = None;
                        let id = self.doc.shapes[index].id.clone();
                        match to {
                            Some((x, y)) => self.move_shape(&id, x, y),
                            // A click on the selected shape edits it, caret where clicked.
                            None if self.selected.as_ref() == Some(&id) => {
                                self.start_editing(id);
                                let caret = self.caret_at(from);
                                self.set_caret(caret);
                            }
                            None => self.select(Some(id)),
                        }
                    }
                    Some(Gesture::Text) if button == Button::Left => self.gesture = None,
                    _ => {}
                }
                self.respond(true)
            }
            Input::KeyDown(Key::Space) if self.editing.is_some() => self.respond(false),
            Input::KeyDown(Key::Space) => {
                let changed = !self.space_held;
                self.space_held = true;
                self.respond(changed)
            }
            Input::KeyUp(Key::Space) => {
                self.space_held = false;
                self.respond(true)
            }
            Input::Undo => {
                self.history.undo(&mut self.doc);
                self.stop_editing();
                self.respond(true)
            }
            Input::Redo => {
                self.history.redo(&mut self.doc);
                self.respond(true)
            }
            Input::ResetZoom => {
                let size = self.viewport.size;
                let center = Point {
                    x: size.x / 2.0,
                    y: size.y / 2.0,
                };
                (self.zoom).ease_to(&mut self.viewport, self.theme, 1.0, center, now);
                self.respond(true)
            }
            Input::Pinch { pos, delta } => {
                (self.zoom).pinch(&mut self.viewport, self.theme, pos, 1.0 + delta, now);
                self.respond(true)
            }
            Input::Wheel { pos, delta, mods } if mods.ctrl || mods.platform => {
                (self.zoom).wheel(&mut self.viewport, self.theme, pos, delta.y, now);
                self.respond(true)
            }
            Input::Wheel { .. } if self.editing.is_some() => self.respond(false),
            Input::Wheel {
                mut delta, mods, ..
            } => {
                // Shift turns a vertical wheel horizontal; macOS has already done it for a mouse.
                if mods.shift && delta.x == 0.0 {
                    delta = Point { x: delta.y, y: 0.0 };
                }
                self.viewport.pan(delta);
                self.respond(true)
            }
            Input::Text { text, replace } => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                let mut replace = replace;
                if self.editing.is_none() {
                    match self.selected.clone() {
                        Some(id) if !text.trim().is_empty() => self.start_editing(id),
                        _ => return self.respond(false),
                    }
                    // A range given before editing started indexes nothing: append.
                    replace = None;
                }
                let range = replace.unwrap_or_else(|| self.text_selection());
                self.replace(range, &text);
                self.respond(true)
            }
            Input::StartEditing => match self.selected.clone() {
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
            Input::ToggleBold | Input::ToggleItalic => {
                self.toggle(input == Input::ToggleItalic);
                self.respond(true)
            }
            Input::Move(m) => {
                let Some((caret, goal)) = self.motion(m) else {
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
                let range = match self.motion(m) {
                    _ if !sel.is_empty() => sel,
                    Some((to, _)) => sel.start.min(to.offset)..sel.start.max(to.offset),
                    None => return self.respond(false),
                };
                self.replace(range, "");
                self.respond(true)
            }
        }
    }

    /// Advances animations to `now`. The app calls it every frame while `Response.animating`.
    pub fn tick(&mut self, now: Duration) -> Response {
        self.now = now;
        let redraw = self.zoom.tick(&mut self.viewport, self.theme, now) || self.animating(now);
        self.respond(redraw)
    }

    /// The zoom pill to draw at `now`, if visible.
    pub fn zoom_pill(&self, now: Duration) -> Option<ZoomPill> {
        self.zoom.pill(self.theme, now)
    }

    /// Opens an editing session on `id` with the caret at the end of its text.
    fn start_editing(&mut self, id: ShapeId) {
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
            goal_x: None,
            pending: None,
            since: self.now,
        });
    }

    /// Ends the editing session, if any: its changes become one undo step.
    fn stop_editing(&mut self) {
        if self.editing.take().is_some() {
            self.history.end(&self.doc);
            self.over_text = false;
        }
    }

    /// The edited shape and its plain text.
    fn edited(&self) -> Option<(&Shape, String)> {
        let e = self.editing.as_ref()?;
        let shape = self.doc.shapes.iter().find(|s| s.id == e.id)?;
        Some((shape, shape.text.plain()))
    }

    /// The caret nearest screen point `p` in the edited shape.
    fn caret_at(&self, p: Point) -> Caret {
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
    fn set_caret(&mut self, c: Caret) {
        if let Some(e) = &mut self.editing {
            (e.caret, e.anchor, e.goal_x, e.pending, e.since) = (c, c.offset, None, None, self.now);
        }
    }

    fn text_selection(&self) -> Range<usize> {
        self.editing.as_ref().map_or(0..0, |e| {
            e.anchor.min(e.caret.offset)..e.anchor.max(e.caret.offset)
        })
    }

    /// Where motion `m` takes the caret, and the column up/down keep.
    fn motion(&self, m: Motion) -> Option<(Caret, Option<f32>)> {
        let e = self.editing.as_ref()?;
        let (shape, s) = self.edited()?;
        let (i, sel) = (e.caret.offset, self.text_selection());
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
        (self.history).change(&mut self.doc, &id, |s| {
            end = s.text.replace(range.clone(), text);
            if let Some((bold, italic)) = pending {
                s.text.set_style(range.start..end, false, bold);
                s.text.set_style(range.start..end, true, italic);
            }
        });
        self.set_caret(Caret {
            offset: end,
            upstream: false,
        });
    }

    /// Toggles italic (else bold) on the text selection, or in the pending style.
    fn toggle(&mut self, italic: bool) {
        let Some(e) = &self.editing else { return };
        let (id, sel) = (e.id.clone(), self.text_selection());
        if sel.is_empty() {
            let Some((shape, _)) = self.edited() else {
                return;
            };
            let mut st = e.pending.unwrap_or_else(|| shape.text.style_at(sel.start));
            *(if italic { &mut st.1 } else { &mut st.0 }) ^= true;
            if let Some(e) = &mut self.editing {
                e.pending = Some(st);
            }
            return;
        }
        (self.history).change(&mut self.doc, &id, |s| {
            let on = !s.text.all_styled(sel.clone(), italic);
            s.text.set_style(sel, italic, on);
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

    /// Index of the topmost shape under screen point `p`.
    fn shape_at(&self, p: Point) -> Option<usize> {
        let c = self.viewport.to_canvas(p);
        self.doc.shapes.iter().rposition(|s| {
            let (x, y) = (s.x as f32, s.y as f32);
            (x..=x + s.width as f32).contains(&c.x) && (y..=y + self.shape_height(s)).contains(&c.y)
        })
    }

    fn respond(&self, redraw: bool) -> Response {
        Response {
            redraw,
            cursor: match self.gesture {
                Some(Gesture::Pan { .. }) => Cursor::Grabbing,
                _ if self.space_held => Cursor::Grab,
                Some(Gesture::Text) => Cursor::Text,
                _ if self.over_text => Cursor::Text,
                _ => Cursor::Arrow,
            },
            animating: self.animating(self.now),
        }
    }
}
