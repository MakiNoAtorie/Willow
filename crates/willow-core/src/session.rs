//! The editor session: one per window. Owns the document and the viewport, takes plain input
//! events with the time passed in, and tells the app whether to redraw and which cursor to show.

use std::time::Duration;

use crate::{Document, Theme};

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
    /// Wheel or trackpad scroll: how far the content should move, in screen pixels.
    Wheel {
        delta: Point,
        mods: Modifiers,
    },
    KeyDown(Key),
    KeyUp(Key),
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

/// A pointer gesture in progress.
#[derive(Clone, Copy, Debug)]
enum Gesture {
    Pan { button: Button, last: Point },
}

pub struct Session {
    doc: Document,
    theme: &'static Theme,
    viewport: Viewport,
    gesture: Option<Gesture>,
    space_held: bool,
}

impl Session {
    pub fn new(doc: Document, theme: &'static Theme) -> Self {
        Session {
            doc,
            theme,
            viewport: Viewport {
                origin: Point::default(),
                zoom: 1.0,
                size: Point::default(),
            },
            gesture: None,
            space_held: false,
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn theme(&self) -> &'static Theme {
        self.theme
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
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
    pub fn handle(&mut self, input: Input, _now: Duration) -> Response {
        match input {
            Input::PointerDown { pos, button, .. } => {
                let pans = match button {
                    _ if self.gesture.is_some() => false,
                    Button::Left => self.space_held || self.shape_at(pos).is_none(),
                    Button::Middle => true,
                    Button::Right => false,
                };
                if pans {
                    self.gesture = Some(Gesture::Pan { button, last: pos });
                }
                self.respond(pans)
            }
            Input::PointerMove { pos, .. } => match &mut self.gesture {
                Some(Gesture::Pan { last, .. }) => {
                    let d = Point {
                        x: pos.x - last.x,
                        y: pos.y - last.y,
                    };
                    *last = pos;
                    self.viewport.pan(d);
                    self.respond(true)
                }
                None => self.respond(false),
            },
            Input::PointerUp { button, .. } => {
                if matches!(self.gesture, Some(Gesture::Pan { button: b, .. }) if b == button) {
                    self.gesture = None;
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
            // Ctrl/Cmd+wheel zooms (#7).
            Input::Wheel { mods, .. } if mods.ctrl || mods.platform => self.respond(false),
            Input::Wheel { mut delta, mods } => {
                // Shift turns a vertical wheel horizontal; macOS has already done it for a mouse.
                if mods.shift && delta.x == 0.0 {
                    delta = Point { x: delta.y, y: 0.0 };
                }
                self.viewport.pan(delta);
                self.respond(true)
            }
        }
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
                None if self.space_held => Cursor::Grab,
                None => Cursor::Arrow,
            },
            animating: false,
        }
    }
}
