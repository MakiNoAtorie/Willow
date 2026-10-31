//! The Tab directional picker: four keycaps around the selected shape; an arrow creates a
//! linked shape on that side as a midpoint click does.

use super::Session;
use super::midpoints::edge_midpoint;
use crate::Side;
use crate::input::{Input, Point, Response};

impl Session {
    /// Opens the picker over the one selected shape, ending its editing first. Nothing with
    /// no shape or several selected.
    pub(super) fn open_tab_picker(&mut self) {
        if self.only_selected().is_none() {
            return;
        }
        self.stop_editing();
        self.menu = None;
        self.tab_picker = true;
    }

    /// Whether the Tab picker is open.
    pub fn tab_picker_open(&self) -> bool {
        self.tab_picker
    }

    /// `input` while the picker is open: an arrow creates, a press or an Edit-menu click closes
    /// it and goes on (`None`), pointer motion, wheel, pinch and key-ups go on, anything else
    /// (any other key) closes it and is consumed.
    pub(super) fn tab_picker_input(&mut self, input: &Input) -> Option<Response> {
        match input {
            Input::PointerMove { .. }
            | Input::PointerUp { .. }
            | Input::PointerLeft
            | Input::Wheel { .. }
            | Input::Pinch { .. }
            | Input::KeyUp(_)
            | Input::FocusLost
            | Input::ModifiersChanged => None,
            // Keys never get here as these (the app takes every key while it is open), so they
            // come from clicks on the app's Edit menu.
            Input::PointerDown { .. }
            | Input::Undo
            | Input::Redo
            | Input::Cut
            | Input::Paste(_) => {
                self.tab_picker = false;
                None
            }
            &Input::PickSide(side) => {
                self.tab_picker = false;
                if let Some(id) = self.only_selected() {
                    self.create_linked(&id, side);
                    if let Some(new) = self.only_selected() {
                        self.reveal(&new);
                    }
                }
                Some(self.respond(true))
            }
            _ => {
                self.tab_picker = false;
                Some(self.respond(true))
            }
        }
    }

    /// The picker's sides and the selected shape's edge midpoints they hang from, canvas
    /// units; empty while closed.
    pub(crate) fn tab_picker(&self) -> Vec<(Side, Point)> {
        let Some(s) = (self.only_selected().filter(|_| self.tab_picker))
            .and_then(|id| self.index_of(&id))
            .map(|i| &self.doc.shapes[i])
        else {
            return Vec::new();
        };
        let r = self.drawn(s);
        [Side::Top, Side::Right, Side::Bottom, Side::Left]
            .map(|side| (side, edge_midpoint(r, side)))
            .to_vec()
    }
}
