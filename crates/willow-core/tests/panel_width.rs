mod common;

use willow_core::{Document, Point};

fn width_in(want: f32, window: f32) -> f32 {
    let mut s = common::session_of(Document::new());
    s.set_panel_width(want, window);
    s.panel().width
}

#[test]
fn width_runs_from_280_px_to_half_the_window() {
    assert_eq!(width_in(100.0, 1200.0), 280.0);
    assert_eq!(width_in(500.0, 1200.0), 500.0);
    assert_eq!(width_in(900.0, 1200.0), 600.0);
}

#[test]
fn half_the_window_wins_in_a_narrow_window() {
    assert_eq!(width_in(420.0, 500.0), 250.0);
    assert_eq!(width_in(280.0, 560.0), 280.0);
}

#[test]
fn widening_keeps_the_canvas_content_where_it_is() {
    let mut s = common::session_of(Document::new());
    s.resize(1000.0 - s.panel().width, 800.0);
    let corner = Point { x: 0.0, y: 0.0 };
    let at = s.viewport().to_screen(corner);
    s.set_panel_width(520.0, 1000.0);
    s.resize(1000.0 - s.panel().width, 800.0);
    assert_eq!(s.viewport().to_screen(corner), at);
}

#[test]
fn changing_the_width_is_not_a_document_edit() {
    let mut s = common::session_of(Document::new());
    let doc = s.document().clone();
    s.set_panel_width(500.0, 1200.0);
    assert_eq!(*s.document(), doc);
    assert!(s.is_saved() && !s.can_undo() && !s.can_redo());
}
