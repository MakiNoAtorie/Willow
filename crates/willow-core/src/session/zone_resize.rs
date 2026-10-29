//! Zone resizing: dragging a zone's edges and corners. Members and nesting never change.

use super::{DRAG_START, EDGE_GRAB, Gesture, Session};
use crate::input::Cursor;
use crate::{Point, Shape, ShapeId, Zone};

/// Which edges a press holds: -1 left or top, 1 right or bottom, 0 neither, per axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Grab {
    x: i8,
    y: i8,
}

impl Grab {
    pub(super) fn cursor(self) -> Cursor {
        match (self.x, self.y) {
            (_, 0) => Cursor::ResizeEw,
            (0, _) => Cursor::ResizeNs,
            (x, y) if x == y => Cursor::ResizeNwse,
            _ => Cursor::ResizeNesw,
        }
    }
}

/// A zone's previewed x, y, width and height, in canvas units.
pub(super) type Rect = (i32, i32, u32, u32);

impl Session {
    /// The topmost zone whose edge or corner is within `EDGE_GRAB` of screen point `p`, and
    /// which. Nothing under a shape; a zone's interior hides the edges of zones below it.
    pub(super) fn zone_edge_at(&self, p: Point) -> Option<(usize, Grab)> {
        if self.shape_at(p).is_some() {
            return None;
        }
        let (g, z) = (EDGE_GRAB, self.viewport.zoom);
        let top_left = |zone: &Zone| {
            self.viewport.to_screen(Point {
                x: zone.x as f32,
                y: zone.y as f32,
            })
        };
        let (i, zone) = self.doc.zones.iter().enumerate().rev().find(|(_, zone)| {
            let tl = top_left(zone);
            (tl.x - g..=tl.x + zone.width as f32 * z + g).contains(&p.x)
                && (tl.y - g..=tl.y + zone.height as f32 * z + g).contains(&p.y)
        })?;
        let tl = top_left(zone);
        let axis = |p: f32, lo: f32, len: u32| {
            if (p - lo).abs() <= g {
                -1
            } else if (p - lo - len as f32 * z).abs() <= g {
                1
            } else {
                0
            }
        };
        let grab = Grab {
            x: axis(p.x, tl.x, zone.width),
            y: axis(p.y, tl.y, zone.height),
        };
        (grab != Grab { x: 0, y: 0 }).then_some((i, grab))
    }

    /// Updates the previewed rectangle of the zone resize for the pointer at `pos`.
    pub(super) fn zone_resize_to(&mut self, pos: Point) {
        let Some(Gesture::ZoneResize {
            index,
            grab,
            from,
            to,
        }) = self.gesture
        else {
            return;
        };
        let (d, z) = (pos - from, self.viewport.zoom);
        if to.is_none() && d.x.hypot(d.y) < DRAG_START {
            return;
        }
        let min = self.zone_min() as i32;
        let zone = &self.doc.zones[index];
        let (dx, dy) = ((d.x / z).round() as i32, (d.y / z).round() as i32);
        // One axis: the low edge moves the position and keeps the far edge fixed.
        let axis = |at: i32, len: u32, d: i32, g: i8| match g {
            -1 => {
                let d = d.min(len as i32 - min);
                (at + d, (len as i32 - d) as u32)
            }
            1 => (at, (len as i32 + d).max(min) as u32),
            _ => (at, len),
        };
        let ((x, w), (y, h)) = (
            axis(zone.x, zone.width, dx, grab.x),
            axis(zone.y, zone.height, dy, grab.y),
        );
        if let Some(Gesture::ZoneResize { to, .. }) = &mut self.gesture {
            *to = Some((x, y, w, h));
        }
    }

    /// The least a zone may measure either way: the minimum width of an empty shape.
    fn zone_min(&self) -> u32 {
        self.min_width(&Shape::new(ShapeId(String::new()), 0, 0))
    }

    /// The zone being resized as the drag would leave it; the document still holds the old one.
    pub(crate) fn zone_resize_preview(&self) -> Option<Zone> {
        let Some(Gesture::ZoneResize {
            index,
            to: Some((x, y, width, height)),
            ..
        }) = self.gesture
        else {
            return None;
        };
        Some(Zone {
            x,
            y,
            width,
            height,
            ..self.doc.zones[index].clone()
        })
    }

    /// Resizes zone `index` to `r` as one undo step. Members, nesting and shapes stay.
    pub(super) fn resize_zone(&mut self, index: usize, (x, y, width, height): Rect) {
        let id = self.doc.zones[index].id.clone();
        self.history.step(&mut self.doc, |h, doc| {
            h.change_zone(doc, &id, |z| {
                (z.x, z.y, z.width, z.height) = (x, y, width, height)
            });
        });
    }
}
