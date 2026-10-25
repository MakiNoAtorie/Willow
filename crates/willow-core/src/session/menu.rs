//! Context menus: a right-click resolves to a `Menu` request (kind, entries, point) that the
//! app draws; `Input::MenuChoose` runs an item against the point it was opened at.

use super::Session;
use crate::input::Point;
use crate::{Color, Link, Shape, ShapeId};
use std::time::Duration;

/// A shape's color fading in from `from`, begun at `start`.
pub(super) struct Ripple {
    pub from: Color,
    pub start: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Empty,
    Shape,
    Connector,
    /// A right-click inside a multi-selection.
    Multi,
    /// The keyboard color picker: one color row over the selected shape.
    Picker,
}

/// What an item does. One variant per item; `choose` runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    NewShape,
    Recolor(Color),
    RemoveLink,
    /// The app has put `Session::clipboard_text` on the clipboard; deletes the selection.
    Cut,
    /// The app has put `Session::clipboard_text` on the clipboard; only closes the menu.
    Copy,
    /// The app sends `Input::Paste` instead while the clipboard has text; else only closes.
    Paste,
    Duplicate,
    Delete,
    SelectChain,
}

/// Cmd (macOS) or Ctrl (Windows), plus Shift if set, plus `key`; the app picks the glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub shift: bool,
    pub key: &'static str,
}

impl Shortcut {
    /// The Delete key alone: the app shows it without Cmd/Ctrl.
    pub const DELETE: Shortcut = Shortcut {
        shift: false,
        key: "\u{232b}",
    };

    /// Cmd/Ctrl + `key`.
    const fn cmd(key: &'static str) -> Shortcut {
        Shortcut { shift: false, key }
    }
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
    /// A small label over the entries after it.
    Heading(&'static str, Option<Shortcut>),
    /// The seven picker colors in a row; `current` is ringed.
    Colors {
        current: Option<Color>,
    },
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
    /// The shape a shape menu was opened on; its colors go to that shape's chain.
    pub shape: Option<ShapeId>,
    /// The link a connector menu was opened on; Remove link removes it.
    pub link: Option<Link>,
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
        let entries = vec![MenuEntry::Item(new_shape)];
        self.menu = Some(self.menu_at(MenuKind::Empty, at, entries));
    }

    /// Opens the shape menu for the shape at screen point `at`, selecting it unless it is
    /// already selected; inside a multi-selection, the multi-selection menu.
    pub(super) fn open_shape_menu(&mut self, at: Point, index: usize) {
        self.stop_editing();
        let shape = &self.doc.shapes[index];
        let (id, current) = (shape.id.clone(), shape.color);
        if !self.selected.contains(&id) {
            self.select([id.clone()]);
        }
        if self.selected.len() > 1 {
            return self.open_multi_menu(at);
        }
        self.menu = Some(Menu {
            shape: Some(id),
            ..self.menu_at(MenuKind::Shape, at, shape_items(current))
        });
    }

    /// Opens the multi-selection menu at screen point `at`; the selection stays.
    fn open_multi_menu(&mut self, at: Point) {
        let entries = vec![
            item(MenuAction::Cut, "Cut", Shortcut::cmd("X")),
            item(MenuAction::Copy, "Copy", Shortcut::cmd("C")),
            item(MenuAction::Duplicate, "Duplicate", Shortcut::cmd("D")),
            item(MenuAction::SelectChain, "Select chain", Shortcut::cmd("L")),
            item(MenuAction::Delete, "Delete", Shortcut::DELETE),
        ];
        self.menu = Some(self.menu_at(MenuKind::Multi, at, entries));
    }

    /// A `kind` menu at screen point `at`, opened on no shape or link.
    pub(super) fn menu_at(&self, kind: MenuKind, at: Point, entries: Vec<MenuEntry>) -> Menu {
        Menu {
            kind,
            at,
            canvas: self.viewport.to_canvas(at),
            entries,
            shape: None,
            link: None,
        }
    }

