//! Select All has something to select only while text is being edited.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::Input;

const T: Duration = Duration::ZERO;

#[test]
fn select_all_is_available_only_while_shape_or_panel_text_is_edited() {
    let mut s = session_of(doc_with("ab"));
    s.select([s.document().shapes[0].id.clone()]);
    assert!(!s.can_select_all(), "the canvas has no text to select");

    s.handle(Input::StartEditing, T);
    assert!(s.can_select_all(), "editing the shape's text");
    s.handle(Input::Escape, T);
    assert!(!s.can_select_all(), "back on the canvas");

    s.handle(Input::TogglePanel, T);
    assert!(s.can_select_all(), "the panel has the keys");
}
