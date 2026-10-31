//! The shape clipboard. Copy writes the selected shapes as a Willow document; Paste reads one
//! back as copies with fresh IDs, or plain text as one shape; Duplicate copies beside the
//! originals. The app moves the text to and from the system clipboard.

use std::collections::{HashMap, HashSet};

use super::Session;
use crate::input::Point;
use crate::{Color, Document, Link, Shape, ShapeId, format};

impl Session {
    /// What Copy and Cut put on the clipboard: while the code panel has focus, its selected
    /// text; while editing, the selected text; otherwise the selected shapes as a whole
    /// `willow 1` document with the links among them, their colors and layout, and no zones.
    /// Its `@layout` lines hold the drawn positions: a copy is what the writer saw.
    pub fn clipboard_text(&self) -> Option<String> {
        if self.panel.focused {
            return self.panel_selected_text();
        }
        if self.editing.is_some() {
            return self.selected_text();
        }
        if self.selected.is_empty() {
            return None;
        }
        let doc = self.copied(&self.selected);
        let heights: Vec<f32> = doc.shapes.iter().map(|s| self.shape_height(s)).collect();
        Some(format::save(&doc, &heights, &[]))
    }

    /// Copies of the shapes `ids` (document order) placed where the originals are drawn, and
    /// the links among them.
    fn copied(&self, ids: &[ShapeId]) -> Document {
        let set: HashSet<&ShapeId> = ids.iter().collect();
        Document {
            shapes: (self.doc.shapes.iter())
                .filter(|s| set.contains(&s.id))
                .map(|s| {
                    let (x, y) = self.drawn_at(s);
                    Shape { x, y, ..s.clone() }
                })
                .collect(),
            links: (self.doc.links.iter())
                .filter(|Link(a, b)| set.contains(a) && set.contains(b))
                .cloned()
                .collect(),
            ..Document::default()
        }
    }

    /// Pastes clipboard `text` with nothing being edited, at the open menu's point, else the
    /// pointer, else the view center. Willow text gives copies whose bounding box is centered
    /// there; other text gives one neutral shape holding it. They join the innermost zone
    /// there and become the selection; one undo step, nothing pushed.
    pub(super) fn paste_shapes(&mut self, text: &str) {
        let at = self.paste_point();
        let (mut shapes, links) = match format::load(text.as_bytes()) {
            Ok(l) => {
                let mut d = l.document;
                // Fresh IDs before measuring: the layout cache is keyed by ID.
                let fresh = self.doc.next_ids(d.shapes.len());
                remap(&mut d.shapes, &mut d.links, fresh);
                format::place(&mut d, &l.unplaced, self.theme, |s| self.shape_height(s));
                (d.shapes, d.links)
            }
            Err(_) => {
                let text = clean(text);
                if text.is_empty() {
                    return;
                }
                let mut s = Shape::new(self.doc.next_id(), 0, 0);
                s.text.replace(0..0, &text);
                s.color = Color::Neutral;
                (vec![s], Vec::new())
            }
        };
        let Some((lo, hi)) = (shapes.iter())
            .map(|s| {
                let (x, y) = (s.x as f32, s.y as f32);
                ((x, y), (x + s.width as f32, y + self.shape_height(s)))
            })
            .reduce(|(a, b), (c, d)| ((a.0.min(c.0), a.1.min(c.1)), (b.0.max(d.0), b.1.max(d.1))))
        else {
            return;
        };
        let dx = (at.x - (lo.0 + hi.0) / 2.0).round() as i32;
        let dy = (at.y - (lo.1 + hi.1) / 2.0).round() as i32;
        for s in &mut shapes {
            (s.x, s.y) = (s.x + dx, s.y + dy);
        }
        let zone = self.innermost_zone(at);
        let copies = shapes.into_iter().map(|s| (s, zone.clone())).collect();
        self.insert_copies(copies, links);
    }

    /// Copies the selected shapes one creation gap right of their bounding box, each in its
    /// original's zone; they become the selection. One undo step, nothing pushed.
    pub(super) fn duplicate(&mut self) {
        if self.editing.is_some() || self.selected.is_empty() {
            return;
        }
        let Document {
            mut shapes,
            mut links,
            ..
        } = self.copied(&self.selected);
        let zones: Vec<Option<ShapeId>> = (shapes.iter())
            .map(|s| self.doc.parent_zone(&s.id).cloned())
            .collect();
        let lo = shapes.iter().map(|s| s.x).min().unwrap_or(0);
        let hi = (shapes.iter().map(|s| s.x + s.width as i32))
            .max()
            .unwrap_or(0);
        let dx = hi - lo + self.theme.creation_gap.round() as i32;
        let fresh = self.doc.next_ids(shapes.len());
        remap(&mut shapes, &mut links, fresh);
        for s in &mut shapes {
            s.x += dx;
        }
        self.insert_copies(shapes.into_iter().zip(zones).collect(), links);
    }

    /// Adds `copies` on top, each joining its zone, then `links`, as one undo step, and
    /// selects them.
    fn insert_copies(&mut self, copies: Vec<(Shape, Option<ShapeId>)>, links: Vec<Link>) {
        let ids: Vec<ShapeId> = copies.iter().map(|(s, _)| s.id.clone()).collect();
        self.history.step(&mut self.doc, |h, doc| {
            for (s, zone) in copies {
                let id = s.id.clone();
                h.add(doc, s);
                if let Some(z) = zone {
                    h.join_zone(doc, &z, &id);
                }
            }
            for Link(a, b) in &links {
                h.link(doc, a, b);
            }
        });
        self.select(ids);
    }

    /// Where Paste puts things, in canvas units: the open menu's point (closing it), else
    /// the pointer while it is over the canvas, else the view center.
    fn paste_point(&mut self) -> Point {
        if let Some(m) = self.menu.take() {
            return m.canvas;
        }
        let size = self.viewport.size;
        let inside = |p: &Point| (0.0..=size.x).contains(&p.x) && (0.0..=size.y).contains(&p.y);
        let screen = (self.pointer.filter(inside)).unwrap_or(Point {
            x: size.x / 2.0,
            y: size.y / 2.0,
        });
        self.viewport.to_canvas(screen)
    }
}

/// What pasting `text` into a shape being edited inserts: for Willow text, its shapes' texts
/// joined by line breaks; otherwise the text itself.
pub(super) fn as_text(text: String) -> String {
    match format::load(text.as_bytes()) {
        Ok(l) if !l.document.shapes.is_empty() => (l.document.shapes.iter())
            .map(|s| s.text.plain())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => text,
    }
}

/// Gives `shapes` the IDs `fresh` in order and points `links` at them.
fn remap(shapes: &mut [Shape], links: &mut [Link], fresh: Vec<ShapeId>) {
    let map: HashMap<ShapeId, ShapeId> = (shapes.iter_mut().zip(fresh))
        .map(|(s, n)| (std::mem::replace(&mut s.id, n.clone()), n))
        .collect();
    for Link(a, b) in links {
        (*a, *b) = (map[a].clone(), map[b].clone());
    }
}

/// Clipboard text as shape text: line breaks normalized, other control characters dropped.
pub(super) fn clean(text: &str) -> String {
    (text.replace("\r\n", "\n").replace('\r', "\n").chars())
        .filter(|&c| c == '\n' || !c.is_control())
        .collect()
}
