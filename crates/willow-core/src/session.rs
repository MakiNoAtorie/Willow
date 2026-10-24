//! The editor session: one per window. Owns the document and the viewport, takes plain input
//! events with the time passed in, and tells the app whether to redraw and which cursor to show.

use std::time::Duration;

use crate::history::History;
use crate::zoom::{Zoom, ZoomPill};
use crate::{Document, ShapeId, Theme};

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

/// One input event. Positions are screen pixels relative to the canvas's top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cursor {
    #[default]
    Arrow,
    /// Space held, ready to pan.
    Grab,
    /// Panning.
    Grabbing,
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

pub struct Session {
    doc: Document,
    history: History,
    theme: &'static Theme,
    viewport: Viewport,
    gesture: Option<Gesture>,
    space_held: bool,
    zoom: Zoom,
    selected: Option<ShapeId>,
    rings: Vec<Ring>,
    /// Time of the last event.
    now: Duration,
}

impl Session {
    pub fn new(doc: Document, theme: &'static Theme) -> Self {
        Session {
            doc,
            history: History::default(),
            theme,
            viewport: Viewport {
                origin: Point::default(),
                zoom: 1.0,
                size: Point::default(),
            },
            gesture: None,
            space_held: false,
            zoom: Zoom::default(),
            selected: None,
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
        self.rings.iter().any(|r| r.progress(now, fade) < 1.0) || self.zoom.running(self.theme, now)
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
            let (w, h) = (s.width as f32, s.height(self.theme));
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
                let gesture = match (button, self.shape_at(pos)) {
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
                _ => self.respond(false),
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
                    Some(Gesture::Press { index, to, .. }) if button == Button::Left => {
                        self.gesture = None;
                        let id = self.doc.shapes[index].id.clone();
                        match to {
                            Some((x, y)) => self.move_shape(&id, x, y),
                            None => self.select(Some(id)),
                        }
                    }
                    _ => {}
                }
                self.respond(true)
            }
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

    /// Index of the topmost shape under screen point `p`.
    fn shape_at(&self, p: Point) -> Option<usize> {
        let c = self.viewport.to_canvas(p);
        self.doc.shapes.iter().rposition(|s| {
            let (x, y) = (s.x as f32, s.y as f32);
            (x..=x + s.width as f32).contains(&c.x) && (y..=y + s.height(self.theme)).contains(&c.y)
        })
    }

    fn respond(&self, redraw: bool) -> Response {
        Response {
            redraw,
            cursor: match self.gesture {
                Some(Gesture::Pan { .. }) => Cursor::Grabbing,
                _ if self.space_held => Cursor::Grab,
                _ => Cursor::Arrow,
            },
            animating: self.animating(self.now),
        }
    }
}
