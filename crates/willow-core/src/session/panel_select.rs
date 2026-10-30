//! The code panel and the canvas follow each other's selection: a caret on a shape's line
//! selects it, and a canvas selection scrolls the unfocused panel to its line.

use std::borrow::Cow;

use super::Session;
use crate::{LineKind, check};

impl Session {
    /// What each panel line is. `Panel.lines` is empty while every line is fine, as after a
    /// regeneration; the lines are then read from the text.
    // ponytail: a full check per use in that case; cache it by revision if a caret move in a
    // 10,000-shape panel is felt.
    fn panel_kinds(&self) -> Cow<'_, [LineKind]> {
        if self.panel.lines.is_empty() {
            Cow::Owned(check(self.panel.text()).lines)
        } else {
            Cow::Borrowed(&self.panel.lines)
        }
    }

    /// The caret has moved to byte `offset`: selects the shape whose line or `@layout` line
    /// it is on. Zone, invalid and ignored lines leave the selection alone.
    pub(super) fn panel_follow_caret(&mut self, offset: usize) {
        let text = self.panel.text();
        let n = text[..text.floor_char_boundary(offset)]
            .matches('\n')
            .count();
        if let Some(LineKind::Shape(id)) = self.panel_kinds().get(n) {
            let id = id.clone();
            self.select([id]);
        }
    }

    /// A canvas selection while the panel is open and unfocused: shows the line of the
    /// selected shape that comes first in story order. A shape the text doesn't have yet is
    /// shown when the next regeneration lands.
    pub(super) fn panel_follow_selection(&mut self) {
        if !self.panel.open || self.panel.focused {
            return;
        }
        let heights = self.heights();
        let Some(i) = self.first_in_story_order(&heights) else {
            return;
        };
        let id = &self.doc.shapes[i].id;
        let line =
            (self.panel_kinds().iter()).position(|k| matches!(k, LineKind::Shape(s) if s == id));
        match line {
            Some(n) => {
                let row = self.rows().lines[n.min(self.rows().lines.len() - 1)].1;
                self.panel_show_row(row);
            }
            None => self.panel.seek = true,
        }
    }

    /// The text has just been regenerated: shows the selection if it was waiting for it.
    pub(super) fn panel_seek_selection(&mut self) {
        if std::mem::take(&mut self.panel.seek) {
            self.panel_follow_selection();
        }
    }

    /// Scrolls the least that shows row `row` (counted over the whole text) between the top
    /// and bottom padding.
    pub(super) fn panel_show_row(&mut self, row: usize) {
        let (c, height) = (&self.theme.code, self.viewport.size.y);
        let top = row as f32 * c.row_height();
        let low = c.pad_top + top + c.row_height() + c.pad_bottom - height;
        let max = self.max_scroll();
        let scroll = self.panel.scroll.min(max).max(low).min(top);
        self.panel.scroll = scroll.clamp(0.0, max);
    }
}
