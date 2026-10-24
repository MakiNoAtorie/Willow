//! The draw query: what the app paints for a screen rectangle, derived from the zoom and the
//! theme's low-zoom thresholds.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use crate::{CaretView, Color, Layout, Point, Rgba, Session, Shape, ShapeId, Side, Zone, side};

/// One frame's worth of drawing: zones, then connectors, then shapes.
pub struct Frame<'a> {
    /// Zones intersecting the rectangle, parents before the zones nested in them.
    pub zones: Vec<VisibleZone<'a>>,
    /// Connectors of links that may cross the rectangle, in canvas units.
    pub connectors: Vec<Connector>,
    /// Connector stroke in canvas units: the theme's, or one screen pixel if wider.
    pub connector_width: f32,
    /// Shapes intersecting the rectangle, in draw order (later on top).
    pub shapes: Vec<Visible<'a>>,
    /// Shape text opacity: 1 at `fade_start` and above, 0 at `fade_end` and below.
    pub text_opacity: f32,
    /// How far fills have moved from pastel toward the picker color, 0 to `block_mix`.
    pub deepen: f32,
    /// Shape shadows and the dot grid are drawn (dropped below `fade_end`).
    pub shadows: bool,
    pub grid: bool,
    /// The hovered shape's edge midpoints, canvas units; paint them over every shape at the
    /// theme's `midpoint` size, constant on screen.
    pub midpoints: Vec<Point>,
    /// The marquee being drawn: top-left and bottom-right in canvas units.
    pub marquee: Option<(Point, Point)>,
    /// A midpoint drag looking for a shape to link to; paint its band over every shape.
    pub link_drag: Option<LinkDrag<'a>>,
}

/// A drag from a midpoint: a band from the midpoint to the pointer, canvas units, and the
/// shape a drop there would link to (it shows the ring); refused shapes never show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkDrag<'a> {
    pub from: Point,
    pub to: Point,
    pub target: Option<&'a ShapeId>,
}

/// A shape fading in after its creation: paint it at `opacity`, `dy` canvas units off its
/// place and scaled by `scale` around its center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appear {
    pub opacity: f32,
    pub dy: f32,
    pub scale: f32,
}

/// A link's straight line, between the midpoints of the sides its shapes face each other with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Connector {
    pub from: Point,
    pub from_side: Side,
    pub to: Point,
    pub to_side: Side,
    /// Under the pointer: paint it in the theme's hover style.
    pub hovered: bool,
}

pub struct VisibleZone<'a> {
    pub zone: &'a Zone,
    /// Inside another zone: a deeper fill.
    pub nested: bool,
    pub fill: Rgba,
    /// The inset edge's color.
    pub edge: Rgba,
    /// Title text's top-left and size in canvas units: never under the theme's minimum on screen.
    pub title_at: Point,
    pub title_size: f32,
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
    /// Set while the shape's create animation runs.
    pub appear: Option<Appear>,
    /// Canvas units to paint the shape off `x`, `y` while it slides after a push; (0, 0) at rest.
    pub slide: Point,
}

