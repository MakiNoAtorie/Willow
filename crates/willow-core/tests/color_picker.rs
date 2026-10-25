//! The keyboard color picker: Ctrl/Cmd+Shift+C, Left/Right, Enter, Esc.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Color, Input, Link, MenuEntry, MenuKind, Session, Shape, ShapeId};

const T: Duration = Duration::ZERO;

fn ring(s: &Session) -> Option<Color> {
    s.menu()?.entries.iter().find_map(|e| match e {
        MenuEntry::Colors { current } => *current,
        _ => None,
    })
}

fn colors(s: &Session) -> Vec<Color> {
    s.document().shapes.iter().map(|x| x.color).collect()
}

fn two_linked() -> Session {
    let mut doc = doc_with("a");
    let a = doc.shapes[0].id.clone();
    doc.shapes.push(Shape::new(ShapeId("b".into()), 300, 0));
    doc.links.push(Link(a.clone(), ShapeId("b".into())));
    let mut s = session_of(doc);
    s.select([a]);
    s
}

#[test]
fn ring_starts_on_current_color_and_stops_at_the_ends() {
    let mut s = two_linked();
    let cur = s.document().shapes[0].color;
    s.handle(Input::OpenColorPicker, T);
    assert_eq!(s.menu().unwrap().kind, MenuKind::Picker);
    assert_eq!(ring(&s), Some(cur));
    for _ in 0..10 {
        s.handle(Input::PickerStep(1), T);
    }
    assert_eq!(ring(&s), Some(Color::Lilac));
    for _ in 0..10 {
        s.handle(Input::PickerStep(-1), T);
    }
    assert_eq!(ring(&s), Some(Color::Neutral));
}

#[test]
fn enter_recolors_the_chain_as_one_undo_step_and_esc_changes_nothing() {
    let mut s = two_linked();
    let before = colors(&s);
    s.handle(Input::OpenColorPicker, T);
    s.handle(Input::PickerStep(1), T);
    s.handle(Input::CloseMenu, T);
    assert!(s.menu().is_none());
    assert_eq!(colors(&s), before);
    s.handle(Input::OpenColorPicker, T);
    s.handle(Input::PickerStep(1), T);
    let want = ring(&s).unwrap();
    s.handle(Input::PickerApply, T);
    assert!(s.menu().is_none());
    assert!(colors(&s).iter().all(|&c| c == want));
    s.handle(Input::Undo, T);
    assert_eq!(colors(&s), before);
}

#[test]
fn nothing_selected_or_multi_selection_opens_nothing() {
    let mut s = two_linked();
    s.select([]);
    s.handle(Input::OpenColorPicker, T);
    assert!(s.menu().is_none());
    s.select([ShapeId("a".into()), ShapeId("b".into())]);
    s.handle(Input::OpenColorPicker, T);
    assert!(s.menu().is_none());
}

#[test]
fn never_opens_while_editing() {
    let mut s = two_linked();
    s.handle(Input::StartEditing, T);
    s.handle(Input::OpenColorPicker, T);
    assert!(s.menu().is_none());
}
