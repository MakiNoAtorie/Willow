//! The draw query: what the app paints for a screen rectangle, derived from the zoom and the
//! theme's low-zoom thresholds.

use std::borrow::Cow;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use crate::{CaretView, Color, Layout, Point, Rgba, Session, Shape};

/// One frame's worth of drawing.
pub struct Frame<'a> {
    /// Shapes intersecting the rectangle, in draw order (later on top).
    pub shapes: Vec<Visible<'a>>,
    /// Shape text opacity: 1 at `fade_start` and above, 0 at `fade_end` and below.
    pub text_opacity: f32,
    /// How far fills have moved from pastel toward the picker color, 0 to `block_mix`.
    pub deepen: f32,
    /// Shape shadows and the dot grid are drawn (dropped below `fade_end`).
    pub shadows: bool,
    pub grid: bool,
}

pub struct Visible<'a> {
    /// The resize preview's shape while the shape is being resized.
    pub shape: Cow<'a, Shape>,
    /// Top-left in canvas units: the drag preview's while the shape is dragged.
    pub x: i32,
    pub y: i32,
    /// Drawn height in canvas units.
    pub height: f32,
    /// Fill color, deepened.
    pub fill: Rgba,
    pub layout: Rc<Layout>,
    /// Selection ring opacity, 0 to 1; a deselected shape's ring is still fading out.
    pub ring: f32,
    /// While text shows, on the edited shape only: the text selection highlight in canvas
    /// units, one (top-left, bottom-right) per line; the IME marked text; and the caret,
    /// unless in its blink's hidden half. Paint them with the shape, in its draw order.
    pub selection: Vec<(Point, Point)>,
    pub marked: Option<Range<usize>>,
    pub caret: Option<CaretView>,
}

impl Session {
    /// What to draw at `now` in the screen rectangle `from`..`to` (canvas-relative screen pixels).
    pub fn draw(&self, from: Point, to: Point, now: Duration) -> Frame<'_> {
        let (t, vp) = (self.theme(), self.viewport());
        let z = &t.zoom;
        let fade = ((vp.zoom - z.fade_end) / (z.fade_start - z.fade_end)).clamp(0.0, 1.0);
        let deepen = z.block_mix * (1.0 - fade);
        let fills = Color::ALL.map(|c| mix(t.pastel(c), t.picker[c as usize], deepen));
        let (a, b) = (vp.to_canvas(from), vp.to_canvas(to));
        let (preview, resized) = (self.drag_preview(), self.resize_preview());
        let rings = self.selection_rings(now);
        let edited = self.editing().filter(|_| fade > 0.0);
        let shapes = (self.document().shapes.iter())
            .filter_map(|s| {
                let s = match &resized {
                    Some(r) if r.id == s.id => Cow::Owned(r.clone()),
                    _ => Cow::Borrowed(s),
                };
                let (x, y) = match preview {
                    Some((id, x, y)) if *id == s.id => (x, y),
                    _ => (s.x, s.y),
                };
                let (fx, fy) = (x as f32, y as f32);
                // Cull on what's known without layout first: layout is the costly part.
                if fx > b.x || fx + (s.width as f32) < a.x || fy > b.y {
                    return None;
                }
                let height = self.shape_height(&s);
                if fy + height < a.y {
                    return None;
                }
                let ring = (rings.iter()).find(|r| *r.0 == s.id).map_or(0.0, |r| r.1);
                let e = edited.filter(|e| e.id == s.id);
                Some(Visible {
                    fill: fills[s.color as usize],
                    layout: self.layout(&s),
                    shape: s,
                    x,
                    y,
                    height,
                    ring,
                    selection: e.map(|_| self.selection_rects()).unwrap_or_default(),
                    marked: e.and_then(|e| e.marked.clone()),
                    caret: e.and_then(|_| self.caret(now)).filter(|c| c.visible),
                })
            })
            .collect();
        let full = vp.zoom >= z.fade_end;
        Frame {
            shapes,
            text_opacity: fade,
            deepen,
            shadows: full,
            grid: full,
        }
    }
}

/// `a` moved `k` (0 to 1) of the way toward `b`, per sRGB channel, as CSS `color-mix` does.
fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    let (a, b) = (a.0.to_be_bytes(), b.0.to_be_bytes());
    let c: [u8; 4] =
        std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * k).round() as u8);
    Rgba(u32::from_be_bytes(c))
}
