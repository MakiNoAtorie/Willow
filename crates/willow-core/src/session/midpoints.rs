//! Midpoints: the circles on a hovered shape's edge midpoints, and creating a linked shape
//! from one.

use std::time::Duration;

use super::{DrawnRect, Gesture, Session};
use crate::input::Point;
use crate::{Shape, ShapeId, Side};

/// Screen pixels around a midpoint that hit it; it wins over the resize edge it sits on.
const MIDPOINT_HIT: f32 = 10.0;

const SIDES: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

/// Which shape shows midpoints, and the create animation.
#[derive(Debug, Default)]
pub(super) struct Midpoints {
    /// The shape the pointer is near; `Session::midpoints` decides whether they show.
    hover: Option<ShapeId>,
    /// The shape last created from a midpoint, and when.
    created: Option<(ShapeId, Duration)>,
}

/// The midpoint of `side` of a drawn rectangle, in canvas units.
pub(super) fn edge_midpoint(r: DrawnRect, side: Side) -> Point {
    let DrawnRect {
        x,
        y,
        width: w,
        height: h,
    } = r;
    match side {
        Side::Top => Point { x: x + w / 2.0, y },
        Side::Bottom => Point {
            x: x + w / 2.0,
            y: y + h,
        },
        Side::Left => Point { x, y: y + h / 2.0 },
        Side::Right => Point {
            x: x + w,
            y: y + h / 2.0,
        },
    }
}

impl Session {
    /// Index of the topmost shape within `MIDPOINT_HIT` of screen point `p`, whose midpoints
    /// `p` may hit; `None` when that is the shape being edited (it has none).
    fn near(&self, p: Point) -> Option<usize> {
        let (c, r) = (
            self.viewport.to_canvas(p),
            MIDPOINT_HIT / self.viewport.zoom,
        );
        let i = self.doc.shapes.iter().rposition(|s| {
            let (x, y) = self.drawn_at(s);
            let (x, y) = (x as f32, y as f32);
            (x - r..=x + s.width as f32 + r).contains(&c.x)
                && (y - r..=y + self.shape_height(s) + r).contains(&c.y)
        })?;
        let edited = (self.editing.as_ref()).is_some_and(|e| e.id == self.doc.shapes[i].id);
        (!edited).then_some(i)
    }

    /// The midpoint under screen point `p`: its shape's index and side.
    pub(super) fn midpoint_at(&self, p: Point) -> Option<(usize, Side)> {
        let i = self.near(p)?;
        let (r, vp) = (self.drawn(&self.doc.shapes[i]), self.viewport);
        let hit = |&side: &Side| {
            let m = vp.to_screen(edge_midpoint(r, side));
            (m.x - p.x).hypot(m.y - p.y) <= MIDPOINT_HIT
        };
        SIDES.into_iter().find(hit).map(|side| (i, side))
    }

    /// Follows the pointer at `p` for the midpoints shown; true if that changed.
    pub(super) fn track_hover(&mut self, p: Point) -> bool {
        let near = (!self.space_held).then(|| self.near(p)).flatten();
        let hover = near.map(|i| self.doc.shapes[i].id.clone());
        hover != std::mem::replace(&mut self.mid.hover, hover.clone())
    }

    /// The hovered shape's four edge midpoints in canvas units, or none: not on the shape
    /// being edited, nor during a gesture other than a midpoint press.
    pub(crate) fn midpoints(&self) -> Vec<Point> {
        let idle = matches!(self.gesture, None | Some(Gesture::Midpoint { .. }));
        let Some(id) = self.mid.hover.as_ref().filter(|_| idle && !self.space_held) else {
            return Vec::new();
        };
        if self.editing.as_ref().is_some_and(|e| &e.id == id) {
            return Vec::new();
        }
        let Some(s) = self.doc.shapes.iter().find(|s| &s.id == id) else {
            return Vec::new();
        };
        let r = self.drawn(s);
        SIDES.map(|side| edge_midpoint(r, side)).to_vec()
    }

    /// Creates an empty shape linked to `id` on `side`, one creation gap away (placed as the
    /// loader places a linked shape) in the chain's color, and starts editing it, selected.
    /// The creation and that first editing session are one undo step.
    pub fn create_linked(&mut self, id: &ShapeId, side: Side) {
        self.stop_editing();
        let Some(a) = self.doc.shapes.iter().find(|s| &s.id == id) else {
            return;
        };
        let mut new = Shape::new(self.doc.next_id(), 0, 0);
        new.color = a.color;
        let (ah, nh, gap) = (
            self.shape_height(a),
            self.shape_height(&new),
            self.theme.creation_gap,
        );
        (new.x, new.y) = crate::chain::beside(a, ah, new.width, nh, side, gap);
        let new_id = new.id.clone();
        // Insert: the nearest shape already on that side links on from the new one instead,
        // its subtree pushed a creation gap clear of it.
        let insert = self.linked_on(id, side).into_iter().next().map(|n| {
            let j = self.index_of(&n).expect("linked");
            let src = self.index_of(id).expect("source");
            (
                self.clearance(&new, &self.doc.shapes[j], side, gap),
                self.subtree(j, src),
                n,
            )
        });
        // `start_editing`'s group is this one; `stop_editing` closes both as one step.
        self.history.begin();
        self.history.add(&mut self.doc, new);
        // It joins its source's zone, wherever it lands.
        if let Some(z) = self.doc.parent_zone(id).cloned() {
            self.history.join_zone(&mut self.doc, &z, &new_id);
        }
        self.history.link(&mut self.doc, id, &new_id);
        if let Some(((dx, dy), block, n)) = insert {
            self.push(&block, dx, dy);
            self.history.unlink(&mut self.doc, id, &n);
            self.history.link(&mut self.doc, &new_id, &n);
        }
        self.select([new_id.clone()]);
        self.start_editing(new_id.clone());
        self.mid = Midpoints {
            hover: None,
            created: Some((new_id, self.now)),
        };
    }

    /// The shape being created and the create animation's eased progress at `now`, while it
    /// runs.
    pub(crate) fn creating(&self, now: Duration) -> Option<(&ShapeId, f32)> {
        let (id, since) = self.mid.created.as_ref()?;
        let (motion, elapsed) = (&self.theme.create.motion, now.saturating_sub(*since));
        (elapsed < motion.duration).then(|| (id, motion.progress(elapsed)))
    }
}
