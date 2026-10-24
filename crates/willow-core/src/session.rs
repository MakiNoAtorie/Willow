//! The editor session: one per window. Owns the document and the viewport, takes plain input
//! events with the time passed in, and tells the app whether to redraw and which cursor to show.

mod editing;
mod file;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use crate::history::History;
use crate::input::{Button, Cursor, Input, Key, Point, Response};
use crate::layout::{Layout, Measure, layout};
use crate::zoom::{Zoom, ZoomPill};
use crate::{Document, Link, RichText, Shape, ShapeId, Theme, Transition};

pub use editing::{CaretView, Editing};

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

/// Screen pixels the pointer may wander before a press counts as a drag.
const DRAG_START: f32 = 3.0;

/// Screen pixels around a shape's edge that grab it for resizing.
const EDGE_GRAB: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Bottom,
}

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
    /// Left press in the text being edited: dragging selects text, never moves the shape.
    Text,
    /// Left press on an edge of the shape at `index`. Once dragged, `to` is the previewed
    /// size; release then resizes the shape to it.
    Resize {
        index: usize,
        edge: Edge,
        from: Point,
        to: Option<ResizeTo>,
    },
}

/// What a resize sets: a shape's x, width and extra height, in canvas units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResizeTo {
    pub x: i32,
    pub width: u32,
    pub extra_height: u32,
}

/// The shape being dragged and its previewed top-left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragPreview<'a> {
    pub id: &'a ShapeId,
    pub x: i32,
    pub y: i32,
}

/// A selection ring easing from `from` to `to` (0 hidden, 1 shown), starting at `since`.
#[derive(Clone, Debug)]
struct Ring {
    from: f32,
    to: f32,
    since: Duration,
}

impl Ring {
    fn done(&self, now: Duration, fade: &Transition) -> bool {
        now.saturating_sub(self.since) >= fade.duration
    }

    fn value(&self, now: Duration, fade: &Transition) -> f32 {
        let p = fade.progress(now.saturating_sub(self.since));
        self.from + (self.to - self.from) * p
    }
}

/// A shape's layout and the text and width it was made for.
struct Cached {
    text: RichText,
    width: u32,
    layout: Rc<Layout>,
}

pub struct Session {
    doc: Document,
    history: History,
    theme: &'static Theme,
    measure: Box<dyn Measure>,
    layouts: RefCell<HashMap<ShapeId, Cached>>,
    /// Where `measure_ahead` goes on: a shape index.
    ahead: Cell<usize>,
    /// Each link's shape indices, for drawing; see `link_ends`.
    link_ends: RefCell<Vec<(usize, usize)>>,
    viewport: Viewport,
    gesture: Option<Gesture>,
    space_held: bool,
    zoom: Zoom,
    /// In the order selected. The edited shape (`editing`) stays selected while edited.
    selected: Vec<ShapeId>,
    editing: Option<Editing>,
    /// The pointer is over the text being edited.
    over_text: bool,
    /// The pointer is over a shape's edge.
    over_edge: Option<Edge>,
    rings: HashMap<ShapeId, Ring>,
    /// Time of the last event.
    now: Duration,
    /// The document as last read or written; `None` after a load that needed repairs.
    saved: Option<Document>,
    /// The bytes last read or written.
    bytes: Vec<u8>,
    /// First and last change not yet in a recovery file.
    unrecovered: Option<(Duration, Duration)>,
    /// A recovery write was taken since the last save.
    recovery_written: bool,
}

