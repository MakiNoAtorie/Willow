//! The keyboard color picker: a `MenuKind::Picker` menu holding one color row over the selected
//! shape. Left/Right move its ring, Enter applies through the menu's Recolor path.

use super::{Menu, MenuAction, MenuEntry, MenuKind, Session};
use crate::{Color, Point};

impl Session {
    /// Opens the picker over the single selected shape, ringed on its color. Does nothing
    /// without exactly one selected shape.
    pub(super) fn open_color_picker(&mut self) {
        let Some(id) = self.only_selected() else {
            return;
        };
        let Some(i) = self.index_of(&id) else {
            return;
        };
        let s = &self.doc.shapes[i];
        let at = self.viewport.to_screen(Point {
            x: s.x as f32,
            y: s.y as f32,
        });
        let entries = vec![MenuEntry::Colors {
            current: Some(self.doc.shapes[i].color),
        }];
        self.menu = Some(Menu {
            shape: Some(id),
            ..self.menu_at(MenuKind::Picker, at, entries)
        });
    }

    /// Whether the keyboard color picker is the open menu.
    pub fn color_picker_open(&self) -> bool {
        self.menu
            .as_ref()
            .is_some_and(|m| m.kind == MenuKind::Picker)
    }

    /// Moves the ring `d` colors, stopping at the ends.
    pub(super) fn picker_step(&mut self, d: i32) {
        if let Some(Menu { entries, .. }) =
            self.menu.as_mut().filter(|m| m.kind == MenuKind::Picker)
            && let [MenuEntry::Colors { current: Some(c) }] = &mut entries[..]
        {
            *c = Color::ALL[(*c as i32 + d).clamp(0, Color::ALL.len() as i32 - 1) as usize];
        }
    }

    /// Applies the ringed color to the chain and closes the picker.
    pub(super) fn picker_apply(&mut self) {
        if let Some(Menu { entries, .. }) =
            self.menu.as_ref().filter(|m| m.kind == MenuKind::Picker)
            && let [MenuEntry::Colors { current: Some(c) }] = entries[..]
        {
            self.choose(MenuAction::Recolor(c));
        }
    }
}
