//! Keyboard pan: arrows with nothing selected pan the view, tracked by key-down and key-up
//! rather than the OS repeat rate. Each press pans along its side on its own, so two held
//! arrows pan diagonally and quick taps stack.

use std::time::Duration;

use super::Session;
use crate::{KeyPanStyle, Side};

/// One arrow press: down at `down`, released at `up`; `done` screen pixels already applied.
#[derive(Clone, Copy, Debug)]
pub(super) struct Press {
    side: Side,
    down: Duration,
    up: Option<Duration>,
    done: f32,
}

impl Press {
    /// Screen pixels this press has panned by `now`: speed ramps up over `ease_in` while held,
    /// then a cubic eases from the key-up speed to rest over `ease_out`, at least `step` away.
    fn offset(&self, now: Duration, k: &KeyPanStyle) -> f32 {
        let (tin, tout) = (k.ease_in.as_secs_f32(), k.ease_out.as_secs_f32());
        let held = |t: f32| {
            k.speed
                * if t < tin {
                    t * t / (2.0 * tin)
                } else {
                    t - tin / 2.0
                }
        };
        let Some(up) = self.up else {
            return held(now.saturating_sub(self.down).as_secs_f32());
        };
        let t = up.saturating_sub(self.down).as_secs_f32();
        let (p, v) = (held(t), k.speed * (t / tin).min(1.0));
        let to = k.step.max(p + v * tout / 2.0);
        let s = (now.saturating_sub(up).as_secs_f32() / tout).min(1.0);
        p + (to - p) * (3.0 - 2.0 * s) * s * s + v * tout * s * (1.0 - s) * (1.0 - s)
    }

    fn settled(&self, now: Duration, k: &KeyPanStyle) -> bool {
        self.up.is_some_and(|up| now >= up + k.ease_out)
    }
}

impl Session {
    /// An arrow's key-down with nothing selected. A repeat of a held arrow does nothing.
    pub(super) fn key_pan_down(&mut self, side: Side) {
        self.advance_key_pan(self.now);
        if !(self.key_pan.iter()).any(|p| p.side == side && p.up.is_none()) {
            self.key_pan.push(Press {
                side,
                down: self.now,
                up: None,
                done: 0.0,
            });
        }
    }

    /// Releases the held arrow `side`, or every held arrow (focus lost).
    pub(super) fn key_pan_up(&mut self, side: Option<Side>) {
        self.advance_key_pan(self.now);
        let now = self.now;
        (self.key_pan.iter_mut())
            .filter(|p| p.up.is_none() && side.is_none_or(|s| s == p.side))
            .for_each(|p| p.up = Some(now));
    }

    /// Pans the view to where the presses are at `now`, dropping settled ones. True if any ran.
    pub(super) fn advance_key_pan(&mut self, now: Duration) -> bool {
        let (k, vp) = (&self.theme.key_pan, &mut self.viewport);
        let ran = !self.key_pan.is_empty();
        for p in &mut self.key_pan {
            let off = p.offset(now, k);
            let d = (off - p.done) / vp.zoom;
            p.done = off;
            let (x, y) = p.side.unit();
            vp.origin.x += x as f32 * d;
            vp.origin.y += y as f32 * d;
        }
        self.key_pan.retain(|p| !p.settled(now, k));
        ran
    }

    /// A keyboard pan is still moving at `now`.
    pub(super) fn key_pan_running(&self, now: Duration) -> bool {
        let k = &self.theme.key_pan;
        self.key_pan.iter().any(|p| !p.settled(now, k))
    }
}