    /// Runs `action` on what the open menu was opened on and closes the menu.
    pub(super) fn choose(&mut self, action: MenuAction) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        match action {
            MenuAction::NewShape => self.new_shape_at(menu.canvas, false),
            MenuAction::Cut | MenuAction::Delete => self.delete(&self.selected.clone()),
            MenuAction::Copy | MenuAction::Paste => {}
            MenuAction::Duplicate => self.duplicate(),
            MenuAction::SelectChain => self.select_chain(),
            MenuAction::Recolor(color) => {
                if let Some(id) = menu.shape {
                    self.ripple_recolor(&id, color);
                }
            }
            MenuAction::RemoveLink => {
                if let Some(Link(a, b)) = menu.link {
                    self.unlink(&a, &b);
                }
            }
        }
    }

    /// Paints `id`'s chain `color` as one undo step, each shape fading from its old color one
    /// ripple step after the one before it along the chain.
    fn ripple_recolor(&mut self, id: &ShapeId, color: Color) {
        let r = self.theme.ripple;
        let (now, fade) = (self.now, r.fade.duration);
        self.ripples.retain(|_, x| now < x.start + fade);
        let chain: Vec<ShapeId> = self.doc.chain(id).into_iter().cloned().collect();
        // Steps from the clicked shape; `chain` is in BFS order, so neighbours come first.
        let mut depth: std::collections::HashMap<&ShapeId, u32> = Default::default();
        for id in &chain {
            let d = (self.doc.links.iter())
                .filter_map(|l| match (&l.0, &l.1) {
                    (a, b) if a == id => depth.get(b),
                    (a, b) if b == id => depth.get(a),
                    _ => None,
                })
                .min()
                .map_or(0, |d| d + 1);
            depth.insert(id, d);
            if let Some(s) = self.doc.shapes.iter().find(|s| &s.id == id) {
                let start = self.now + r.step * d;
                self.ripples.insert(
                    id.clone(),
                    Ripple {
                        from: s.color,
                        start,
                    },
                );
            }
        }
        self.recolor(id, color);
    }

    /// The color `id` is fading in from and how far along (eased, 0 to 1), while it ripples.
    pub(crate) fn rippling(&self, id: &ShapeId, now: Duration) -> Option<(Color, f32)> {
        let r = self.ripples.get(id)?;
        let p = self.theme.ripple.fade.progress(now.saturating_sub(r.start));
        (now < r.start + self.theme.ripple.fade.duration).then_some((r.from, p))
    }

    /// An empty neutral shape centered on canvas point `c`, in the innermost zone under it,
    /// editing. One undo step together with its first editing session.
    pub(super) fn new_shape_at(&mut self, c: Point, push: bool) {
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
        if push {
            // Other chains it lands on are cleared, as midpoint creation does.
            let new = self.doc.shapes.len() - 1;
            self.clear_others(crate::Side::Right, vec![(new, None)]);
        }
        self.select([id.clone()]);
        // Its begin is a no-op; `stop_editing`'s end closes the step.
        self.start_editing(id);
    }

    /// The smallest zone containing canvas point `c`.
    pub(super) fn innermost_zone(&self, c: Point) -> Option<ShapeId> {
        (self.doc.zones.iter())
            .filter(|z| {
                let (x, y) = (z.x as f32, z.y as f32);
                (x..=x + z.width as f32).contains(&c.x) && (y..=y + z.height as f32).contains(&c.y)
            })
            .min_by_key(|z| z.width as u64 * z.height as u64)
            .map(|z| z.id.clone())
    }
}

fn item(action: MenuAction, label: &'static str, shortcut: Shortcut) -> MenuEntry {
    MenuEntry::Item(MenuItem {
        action,
        label,
        shortcut: Some(shortcut),
        enabled: true,
        destructive: false,
    })
}

/// The shape menu: clipboard items, a separator, then the colors under their heading with
/// `current` ringed.
fn shape_items(current: Color) -> Vec<MenuEntry> {
    vec![
        item(MenuAction::Cut, "Cut", Shortcut::cmd("X")),
        item(MenuAction::Copy, "Copy", Shortcut::cmd("C")),
        item(MenuAction::Paste, "Paste", Shortcut::cmd("V")),
        item(MenuAction::Duplicate, "Duplicate", Shortcut::cmd("D")),
        item(MenuAction::SelectChain, "Select chain", Shortcut::cmd("L")),
        MenuEntry::Separator,
        MenuEntry::Heading(
            "Color",
            Some(Shortcut {
                shift: true,
                key: "C",
            }),
        ),
        MenuEntry::Colors {
            current: Some(current),
        },
    ]
}
