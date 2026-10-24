//! Context menus: a right-click resolves to a `Menu` request (kind, entries, point) that the
//! app draws; `Input::MenuChoose` runs an item against the point it was opened at.

use super::Session;
use crate::input::Point;
use crate::{Color, Shape, ShapeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Empty,
}

/// What an item does. One variant per item; `choose` runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    NewShape,
}

/// Cmd (macOS) or Ctrl (Windows), plus Shift if set, plus `key`; the app picks the glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub shift: bool,
    pub key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub action: MenuAction,
    pub label: &'static str,
    pub shortcut: Option<Shortcut>,
    pub enabled: bool,
    /// Drawn in the destructive color (zone Delete).
    pub destructive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    Item(MenuItem),
    Separator,
}

/// An open menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub kind: MenuKind,
    /// Where it was opened, screen pixels from the canvas's top-left.
    pub at: Point,
    /// The same point in canvas units, for items that act there.
    pub canvas: Point,
    pub entries: Vec<MenuEntry>,
}

impl Session {
    /// The open context menu, if any.
    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    /// Opens the empty-space menu at screen point `at`, leaving any editing.
    pub(super) fn open_empty_menu(&mut self, at: Point) {
        self.stop_editing();
        let new_shape = MenuItem {
            action: MenuAction::NewShape,
            label: "New shape",
            shortcut: Some(Shortcut {
                shift: true,
                key: "N",
            }),
            enabled: true,
            destructive: false,
        };
        self.menu = Some(Menu {
            kind: MenuKind::Empty,
            at,
            canvas: self.viewport.to_canvas(at),
            entries: vec![MenuEntry::Item(new_shape)],
        });
    }

    /// Runs `action` at the open menu's point and closes the menu.
    pub(super) fn choose(&mut self, action: MenuAction) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        match action {
            MenuAction::NewShape => self.new_shape_at(menu.canvas),
        }
    }

    /// An empty neutral shape centered on canvas point `c`, in the innermost zone under it,
    /// editing. One undo step together with its first editing session.
    fn new_shape_at(&mut self, c: Point) {
        let id = self.doc.next_id();
        let mut shape = Shape::new(id.clone(), 0, 0);
        shape.color = Color::Neutral;
        shape.x = (c.x - shape.width as f32 / 2.0).round() as i32;
        shape.y = (c.y - self.shape_height(&shape) / 2.0).round() as i32;
        let zone = self.innermost_zone(c);
        self.history.begin();
        self.history.add(&mut self.doc, shape);
        if let Some(zone) = zone {
            self.history.join_zone(&mut self.doc, &zone, &id);
        }
        self.select([id.clone()]);
        // Its begin is a no-op; `stop_editing`'s end closes the step.
        self.start_editing(id);
    }

    /// The smallest zone containing canvas point `c`.
    fn innermost_zone(&self, c: Point) -> Option<ShapeId> {
        (self.doc.zones.iter())
            .filter(|z| {
                let (x, y) = (z.x as f32, z.y as f32);
                (x..=x + z.width as f32).contains(&c.x) && (y..=y + z.height as f32).contains(&c.y)
            })
            .min_by_key(|z| z.width as u64 * z.height as u64)
            .map(|z| z.id.clone())
    }
}
