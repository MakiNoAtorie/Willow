//! The input vocabulary: what the app sends the session and what it gets back.

use std::ops::{Range, Sub};

/// A point in screen pixels (relative to the canvas's top-left) or in canvas units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Sub for Point {
    type Output = Point;

    fn sub(self, o: Point) -> Point {
        Point {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
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
    /// IME composition: replaces byte range `replace`, or else the marked text, or else the
    /// text selection with `text` and marks it. `selected` (bytes into `text`) is the caret
    /// and anchor, else the caret goes after it. Marked text is no undo step until a `Text`
    /// input commits it. Starts editing on a selected shape, as `Text` does.
    Mark {
        text: String,
        replace: Option<Range<usize>>,
        selected: Option<Range<usize>>,
    },
    /// Commits the marked text as it is. Any input other than `Mark` and `Text` does too.
    Unmark,
    /// Shift+motion: moves the caret and keeps the selection's other end.
    Select(Motion),
    SelectAll,
    /// Deletes the text selection; the app has put `Session::selected_text` on the clipboard.
    Cut,
    /// Plain text from the clipboard, replacing the text selection. Line breaks (`\r\n`, `\r`
    /// or `\n`) become paragraph breaks; other control characters are dropped.
    Paste(String),
    /// The window lost focus: Space no longer counts as held and a pan ends, since their
    /// key-up or release may never arrive.
    FocusLost,
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
    /// Over a side edge, or resizing from one.
    ResizeEw,
    /// Over the bottom edge, or resizing from it.
    ResizeNs,
}

/// What the app does after an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub redraw: bool,
    pub cursor: Cursor,
    /// An animation is running: keep requesting frames.
    pub animating: bool,
}
