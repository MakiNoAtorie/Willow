//! Pointer press, move and release on the canvas: the gestures `Session::dispatch` hands over.

use super::{Gesture, Session, past_drag_start};
use crate::input::{Button, Modifiers, Point, Response};

impl Session {
    pub(super) fn pointer_down(&mut self, pos: Point, button: Button, mods: Modifiers) -> Response {
        // A press outside the menu closes it; a right press goes on to open another.
        if self.menu.take().is_some() && button != Button::Right {
            return self.respond(true);
        }
        let hit = self.shape_at(pos);
        if button == Button::Right && self.gesture.is_none() {
            match hit {
                None if let Some(l) = self.connector_at(pos) => self.open_connector_menu(pos, l),
                None if let Some(i) = self.band_at(pos) => self.open_zone_menu(pos, i),
                None => self.open_empty_menu(pos),
                Some(i) => self.open_shape_menu(pos, i),
            }
            return self.respond(true);
        }
        if self.gesture.is_none() && button == Button::Left && !self.space_held {
            if let Some((index, side)) = self.midpoint_at(pos) {
                let (from, dragged) = (pos, false);
                self.gesture = Some(Gesture::Midpoint {
                    index,
                    side,
                    from,
                    dragged,
                });
                return self.respond(true);
            }
            if let Some(i) = hit.filter(|_| mods.alt && self.editing.is_none()) {
                let id = self.doc.shapes[i].id.clone();
                self.autofit(&id);
                return self.respond(true);
            }
            if let Some((index, edge)) = self.edge_at(pos) {
                self.stop_editing();
                self.select([self.doc.shapes[index].id.clone()]);
                self.gesture = Some(Gesture::Resize {
                    index,
                    edge,
                    from: pos,
                    to: None,
                });
                return self.respond(true);
            }
            if let Some((index, grab)) = self.zone_edge_at(pos) {
                self.stop_editing();
                self.select([]);
                self.gesture = Some(Gesture::ZoneResize {
                    index,
                    grab,
                    from: pos,
                    to: None,
                });
                return self.respond(true);
            }
            if hit.is_none() && mods.shift {
                self.stop_editing();
                self.start_marquee(pos);
                return self.respond(true);
            }
        }
        if self.gesture.is_none()
            && let Some(e) = &self.editing
        {
            // Panning is ignored while editing; a left press leaves editing unless it
            // lands in the edited text.
            if button != Button::Left {
                return self.respond(false);
            }
            if hit.is_some_and(|i| self.doc.shapes[i].id == e.id) {
                let caret = self.caret_at(pos);
                if mods.shift {
                    self.extend_to(caret, None);
                } else {
                    self.set_caret(caret);
                }
                self.gesture = Some(Gesture::Text);
                return self.respond(true);
            }
            self.stop_editing();
            if hit.is_none() && self.band_at(pos).is_none() {
                return self.respond(true);
            }
        }
        let band = (hit.is_none() && !self.space_held && button == Button::Left)
            .then(|| self.band_at(pos))
            .flatten();
        let gesture = match (button, hit) {
            _ if self.gesture.is_some() => None,
            // Zones are never selected: a band press clears the selection.
            (Button::Left, None) if let Some(index) = band => {
                self.select([]);
                Some(Gesture::Band {
                    index,
                    from: pos,
                    to: None,
                })
            }
            (Button::Left, Some(index)) if !self.space_held => Some(Gesture::Press {
                index,
                from: pos,
                to: None,
            }),
            (Button::Left | Button::Middle, hit) => Some(Gesture::Pan {
                button,
                last: pos,
                click: (button == Button::Left && hit.is_none()).then_some(pos),
            }),
            (Button::Right, _) => None,
        };
        let redraw = matches!(gesture, Some(Gesture::Pan { .. } | Gesture::Band { .. }));
        if gesture.is_some() {
            self.gesture = gesture;
        }
        self.respond(redraw)
    }

