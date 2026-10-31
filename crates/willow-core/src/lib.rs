//! Willow's GPUI-free core: the document model and the themes as plain data (ADR 0001).

mod chain;
mod check;
mod draw;
mod edit;
mod format;
mod generate;
mod history;
mod input;
mod layout;
mod recovery;
mod session;
mod syntax;
mod theme;
mod zoom;

pub use chain::{Link, Side, side};
pub use check::{LineKind, check};
pub use draw::{Appear, Connector, Frame, LinkDrag, Visible, VisibleZone};
pub use format::{LoadError, Loaded, Unplaced, load, save};
pub use generate::generate;
pub use input::{Button, Cursor, Input, Key, Modifiers, Motion, Point, Response};
pub use layout::{Align, Caret, Cluster, Layout, Line, Measure, layout};
pub use recovery::{LaunchPlan, Recovery, RecoveryFile, Reopen, launch_plan};
pub use session::{
    CaretView, DragPreview, DrawnRect, Editing, Menu, MenuAction, MenuEntry, MenuItem, MenuKind,
    Panel, PanelRow, Regenerated, Regeneration, ResizeTo, Session, Shortcut, Span, TextCheck,
    TextChecked, Viewport,
};
pub use theme::{
    CaretStyle, ChromeStyle, CodeStyle, ConnectorStyle, CreateStyle, CubicBezier, DESK,
    KeyPanStyle, MenuStyle, MidpointStyle, PAPER, Rgba, RippleStyle, SelectionStyle, Shadow,
    ShapeStyle, THEMES, TabPickerStyle, Theme, ToggleStyle, Transition, Weave, ZoneStyle,
    ZoomPillStyle, ZoomStyle,
};
pub use zoom::ZoomPill;

/// Width of every new shape, in canvas units. Shared by every theme.
pub const DEFAULT_WIDTH: u32 = 210;

/// One file's content. `Default` is an empty document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    /// Drawn in this order (later on top).
    pub shapes: Vec<Shape>,
    /// Every chain is a tree and all its shapes share one color; change links through
    /// `link` / `unlink` to keep it so.
    pub links: Vec<Link>,
    /// Drawn in this order, so a parent comes before the zones nested in it.
    pub zones: Vec<Zone>,
}

impl Document {
    /// A new document: one empty shape at the canvas origin.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut doc = Document::default();
        let id = doc.next_id();
        doc.shapes.push(Shape::new(id, 0, 0));
        doc
    }

    /// The first ID no shape or zone uses, in the order a, b, …, z, aa, ab, …
    pub fn next_id(&self) -> ShapeId {
        self.next_ids(1).remove(0)
    }

    /// The first `n` IDs no shape or zone uses, in generation order.
    pub(crate) fn next_ids(&self, n: usize) -> Vec<ShapeId> {
        let ids: std::collections::HashSet<&str> = (self.shapes.iter().map(|s| s.id.0.as_str()))
            .chain(self.zones.iter().map(|z| z.id.0.as_str()))
            .collect();
        (0..)
            .map(id_for)
            .filter(|id| !ids.contains(id.as_str()))
            .take(n)
            .map(ShapeId)
            .collect()
    }

    /// The zone listing shape or zone `id` as a member, if any.
    pub(crate) fn parent_zone(&self, id: &ShapeId) -> Option<&ShapeId> {
        (self.zones.iter())
            .find(|z| z.members.contains(id))
            .map(|z| &z.id)
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

/// A shape's or zone's stable ID: short lowercase letters, never changed once given. Shapes
/// and zones share one namespace.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId(pub String);

impl ShapeId {
    /// The sort key for ID tie-breaks: generation order, shorter first, then alphabetical.
    pub(crate) fn generation_order(&self) -> (usize, &str) {
        (self.0.len(), &self.0)
    }
}

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

/// A titled, colored container. Membership is explicit: `members` lists shape and zone IDs,
/// whatever the geometry. Zones nest one level deep at most.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub id: ShapeId,
    /// Plain text.
    pub title: String,
    pub color: Color,
    /// Top-left corner and size, in canvas units.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub members: Vec<ShapeId>,
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
