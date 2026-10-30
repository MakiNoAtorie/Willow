//! The draw query: what the app paints for a screen rectangle, derived from the zoom and the
//! theme's low-zoom thresholds.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    CaretView, Color, Layout, Paragraph, Point, Rgba, Run, Session, Shape, ShapeId, Side, Zone,
    side,
};

/// One frame's worth of drawing: zones, then connectors, then shapes.
pub struct Frame<'a> {
    /// Zones intersecting the rectangle, parents before the zones nested in them.
    pub zones: Vec<VisibleZone<'a>>,
    /// The zone whose edge shows the hover ring (`zone_ring_width` wide, `Theme.accent`,
    /// following its corner radius; paint it after every zone fill): the zone a drag in
    /// progress would make a dragged shape or zone join, or the hovered title band's zone.
    pub zone_ring: Option<&'a ShapeId>,
    /// The ring's width in canvas units: `Theme.zone.ring` on screen zoomed out, scaling with
    /// the zone zoomed in.
    pub zone_ring_width: f32,
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
    /// The Tab picker: each side's keycap hangs off this edge midpoint of the selected
    /// shape, canvas units; paint it over everything, constant on screen (`Theme.tab_picker`).
    pub tab_picker: Vec<(Side, Point)>,
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
    /// The zone as a drag in progress previews it.
    pub zone: Cow<'a, Zone>,
    /// Inside another zone: a deeper fill.
    pub nested: bool,
    pub fill: Rgba,
    /// The inset edge's color.
    pub edge: Rgba,
    /// Top-left of the title's line box and the title's size in canvas units: never under the
    /// theme's minimum on screen. The box is `Theme.line_height` times the size tall; paint the
    /// glyphs centered in it, in the bold face if `ZoneStyle::title_bold`.
    pub title_at: Point,
    pub title_size: f32,
    /// The title fits inside the zone on screen, `title_x` in on both sides, or is being
    /// edited; else paint none.
    pub show_title: bool,
    /// While its title is edited: the text selection highlight (top-left, bottom-right), the
    /// IME marked text's underline (from, to: `Theme.caret.marked_underline` screen px thick,
    /// hanging below) and the caret, unless in its blink's hidden half. Canvas units; paint
    /// the highlight under the title, the rest over it.
    pub selection: Vec<(Point, Point)>,
    pub marked: Option<(Point, Point)>,
    pub caret: Option<CaretView>,
}

pub struct Visible<'a> {
    /// The resize preview's shape while the shape is being resized.
    pub shape: Cow<'a, Shape>,
    /// Drawn top-left in canvas units: the drag preview's while the shape is dragged. Read
    /// this, not `shape.x` and `shape.y`, which are the placed position.
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
        // A multi-selection or a zone being dragged: how far, and what moves.
        let group = (self
            .group_drag()
            .map(|d| (d, self.selection().iter().collect())))
        .or_else(|| self.zone_drag());
        let (group, moved): (_, HashSet<&ShapeId>) = match group {
            Some((d, moved)) => (Some(d), moved),
            None => (None, HashSet::new()),
        };
        let doc = self.document();
        // A shape as the gesture in progress previews it: its shape and drawn top-left.
        let shown = |s: &'a Shape| {
            let s = match &resized {
                Some(r) if r.id == s.id => Cow::Owned(r.clone()),
                _ => Cow::Borrowed(s),
            };
            let (x, y) = self.drawn_at(&s);
            let (x, y) = match (group, preview) {
                (Some((dx, dy)), _) if moved.contains(&s.id) => (x + dx, y + dy),
                (_, Some(p)) if *p.id == s.id => (p.x, p.y),
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
            let ((p, px, py), (q, qx, qy)) = (shown(p), shown(q));
            // Cull on what's known without layout first.
            let (left, right) = (px.min(qx), (px + p.width as i32).max(qx + q.width as i32));
            if left as f32 > b.x || (right as f32) < a.x || py.min(qy) as f32 > b.y {
                continue;
            }
            let (ph, qh) = (self.shape_height(&p), self.shape_height(&q));
            if (py as f32 + ph).max(qy as f32 + qh) < a.y {
                continue;
            }
            // A sliding shape's connector end slides with it, a previewed one with the preview.
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
        let title_size = self.title_size();
        let zone_resized = self.zone_resize_preview();
        let zones = (doc.zones.iter())
            .map(|z| match (&zone_resized, group) {
                (Some(r), _) if r.id == z.id => Cow::Owned(r.clone()),
                (_, Some((dx, dy))) if moved.contains(&z.id) => Cow::Owned(Zone {
                    x: z.x + dx,
                    y: z.y + dy,
                    ..z.clone()
                }),
                _ => Cow::Borrowed(z),
            })
            .filter(|z| {
                let (x, y) = (z.x as f32, z.y as f32);
                x <= b.x && x + z.width as f32 >= a.x && y <= b.y && y + z.height as f32 >= a.y
            })
            .map(|zone| {
                let (pastel, nested) = (t.pastel(zone.color), nested.contains(&zone.id));
                let e = self.editing().filter(|e| e.zone && e.id == zone.id);
                let fill = if nested {
                    zt.nested_fill_mix
                } else {
                    zt.fill_mix
                };
                VisibleZone {
                    nested,
                    fill: mix(t.canvas, pastel, fill),
                    edge: mix(pastel, t.picker[zone.color as usize], zt.edge_mix),
                    title_at: self.title_at(&zone),
                    title_size,
                    show_title: e.is_some()
                        || self.title_width(&zone.title, title_size) + 2.0 * zt.title_x * k
                            <= zone.width as f32,
                    selection: e.map(|_| self.selection_rects()).unwrap_or_default(),
                    marked: e.and_then(|e| self.title_underline(e.marked.clone()?)),
                    caret: e.and_then(|_| self.caret(now)).filter(|c| c.visible),
                    zone,
                }
            })
            .collect();
        let shapes = (doc.shapes.iter())
            .filter_map(|s| {
                let (s, x, y) = shown(s);
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
            zone_ring: self.drop_zone().or(self.band_ring()),
            zone_ring_width: t.zone.ring / vp.zoom.min(1.0),
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
            tab_picker: self.tab_picker(),
        }
    }
}

impl Session {
    /// Zone titles' size in canvas units: never under the theme's minimum on screen.
    pub(crate) fn title_size(&self) -> f32 {
        let zt = &self.theme().zone;
        zt.title_size
            .max(zt.title_min_screen / self.viewport().zoom)
    }

    /// `zone`'s title line box top-left in canvas units, constant on screen zoomed out.
    pub(crate) fn title_at(&self, zone: &Zone) -> Point {
        let (zt, k) = (&self.theme().zone, 1.0 / self.viewport().zoom.min(1.0));
        Point {
            x: zone.x as f32 + zt.title_x * k,
            y: zone.y as f32 + zt.title_y * k,
        }
    }

    /// `title`'s width at `size`, canvas units. Measured on every draw: the app's word cache
    /// makes a known title a few lookups. Shaped at the shape font's size in the face titles
    /// draw with, then scaled to `size`.
    pub(crate) fn title_width(&self, title: &str, size: f32) -> f32 {
        let t = self.theme();
        let para = Paragraph {
            runs: vec![Run {
                text: title.into(),
                bold: t.zone.title_bold(),
                ..Run::default()
            }],
        };
        let clusters: Vec<_> = (title.grapheme_indices(true))
            .map(|(i, g)| i..i + g.len())
            .collect();
        let w: f32 = self.measure.advances(&para, &clusters, t).iter().sum();
        w * size / t.font_size
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
