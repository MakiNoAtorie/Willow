//! Willow's GPUI-free core: the document model and the themes as plain data (ADR 0001).

mod edit;
mod history;
mod layout;
mod session;
mod theme;
mod zoom;

pub use layout::{Caret, Cluster, Layout, Line, Measure, layout};
pub use session::{
    Button, CaretView, Cursor, Editing, Input, Key, Modifiers, Motion, Point, Response, Session,
    Viewport,
};
pub use theme::{
    CaretStyle, CubicBezier, MenuStyle, PAPER, Rgba, SelectionStyle, Shadow, ShapeStyle, Theme,
    Transition, ZoomPillStyle, ZoomStyle,
};
pub use zoom::ZoomPill;

/// Width of every new shape, in canvas units. Shared by every theme.
pub const DEFAULT_WIDTH: u32 = 210;

/// One file's content: shapes, drawn in this order (later on top).
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub shapes: Vec<Shape>,
}

impl Document {
    /// A new document: one empty shape at the canvas origin.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut doc = Document { shapes: Vec::new() };
        let id = doc.next_id();
        doc.shapes.push(Shape::new(id, 0, 0));
        doc
    }

    /// The first unused ID in the order a, b, …, z, aa, ab, …
    pub fn next_id(&self) -> ShapeId {
        (0..)
            .map(id_for)
            .find(|id| self.shapes.iter().all(|s| s.id.0 != *id))
            .map(ShapeId)
            .expect("unbounded")
    }
}

/// Bijective base-26: 0 → "a", 25 → "z", 26 → "aa".
fn id_for(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'a' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).expect("ascii")
}

/// A shape's stable ID: short lowercase letters, never changed once given.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId(pub String);

/// A rounded rectangle holding rich text. Positions and sizes are whole canvas units.
/// Its drawn height is text height + vertical padding + `extra_height`; text height is never stored.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub id: ShapeId,
    pub text: RichText,
    /// Top-left corner.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub extra_height: u32,
    pub color: Color,
}

impl Shape {
    /// An empty neutral shape of the default width with its top-left at (x, y).
    pub fn new(id: ShapeId, x: i32, y: i32) -> Self {
        Shape {
            id,
            text: RichText::default(),
            x,
            y,
            width: DEFAULT_WIDTH,
            extra_height: 0,
            color: Color::Neutral,
        }
    }
}

/// Paragraphs of styled runs. Empty text has no paragraphs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub paragraphs: Vec<Paragraph>,
}

impl RichText {
    pub fn is_empty(&self) -> bool {
        self.paragraphs
            .iter()
            .all(|p| p.runs.iter().all(|r| r.text.is_empty()))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

/// The seven fixed palette names, in palette order. The names are what files store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    #[default]
    Neutral,
    Rose,
    Peach,
    Butter,
    Sage,
    Sky,
    Lilac,
}

impl Color {
    pub const ALL: [Color; 7] = [
        Color::Neutral,
        Color::Rose,
        Color::Peach,
        Color::Butter,
        Color::Sage,
        Color::Sky,
        Color::Lilac,
    ];

    pub fn name(self) -> &'static str {
        ["neutral", "rose", "peach", "butter", "sage", "sky", "lilac"][self as usize]
    }
}