    pub(super) fn pointer_move(&mut self, pos: Point) -> Response {
        match &mut self.gesture {
            Some(Gesture::Resize { .. }) => {
                self.resize_to(pos);
                self.respond(true)
            }
            Some(Gesture::Press { .. }) => {
                let dragged = self.drag_to(pos);
                self.respond(dragged)
            }
            Some(Gesture::ZoneResize { .. }) => {
                self.zone_resize_to(pos);
                self.respond(true)
            }
            Some(Gesture::Marquee { .. }) => {
                self.marquee_to(pos);
                self.respond(true)
            }
            Some(Gesture::Pan { last, click, .. }) => {
                if click.is_some_and(|c| past_drag_start(pos - c)) {
                    *click = None;
                }
                let d = pos - *last;
                *last = pos;
                self.viewport.pan(d);
                self.respond(true)
            }
            Some(Gesture::Band { index, from, to }) => {
                let d = pos - *from;
                if to.is_none() && !past_drag_start(d) {
                    return self.respond(false);
                }
                let (zone, z) = (&self.doc.zones[*index], self.viewport.zoom);
                *to = Some((
                    (zone.x as f32 + d.x / z).round() as i32,
                    (zone.y as f32 + d.y / z).round() as i32,
                ));
                self.respond(true)
            }
            Some(Gesture::Text) => {
                let caret = self.caret_at(pos);
                self.extend_to(caret, None);
                self.respond(true)
            }
            Some(Gesture::Midpoint { from, dragged, .. }) => {
                *dragged |= past_drag_start(pos - *from);
                if *dragged {
                    let redraw = self.drag_link(pos);
                    return self.respond(redraw);
                }
                self.respond(false)
            }
            None => {
                let edge_changed = self.track_edges(pos);
                let over = self.editing.as_ref().is_some_and(|e| {
                    (self.shape_at(pos)).is_some_and(|i| self.doc.shapes[i].id == e.id)
                });
                let changed = over != self.over_text;
                let changed = self.track_hover(pos) || changed;
                let changed = self.track_connector(pos) || changed;
                self.over_text = over;
                self.respond(changed || edge_changed)
            }
        }
    }

    pub(super) fn pointer_up(&mut self, button: Button, mods: Modifiers) -> Response {
        match self.gesture {
            Some(Gesture::Pan {
                button: b, click, ..
            }) if b == button => {
                self.gesture = None;
                if click.is_some() {
                    self.select([]);
                }
            }
            Some(Gesture::Press { .. })
                if button == Button::Left
                    && let Some((dx, dy)) = self.group_drag() =>
            {
                self.gesture = None;
                let ids = self.selected.clone();
                self.drop_shapes(&ids, dx, dy);
            }
            Some(Gesture::Press { index, to, from }) if button == Button::Left => {
                self.gesture = None;
                let id = self.doc.shapes[index].id.clone();
                match to {
                    Some(to) => {
                        let (dx, dy) = self.dragged(index, to);
                        self.drop_shapes(&[id], dx, dy);
                    }
                    None if mods.shift => self.toggle_selected(id),
                    // A click on the selected shape edits it, caret where clicked.
                    None if self.selected == [id.clone()] => {
                        self.start_editing(id);
                        let caret = self.caret_at(from);
                        self.set_caret(caret);
                    }
                    None => self.select([id]),
                }
            }
            Some(Gesture::Text) if button == Button::Left => self.gesture = None,
            Some(Gesture::Band { index, to, .. }) if button == Button::Left => {
                self.gesture = None;
                if let Some((x, y)) = to {
                    let z = &self.doc.zones[index];
                    let (dx, dy) = (x - z.x, y - z.y);
                    self.drop_zone_by(index, dx, dy);
                }
            }
            Some(Gesture::Midpoint {
                index,
                side,
                dragged,
                ..
            }) if button == Button::Left => {
                self.gesture = None;
                if !dragged {
                    let id = self.doc.shapes[index].id.clone();
                    self.create_linked(&id, side);
                } else {
                    self.drop_link(index);
                }
            }
            Some(Gesture::Marquee { .. }) if button == Button::Left => self.gesture = None,
            Some(Gesture::ZoneResize { index, to, .. }) if button == Button::Left => {
                self.gesture = None;
                if let Some(r) = to {
                    self.resize_zone(index, r);
                }
            }
            Some(Gesture::Resize { index, to, .. }) if button == Button::Left => {
                self.gesture = None;
                if let Some(r) = to {
                    let id = self.doc.shapes[index].id.clone();
                    self.resize_shape(&id, r);
                }
            }
            _ => {}
        }
        self.respond(true)
    }
}
