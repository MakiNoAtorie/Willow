//! Keyboard move and resize (Ctrl/Cmd+Arrows, with Shift to resize).

use super::{ResizeTo, Session};
use crate::Side;

impl Session {
    /// Moves the selection one step toward `side`, like a drag: pushes nothing, zones stay.
    pub(super) fn nudge(&mut self, side: Side) {
        let ((x, y), step) = (side.unit(), self.theme.nudge);
        self.move_selection(x * step, y * step);
    }

    /// Resizes the one selected shape a step: Left/Right narrow/widen it around its center,
    /// Top/Bottom shrink/grow its extra height. Same limits and pushes as a mouse resize.
    pub(super) fn nudge_size(&mut self, side: Side) {
        let Some(id) = self.only_selected() else {
            return;
        };
        let Some(i) = self.index_of(&id) else {
            return;
        };
        let (s, step) = (&self.doc.shapes[i], self.theme.nudge);
        let half = step / 2;
        let d = match side {
            Side::Left => (-half).max(self.least_growth(s)).min(0),
            Side::Right => half,
            _ => 0,
        };
        let extra = match side {
            Side::Top => (s.extra_height as i32 - step).max(0),
            Side::Bottom => s.extra_height as i32 + step,
            _ => s.extra_height as i32,
        };
        let r = ResizeTo {
            x: s.x - d,
            width: (s.width as i32 + 2 * d) as u32,
            extra_height: extra as u32,
        };
        self.resize_shape(&id, r);
    }
}
