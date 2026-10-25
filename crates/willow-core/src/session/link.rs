//! Linking by dragging from a midpoint onto another shape, and the connector under the
//! pointer: its hover highlight and its menu's Remove link.

use std::collections::HashSet;

use super::menu::{Menu, MenuAction, MenuEntry, MenuItem, MenuKind};
use super::midpoints::edge_midpoint;
use super::{Gesture, Session};
use crate::draw::LinkDrag;
use crate::input::Point;
use crate::{Link, ShapeId, side};

/// A link drag in progress and the hovered connector.
#[derive(Debug, Default)]
pub(super) struct Links {
    pub(super) drag: Option<Drag>,
    /// The link whose connector the pointer is over (or whose menu is open).
    hover: Option<Link>,
}

/// A midpoint drag past `DRAG_START`: the pointer in screen pixels, and the source's chain
/// (refused targets), taken once: a document change ends the drag (`Session::handle`).
#[derive(Debug)]
pub(super) struct Drag {
    to: Point,
    chain: HashSet<ShapeId>,
}

impl Session {
    /// Follows a midpoint drag to screen point `p`; true (redraw) always, the band moves.
    pub(super) fn drag_link(&mut self, p: Point) -> bool {
        let Some(Gesture::Midpoint { index, .. }) = self.gesture else {
            return false;
        };
        match &mut self.links.drag {
            Some(d) => d.to = p,
            None => {
                let id = &self.doc.shapes[index].id;
                let chain = self.doc.chain(id).into_iter().cloned().collect();
                self.links.drag = Some(Drag { to: p, chain });
            }
        }
        true
    }

    /// Ends a midpoint drag from the shape at `index`: links it to a valid target under the
    /// pointer as one undo step, else nothing.
    pub(super) fn drop_link(&mut self, index: usize) {
        let target = self.link_target();
        self.links.drag = None;
        if let Some(t) = target {
            let (a, b) = (
                self.doc.shapes[index].id.clone(),
                self.doc.shapes[t].id.clone(),
            );
            self.stop_editing();
            self.link(&a, &b);
        }
    }

    /// The shape under the dragged pointer that the drag's source may link to: not itself,
    /// nor anything in its chain (linked already, or a cycle).
    fn link_target(&self) -> Option<usize> {
        let d = self.links.drag.as_ref()?;
        let t = self.shape_at(d.to)?;
        (!d.chain.contains(&self.doc.shapes[t].id)).then_some(t)
    }

    /// The band of a midpoint drag in progress, from the midpoint dragged to the pointer.
    pub(crate) fn link_drag(&self) -> Option<LinkDrag<'_>> {
        let (Some(Gesture::Midpoint { index, side, .. }), Some(d)) =
            (self.gesture, &self.links.drag)
        else {
            return None;
        };
        let s = &self.doc.shapes[index];
        Some(LinkDrag {
            from: edge_midpoint(s, self.shape_height(s), side),
            to: self.viewport.to_canvas(d.to),
            target: self.link_target().map(|t| &self.doc.shapes[t].id),
        })
    }

    /// The link whose connector is within the theme's hit tolerance of screen point `p`,
    /// unless a shape or a midpoint there hides it.
    // ponytail: tests every link per pointer move; cull by bounds if huge documents lag.
    pub(super) fn connector_at(&self, p: Point) -> Option<Link> {
        if self.shape_at(p).is_some() || self.midpoint_at(p).is_some() {
            return None;
        }
        let (shapes, vp) = (&self.doc.shapes, self.viewport);
        let hit = self.theme.connector.hit;
        let ends = self.link_ends();
        // Topmost first: later links draw later.
        let k = (0..ends.len()).rev().find(|&k| {
            let (Some(a), Some(b)) = (shapes.get(ends[k].0), shapes.get(ends[k].1)) else {
                return false;
            };
            let (ah, bh) = (self.shape_height(a), self.shape_height(b));
            let (ac, bc) = (a.center(ah), b.center(bh));
            let from = vp.to_screen(edge_midpoint(a, ah, side(ac, bc)));
            let to = vp.to_screen(edge_midpoint(b, bh, side(bc, ac)));
            distance(p, from, to) <= hit
        })?;
        Some(self.doc.links[k].clone())
    }

    /// Follows the pointer at `p` for the connector highlight; true if that changed. Held
    /// while a menu is open, so the connector menu's link stays lit.
    pub(super) fn track_connector(&mut self, p: Point) -> bool {
        if self.menu.is_some() {
            return false;
        }
        let hover = (!self.space_held).then(|| self.connector_at(p)).flatten();
        hover != std::mem::replace(&mut self.links.hover, hover.clone())
    }

    /// Whether the connector between `a` and `b` is highlighted: hovered, no gesture running.
    pub(crate) fn connector_hovered(&self, a: &ShapeId, b: &ShapeId) -> bool {
        let Some(Link(x, y)) = self.links.hover.as_ref().filter(|_| self.gesture.is_none()) else {
            return false;
        };
        (x, y) == (a, b) || (x, y) == (b, a)
    }

    /// Opens the connector menu at screen point `at` for `link`, leaving any editing.
    pub(super) fn open_connector_menu(&mut self, at: Point, link: Link) {
        self.stop_editing();
        self.links.hover = Some(link.clone());
        let remove = MenuItem {
            action: MenuAction::RemoveLink,
            label: "Remove link",
            shortcut: None,
            enabled: true,
            destructive: false,
        };
        let entries = vec![MenuEntry::Item(remove)];
        self.menu = Some(Menu {
            link: Some(link),
            ..self.menu_at(MenuKind::Connector, at, entries)
        });
    }
}

/// Distance from `p` to the segment `a`..`b`.
fn distance(p: Point, a: Point, b: Point) -> f32 {
    let (d, ap) = (b - a, p - a);
    let len2 = d.x * d.x + d.y * d.y;
    let t = if len2 > 0.0 {
        ((ap.x * d.x + ap.y * d.y) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (ap.x - t * d.x).hypot(ap.y - t * d.y)
}
