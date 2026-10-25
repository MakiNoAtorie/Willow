//! Keyboard move and resize (Ctrl/Cmd+Arrows, with Shift to resize).

use super::{ResizeTo, Session};
use crate::Side;

/// How far one press moves or resizes, in canvas units.
const STEP: i32 = 24;

impl Session {
    /// Moves the selection one step toward `side`, like a drag: pushes nothing, zones stay.
    pub(super) fn nudge(&mut self, side: Side) {
        let (dx, dy) = match side {
            Side::Left => (-STEP, 0),
            Side::Right => (STEP, 0),
            Side::Top => (0, -STEP),
            Side::Bottom => (0, STEP),
        };
        self.move_selection(dx, dy);
    }

    /// Resizes the one selected shape a step: Left/Right narrow/widen it around its center,
    /// Top/Bottom shrink/grow its extra height. Same limits and pushes as a mouse resize.
    pub(super) fn nudge_size(&mut self, side: Side) {
        let Some(id) = self.only_selected() else {
            return;
        };
        let Some(s) = self.doc.shapes.iter().find(|s| s.id == id) else {
            return;
        };
        let half = STEP / 2;
        let d = match side {
            Side::Left => (-half).max(self.least_growth(s)).min(0),
            Side::Right => half,
            _ => 0,
        };
        let extra = match side {
            Side::Top => (s.extra_height as i32 - STEP).max(0),
            Side::Bottom => s.extra_height as i32 + STEP,
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