impl Session {
    /// `measure` is the app's text-measuring function.
    pub fn new(doc: Document, theme: &'static Theme, measure: Box<dyn Measure>) -> Self {
        Session {
            saved: Some(doc.clone()),
            bytes: Vec::new(),
            unrecovered: None,
            recovery_written: false,
            doc,
            history: History::default(),
            theme,
            measure,
            layouts: RefCell::default(),
            ahead: Cell::default(),
            link_ends: RefCell::default(),
            viewport: Viewport {
                origin: Point::default(),
                zoom: 1.0,
                size: Point::default(),
            },
            gesture: None,
            space_held: false,
            zoom: Zoom::default(),
            selected: Vec::new(),
            editing: None,
            over_text: false,
            over_edge: None,
            rings: HashMap::new(),
            now: Duration::ZERO,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn theme(&self) -> &'static Theme {
        self.theme
    }

    /// The selected shapes, in the order selected. The shape being edited is `editing()`.
    pub fn selection(&self) -> &[ShapeId] {
        &self.selected
    }

    /// The selected shape when exactly one is: what a click edits and typing starts editing.
    fn only_selected(&self) -> Option<ShapeId> {
        match self.selected.as_slice() {
            [id] => Some(id.clone()),
            _ => None,
        }
    }

    /// `shape`'s text layout, positioned from the text's top-left inside the padding. Cached
    /// per shape; recomputed only when its text or width changed.
    pub fn layout(&self, shape: &Shape) -> Rc<Layout> {
        let mut cache = self.layouts.borrow_mut();
        if let Some(c) = cache.get(&shape.id)
            && c.text == shape.text
            && c.width == shape.width
        {
            return c.layout.clone();
        }
        let width = (shape.width as f32 - 2.0 * self.theme.shape.pad_x).max(0.0);
        let l = Rc::new(layout(&shape.text, width, self.theme, &*self.measure));
        let (text, width, layout) = (shape.text.clone(), shape.width, l.clone());
        cache.insert(
            shape.id.clone(),
            Cached {
                text,
                width,
                layout,
            },
        );
        l
    }

    /// Each link's two shape indices, in link order. Cached, and checked against the document
    /// on every call (cheap ID compares), so any change to shapes or links rebuilds it.
    // ponytail: a link to a missing shape (`usize::MAX`) fails the check and rebuilds every
    // call; commands never make one.
    pub(crate) fn link_ends(&self) -> std::cell::Ref<'_, Vec<(usize, usize)>> {
        let (shapes, links) = (&self.doc.shapes, &self.doc.links);
        let is = |i: usize, id: &ShapeId| shapes.get(i).is_some_and(|s| &s.id == id);
        let valid = |ends: &Vec<(usize, usize)>| {
            ends.len() == links.len()
                && (ends.iter().zip(links)).all(|(&(i, j), Link(a, b))| is(i, a) && is(j, b))
        };
        if !valid(&self.link_ends.borrow()) {
            let at: HashMap<&ShapeId, usize> =
                shapes.iter().enumerate().map(|(i, s)| (&s.id, i)).collect();
            let index = |id| at.get(id).copied().unwrap_or(usize::MAX);
            *self.link_ends.borrow_mut() = (links.iter())
                .map(|Link(a, b)| (index(a), index(b)))
                .collect();
        }
        self.link_ends.borrow()
    }

    /// Drawn height in canvas units: text, padding and extra height.
    pub fn shape_height(&self, shape: &Shape) -> f32 {
        self.layout(shape).height + 2.0 * self.theme.shape.pad_y + shape.extra_height as f32
    }

    /// Every shape's drawn height, in document order: what `save` needs for the side rule.
    pub fn heights(&self) -> Vec<f32> {
        self.doc
            .shapes
            .iter()
            .map(|s| self.shape_height(s))
            .collect()
    }

    /// Lays out the next `n` shapes in document order, drawn or not, so that `heights` (every
    /// save and recovery write) finds them cached. True once it has been through them all.
    /// The app calls it in small steps while idle after opening a document.
    pub fn measure_ahead(&self, n: usize) -> bool {
        let (shapes, from) = (&self.doc.shapes, self.ahead.get());
        let to = (from + n).min(shapes.len());
        for s in &shapes[from.min(to)..to] {
            self.layout(s);
        }
        self.ahead.set(to);
        to == shapes.len()
    }

    /// Selection rings to draw at `now`: shape and ring opacity, 0 to 1. Includes rings
    /// still fading out after a deselect.
    pub fn selection_rings(&self, now: Duration) -> Vec<(&ShapeId, f32)> {
        let fade = &self.theme.selection.fade;
        self.rings
            .iter()
            .filter(|(_, r)| r.to > 0.0 || !r.done(now, fade))
            .map(|(id, r)| (id, r.value(now, fade)))
            .collect()
    }

    /// `id`'s selection ring opacity at `now`, 0 to 1.
    pub(crate) fn ring(&self, id: &ShapeId, now: Duration) -> f32 {
        let fade = &self.theme.selection.fade;
        self.rings.get(id).map_or(0.0, |r| r.value(now, fade))
    }