impl Session {
    /// What to draw at `now` in the screen rectangle `from`..`to` (canvas-relative screen pixels).
    pub fn draw<'a>(&'a self, from: Point, to: Point, now: Duration) -> Frame<'a> {
        let (t, vp) = (self.theme(), self.viewport());
        let z = &t.zoom;
        let fade = ((vp.zoom - z.fade_end) / (z.fade_start - z.fade_end)).clamp(0.0, 1.0);
        let deepen = z.block_mix * (1.0 - fade);
        let fills = Color::ALL.map(|c| mix(t.pastel(c), t.picker[c as usize], deepen));
        let (a, b) = (vp.to_canvas(from), vp.to_canvas(to));
        let (preview, resized) = (self.drag_preview(), self.resize_preview());
        let edited = self.editing().filter(|_| fade > 0.0);
        let group = self.group_drag();
        let moved: HashSet<&ShapeId> = group.iter().flat_map(|_| self.selection()).collect();
        let doc = self.document();
        // A shape as the gesture in progress previews it: its shape and top-left.
        let placed = |s: &'a Shape| {
            let s = match &resized {
                Some(r) if r.id == s.id => Cow::Owned(r.clone()),
                _ => Cow::Borrowed(s),
            };
            let (x, y) = match preview {
                Some(p) if *p.id == s.id => (p.x, p.y),
                _ => (s.x, s.y),
            };
            let (x, y) = match group {
                Some((dx, dy)) if moved.contains(&s.id) => (s.x + dx, s.y + dy),
                _ => (x, y),
            };
            (s, x, y)
        };
        let creating = self.creating(now);
        let link_drag = self.link_drag();
        let target = link_drag.and_then(|l| l.target);
        let mut connectors = Vec::new();
        for &(p, q) in self.link_ends().iter() {
            let (Some(p), Some(q)) = (doc.shapes.get(p), doc.shapes.get(q)) else {
                continue;
            };
            let ((p, px, py), (q, qx, qy)) = (placed(p), placed(q));
            // Cull on what's known without layout first.
            let (left, right) = (px.min(qx), (px + p.width as i32).max(qx + q.width as i32));
            if left as f32 > b.x || (right as f32) < a.x || py.min(qy) as f32 > b.y {
                continue;
            }
            let (ph, qh) = (self.shape_height(&p), self.shape_height(&q));
            if (py as f32 + ph).max(qy as f32 + qh) < a.y {
                continue;
            }
            // A sliding shape's connector end slides with it.
            let center = |s: &Shape, x: i32, y: i32, h: f32| {
                let o = self.sliding(&s.id, now);
                Point {
                    x: x as f32 + s.width as f32 / 2.0 + o.x,
                    y: y as f32 + h / 2.0 + o.y,
                }
            };
            let (pc, qc) = (center(&p, px, py, ph), center(&q, qx, qy, qh));
            let (from_side, to_side) = (side(pc, qc), side(qc, pc));
            let (mut from, mut to) = (
                midpoint(pc, p.width, ph, from_side),
                midpoint(qc, q.width, qh, to_side),
            );
            // A new shape's connector draws out from the other end.
            match creating {
                Some((id, k)) if *id == q.id => to = lerp(from, to, k),
                Some((id, k)) if *id == p.id => from = lerp(to, from, k),
                _ => {}
            }
            connectors.push(Connector {
                from,
                from_side,
                to,
                to_side,
                hovered: self.connector_hovered(&p.id, &q.id),
            });
        }
        let zone_ids: HashSet<&ShapeId> = doc.zones.iter().map(|z| &z.id).collect();
        let nested: HashSet<&ShapeId> = (doc.zones.iter())
            .flat_map(|z| &z.members)
            .filter(|m| zone_ids.contains(m))
            .collect();
        let zt = &t.zone;
        let k = 1.0 / vp.zoom.min(1.0);
        let zones = (doc.zones.iter())
            .filter(|z| {
                let (x, y) = (z.x as f32, z.y as f32);
                x <= b.x && x + z.width as f32 >= a.x && y <= b.y && y + z.height as f32 >= a.y
            })
            .map(|zone| {
                let (pastel, nested) = (t.pastel(zone.color), nested.contains(&zone.id));
                let fill = if nested {
                    zt.nested_fill_mix
                } else {
                    zt.fill_mix
                };
                VisibleZone {
                    zone,
                    nested,
                    fill: mix(t.canvas, pastel, fill),
                    edge: mix(pastel, t.picker[zone.color as usize], zt.edge_mix),
                    title_at: Point {
                        x: zone.x as f32 + zt.title_x * k,
                        y: zone.y as f32 + zt.title_y * k,
                    },
                    title_size: zt.title_size.max(zt.title_min_screen / vp.zoom),
                }
            })
            .collect();
        let shapes = (doc.shapes.iter())
            .filter_map(|s| {
                let (s, x, y) = placed(s);
                let (fx, fy) = (x as f32, y as f32);
                // Cull on what's known without layout first: layout is the costly part.
                if fx > b.x || fx + (s.width as f32) < a.x || fy > b.y {
                    return None;
                }
                let height = self.shape_height(&s);
                if fy + height < a.y {
                    return None;
                }
                let (ring, slide) = (self.ring(&s.id, now), self.sliding(&s.id, now));
                let ring = if target == Some(&s.id) { 1.0 } else { ring };
                let e = edited.filter(|e| e.id == s.id);
                let appear = (creating.filter(|(id, _)| **id == s.id)).map(|(_, k)| Appear {
                    opacity: k,
                    dy: t.create.rise * (1.0 - k),
                    scale: t.create.scale + (1.0 - t.create.scale) * k,
                });
                Some(Visible {
                    fill: match self.rippling(&s.id, now) {
                        Some((from, p)) => mix(fills[from as usize], fills[s.color as usize], p),
                        None => fills[s.color as usize],
                    },
                    layout: self.layout(&s),
                    shape: s,
                    x,
                    y,
                    height,
                    ring,
                    selection: e.map(|_| self.selection_rects()).unwrap_or_default(),
                    marked: e.and_then(|e| e.marked.clone()),
                    caret: e.and_then(|_| self.caret(now)).filter(|c| c.visible),
                    appear,
                    slide,
                })
            })
            .collect();
        let full = vp.zoom >= z.fade_end;
        Frame {
            zones,
            connectors,
            connector_width: t.connector.width.max(t.connector.min_screen / vp.zoom),
            shapes,
            text_opacity: fade,
            deepen,
            shadows: full,
            grid: full,
            midpoints: self.midpoints(),
            marquee: self.marquee(),
            link_drag,
        }
    }
}

/// The midpoint of `side` of a `w` x `h` shape centered at `c`.
fn midpoint(c: Point, w: u32, h: f32, side: Side) -> Point {
    let (dx, dy) = match side {
        Side::Top => (0.0, -h / 2.0),
        Side::Bottom => (0.0, h / 2.0),
        Side::Left => (-(w as f32) / 2.0, 0.0),
        Side::Right => (w as f32 / 2.0, 0.0),
    };
    Point {
        x: c.x + dx,
        y: c.y + dy,
    }
}

/// `a` moved `k` (0 to 1) of the way toward `b`.
fn lerp(a: Point, b: Point, k: f32) -> Point {
    Point {
        x: a.x + (b.x - a.x) * k,
        y: a.y + (b.y - a.y) * k,
    }
}

/// `a` moved `k` (0 to 1) of the way toward `b`, per sRGB channel, as CSS `color-mix` does.
fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    let (a, b) = (a.0.to_be_bytes(), b.0.to_be_bytes());
    let c: [u8; 4] =
        std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * k).round() as u8);
    Rgba(u32::from_be_bytes(c))
}
