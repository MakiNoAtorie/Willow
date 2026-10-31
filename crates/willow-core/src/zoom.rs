//! Zoom: eased wheel steps that retarget mid-animation, direct pinch, reset, and the zoom pill.

use std::time::Duration;

use crate::{Point, Theme, Viewport};

/// Screen pixels of wheel one Zoom In / Zoom Out command counts as: one Windows notch.
pub(crate) const ZOOM_NOTCH_PX: f32 = 60.0;

/// Zoom factor per screen pixel of Ctrl/Cmd+wheel: one 60 px Windows notch is about 1.2x.
// ponytail: calibration knob, tune per device if a notch feels too big or small.
const WHEEL_ZOOM_PER_PX: f32 = 0.003;

/// The zoom pill as drawn now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomPill {
    /// The zoom level being reached, in whole percent.
    pub percent: u32,
    pub opacity: f32,
}

/// A wheel step or reset easing from `from` to `to`, keeping `anchor` (screen) fixed.
#[derive(Clone, Copy, Debug)]
struct Step {
    from: f32,
    to: f32,
    start: Duration,
    anchor: Point,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Zoom {
    step: Option<Step>,
    /// Time of the last zoom change and the level it aimed for.
    pill: Option<(Duration, f32)>,
}

impl Zoom {
    /// Ctrl/Cmd+wheel: one step of `dy` screen pixels around `pos`, retargeting a running step.
    pub fn wheel(&mut self, vp: &mut Viewport, t: &Theme, pos: Point, dy: f32, now: Duration) {
        let base = self.step.map_or(vp.zoom, |s| s.to);
        self.ease_to(vp, t, base * (dy * WHEEL_ZOOM_PER_PX).exp(), pos, now);
    }

    /// Eases to `zoom` around screen point `anchor`, starting from the zoom shown at `now`.
    pub fn ease_to(
        &mut self,
        vp: &mut Viewport,
        t: &Theme,
        zoom: f32,
        anchor: Point,
        now: Duration,
    ) {
        self.tick(vp, t, now);
        let to = zoom.clamp(t.zoom.min, t.zoom.max);
        self.step = Some(Step {
            from: vp.zoom,
            to,
            start: now,
            anchor,
        });
        self.pill = Some((now, to));
    }

    /// Pinch: scales by `factor` around `pos` at once, cancelling any eased step.
    pub fn pinch(&mut self, vp: &mut Viewport, t: &Theme, pos: Point, factor: f32, now: Duration) {
        self.step = None;
        let to = (vp.zoom * factor).clamp(t.zoom.min, t.zoom.max);
        vp.zoom_at(to, pos);
        self.pill = Some((now, to));
    }

    /// Advances a running step to `now`. Returns whether anything was animating before.
    pub fn tick(&mut self, vp: &mut Viewport, t: &Theme, now: Duration) -> bool {
        let was = self.step.is_some() || self.pill.is_some();
        if let Some(s) = self.step {
            let p = t.zoom_step.progress(now.saturating_sub(s.start));
            vp.zoom_at(s.from + (s.to - s.from) * p, s.anchor);
            if p >= 1.0 {
                vp.zoom = s.to;
                self.step = None;
            }
        }
        if self.pill_opacity(t, now).is_none() {
            self.pill = None;
        }
        was
    }

    /// A step is easing or the pill is still visible at `now`.
    pub fn running(&self, t: &Theme, now: Duration) -> bool {
        self.step.is_some() || self.pill_opacity(t, now).is_some()
    }

    pub fn pill(&self, t: &Theme, now: Duration) -> Option<ZoomPill> {
        let (_, zoom) = self.pill?;
        Some(ZoomPill {
            percent: (zoom * 100.0).round() as u32,
            opacity: self.pill_opacity(t, now)?,
        })
    }

    /// Held fully visible, then faded; `None` once gone.
    fn pill_opacity(&self, t: &Theme, now: Duration) -> Option<f32> {
        let (since, _) = self.pill?;
        let p = &t.zoom_pill;
        let elapsed = now.saturating_sub(since);
        if elapsed >= p.hold + p.fade.duration {
            return None;
        }
        Some(1.0 - p.fade.progress(elapsed.saturating_sub(p.hold)))
    }
}

impl Viewport {
    /// Sets the zoom, keeping the canvas point under screen point `anchor` in place.
    fn zoom_at(&mut self, zoom: f32, anchor: Point) {
        let c = self.to_canvas(anchor);
        self.zoom = zoom;
        self.origin = Point {
            x: c.x - anchor.x / zoom,
            y: c.y - anchor.y / zoom,
        };
    }
}