    /// Whether anything is still animating at `now`; the app keeps requesting frames until not.
    /// The caret blink isn't: see `next_wake`.
    pub fn animating(&self, now: Duration) -> bool {
        let fade = &self.theme.selection.fade;
        self.rings.values().any(|r| !r.done(now, fade)) || self.zoom.running(self.theme, now)
    }

    /// Deselects shapes the document no longer has (after undo, redo or a reload).
    fn drop_missing_selection(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        let ids: HashSet<&ShapeId> = self.doc.shapes.iter().map(|s| &s.id).collect();
        let kept: Vec<ShapeId> = (self.selected.iter())
            .filter(|id| ids.contains(id))
            .cloned()
            .collect();
        self.select(kept);
    }

    /// Makes `ids` the selection, duplicates dropped. Rings fade in on newly selected shapes
    /// and out on deselected ones. Not an undo step.
    pub fn select(&mut self, ids: impl IntoIterator<Item = ShapeId>) {
        let mut seen = HashSet::new();
        let ids: Vec<ShapeId> = (ids.into_iter())
            .filter(|id| seen.insert(id.clone()))
            .collect();
        if ids == self.selected {
            return;
        }
        let (now, fade) = (self.now, &self.theme.selection.fade);
        let (old, new): (HashSet<_>, HashSet<_>) =
            (self.selected.iter().collect(), ids.iter().collect());
        let fades: Vec<(ShapeId, f32)> = (old.difference(&new).map(|&id| (id.clone(), 0.0)))
            .chain(new.difference(&old).map(|&id| (id.clone(), 1.0)))
            .collect();
        for (id, to) in fades {
            let from = self.rings.get(&id).map_or(0.0, |r| r.value(now, fade));
            self.rings.insert(
                id,
                Ring {
                    from,
                    to,
                    since: now,
                },
            );
        }
        // Drop rings that have finished fading out.
        self.rings.retain(|_, r| r.to > 0.0 || !r.done(now, fade));
        self.selected = ids;
    }

    /// The shape being dragged and its previewed top-left; the document still holds the old one.
    pub fn drag_preview(&self) -> Option<DragPreview<'_>> {
        match self.gesture {
            Some(Gesture::Press {
                index,
                to: Some((x, y)),
                ..
            }) => Some(DragPreview {
                id: &self.doc.shapes[index].id,
                x,
                y,
            }),
            _ => None,
        }
    }

    /// The shape being resized as the drag would leave it; the document still holds the old one.
    pub fn resize_preview(&self) -> Option<Shape> {
        match self.gesture {
            Some(Gesture::Resize {
                index, to: Some(r), ..
            }) => Some(Shape {
                x: r.x,
                width: r.width,
                extra_height: r.extra_height,
                ..self.doc.shapes[index].clone()
            }),
            _ => None,
        }
    }

    /// Sets a shape's x, width and extra height: one discrete change to the document.
    pub fn resize_shape(&mut self, id: &ShapeId, r: ResizeTo) {
        self.history.change(&mut self.doc, id, |s| {
            (s.x, s.width, s.extra_height) = (r.x, r.width, r.extra_height)
        });
    }

    /// Drops a shape's extra height so it fits its text; the width stays.
    pub fn autofit(&mut self, id: &ShapeId) {
        self.history
            .change(&mut self.doc, id, |s| s.extra_height = 0);
    }

    /// The narrowest a shape may be: padding plus its widest grapheme cluster, or one em
    /// when it has no text.
    fn min_width(&self, shape: &Shape) -> u32 {
        let t = self.theme;
        // Wide enough that only paragraph breaks wrap.
        let l = layout(&shape.text, 1e6, t, &*self.measure);
        let widest = (l.lines.iter().flat_map(|l| &l.clusters))
            .map(|c| c.width)
            .fold(0.0, f32::max);
        let content = if shape.text.is_empty() {
            t.font_size
        } else {
            widest
        };
        (2.0 * t.shape.pad_x + content).ceil() as u32
    }

    /// Moves a shape's top-left to (x, y): one discrete change to the document.
    pub fn move_shape(&mut self, id: &ShapeId, x: i32, y: i32) {
        self.history
            .change(&mut self.doc, id, |s| (s.x, s.y) = (x, y));
    }

    /// Links `a` to `b` as one undo step; refused (false, no change) for the same shape, an
    /// existing link or a cycle. A join repaints `b`'s chain in `a`'s color.
    pub fn link(&mut self, a: &ShapeId, b: &ShapeId) -> bool {
        self.history.link(&mut self.doc, a, b)
    }

    /// Removes the link between `a` and `b` as one undo step; both parts keep the color.
    pub fn unlink(&mut self, a: &ShapeId, b: &ShapeId) -> bool {
        self.history.unlink(&mut self.doc, a, b)
    }

    /// Paints `id`'s whole chain `color` as one undo step.
    pub fn recolor(&mut self, id: &ShapeId, color: crate::Color) {
        self.history.recolor(&mut self.doc, id, color);
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
        let revision = self.history.revision();
        let r = self.dispatch(input, now);
        if self.history.revision() != revision {
            self.changed(now);
        }
        r
    }

    fn dispatch(&mut self, input: Input, now: Duration) -> Response {
        self.now = now;
        // Anything that may edit, undo or end the session commits the marked text first, so
        // no undo step ever holds it. Space keys come while the IME converts: leave it alone.
        match input {
            Input::Mark { .. } | Input::Text { .. } | Input::KeyDown(_) | Input::KeyUp(_) => {}
            Input::PointerMove { .. } | Input::Wheel { .. } | Input::Pinch { .. } => {}
            Input::ResetZoom => {}
            _ => self.unmark(),
        }
        match input {
            Input::PointerDown { pos, button, mods } => {
                let hit = self.shape_at(pos);
                if self.gesture.is_none() && button == Button::Left && !self.space_held {
                    if let Some(i) = hit.filter(|_| mods.alt && self.editing.is_none()) {
                        let id = self.doc.shapes[i].id.clone();
                        self.autofit(&id);
                        return self.respond(true);
                    }
                    if let Some((index, edge)) = self.edge_at(pos) {
                        self.stop_editing();
                        self.select([self.doc.shapes[index].id.clone()]);
                        self.gesture = Some(Gesture::Resize {
                            index,
                            edge,
                            from: pos,
                            to: None,
                        });
                        return self.respond(true);
                    }
                }
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
                        if mods.shift {
                            self.extend_to(caret, None);
                        } else {
                            self.set_caret(caret);
                        }
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
            Input::PointerMove { pos, .. }
                if matches!(self.gesture, Some(Gesture::Resize { .. })) =>
            {
                self.resize_to(pos);
                self.respond(true)
            }
            Input::PointerMove { pos, .. } => match &mut self.gesture {
                Some(Gesture::Pan { last, click, .. }) => {
                    if click.is_some_and(|c| (pos.x - c.x).hypot(pos.y - c.y) >= DRAG_START) {
                        *click = None;
                    }
                    let d = pos - *last;
                    *last = pos;
                    self.viewport.pan(d);
                    self.respond(true)
                }
                Some(Gesture::Press { index, from, to }) => {
                    let d = pos - *from;
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
                Some(Gesture::Text) => {
                    let caret = self.caret_at(pos);
                    self.extend_to(caret, None);
                    self.respond(true)
                }
                Some(Gesture::Resize { .. }) => self.respond(false),
                None => {
                    let edge = (!self.space_held).then(|| self.edge_at(pos)).flatten();
                    let edge = edge.map(|e| e.1);
                    let edge_changed = edge != std::mem::replace(&mut self.over_edge, edge);
                    let over = self.editing.as_ref().is_some_and(|e| {
                        (self.shape_at(pos)).is_some_and(|i| self.doc.shapes[i].id == e.id)
                    });
                    let changed = over != self.over_text;
                    self.over_text = over;
                    self.respond(changed || edge_changed)
                }
            },
            Input::PointerUp { button, .. } => {
                match self.gesture {
                    Some(Gesture::Pan {
                        button: b, click, ..
                    }) if b == button => {
                        self.gesture = None;
                        if click.is_some() {
                            self.select([]);
                        }
                    }
                    Some(Gesture::Press { index, to, from }) if button == Button::Left => {
                        self.gesture = None;
                        let id = self.doc.shapes[index].id.clone();
                        match to {
                            Some((x, y)) => self.move_shape(&id, x, y),
                            // A click on the selected shape edits it, caret where clicked.
                            None if self.selected == [id.clone()] => {
                                self.start_editing(id);
                                let caret = self.caret_at(from);
                                self.set_caret(caret);
                            }
                            None => self.select([id]),
                        }
                    }
                    Some(Gesture::Text) if button == Button::Left => self.gesture = None,
                    Some(Gesture::Resize { index, to, .. }) if button == Button::Left => {
                        self.gesture = None;
                        if let Some(r) = to {
                            let id = self.doc.shapes[index].id.clone();
                            self.resize_shape(&id, r);
                        }
                    }
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
            Input::FocusLost => {
                self.space_held = false;
                if matches!(self.gesture, Some(Gesture::Pan { .. })) {
                    self.gesture = None;
                }
                self.respond(true)
            }
            Input::Undo | Input::Redo => {
                if input == Input::Undo {
                    self.history.undo(&mut self.doc);
                } else {
                    self.history.redo(&mut self.doc);
                }
                self.stop_editing();
                self.drop_missing_selection();
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
            Input::Text { .. }
            | Input::Mark { .. }
            | Input::Unmark
            | Input::StartEditing
            | Input::StopEditing
            | Input::Select(_)
            | Input::Cut
            | Input::Paste(_)
            | Input::SelectAll
            | Input::ToggleBold
            | Input::ToggleItalic
            | Input::Move(_)
            | Input::Delete(_) => self.handle_text(input),
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

    /// Updates the previewed size of the resize gesture for the pointer at `pos`.
    fn resize_to(&mut self, pos: Point) {
        let Some(Gesture::Resize {
            index,
            edge,
            from,
            to,
        }) = &mut self.gesture
        else {
            return;
        };
        let (d, z) = (pos - *from, self.viewport.zoom);
        if to.is_none() && d.x.hypot(d.y) < DRAG_START {
            return;
        }
        let (index, edge) = (*index, *edge);
        let s = &self.doc.shapes[index];
        let min = self.min_width(s) as i32;
        let grown = |dx: f32| {
            // Whole units each side; the width stays at least `min`.
            let d = (dx / z).round() as i32;
            let low = (min - s.width as i32 + 1).div_euclid(2);
            d.max(low)
        };
        let r = match edge {
            Edge::Right | Edge::Left => {
                let d = grown(if edge == Edge::Right { d.x } else { -d.x });
                ResizeTo {
                    x: s.x - d,
                    width: (s.width as i32 + 2 * d) as u32,
                    extra_height: s.extra_height,
                }
            }
            Edge::Bottom => {
                let extra = (s.extra_height as i32 + (d.y / z).round() as i32).max(0);
                ResizeTo {
                    x: s.x,
                    width: s.width,
                    extra_height: extra as u32,
                }
            }
        };
        if let Some(Gesture::Resize { to, .. }) = &mut self.gesture {
            *to = Some(r);
        }
    }

    /// The topmost shape whose edge is within `EDGE_GRAB` of screen point `p`, and which edge.
    /// A shape's interior under `p` hides the edges of shapes below it.
    fn edge_at(&self, p: Point) -> Option<(usize, Edge)> {
        let g = EDGE_GRAB;
        for (i, s) in self.doc.shapes.iter().enumerate().rev() {
            let tl = self.viewport.to_screen(Point {
                x: s.x as f32,
                y: s.y as f32,
            });
            let z = self.viewport.zoom;
            let (w, h) = (s.width as f32 * z, self.shape_height(s) * z);
            if !(tl.x - g..=tl.x + w + g).contains(&p.x)
                || !(tl.y - g..=tl.y + h + g).contains(&p.y)
            {
                continue;
            }
            let edge = if p.y < tl.y {
                None
            } else if (p.x - tl.x).abs() <= g {
                Some(Edge::Left)
            } else if (p.x - tl.x - w).abs() <= g {
                Some(Edge::Right)
            } else if (p.y - tl.y - h).abs() <= g {
                Some(Edge::Bottom)
            } else {
                None
            };
            return edge.map(|e| (i, e));
        }
        None
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
                Some(Gesture::Resize { edge, .. }) => edge_cursor(edge),
                _ if self.space_held => Cursor::Grab,
                Some(Gesture::Text) => Cursor::Text,
                _ if self.over_text => Cursor::Text,
                _ if let Some(e) = self.over_edge => edge_cursor(e),
                _ => Cursor::Arrow,
            },
            animating: self.animating(self.now),
        }
    }
}

fn edge_cursor(edge: Edge) -> Cursor {
    match edge {
        Edge::Left | Edge::Right => Cursor::ResizeEw,
        Edge::Bottom => Cursor::ResizeNs,
    }
}
