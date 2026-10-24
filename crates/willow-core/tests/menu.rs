//! Context menus: the request a right-click resolves to, and what New shape does.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{
    Button, Color, Document, Input, MenuAction, MenuEntry, MenuKind, Modifiers, Point, Session,
    ShapeId, Zone,
};

const T: Duration = Duration::ZERO;

fn right_click(s: &mut Session, canvas: Point) -> Point {
    let pos = s.viewport().to_screen(canvas);
    let mods = Modifiers::default();
    let button = Button::Right;
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    pos
}

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn zone(id: &str, x: i32, y: i32, w: u32, h: u32, members: &[&str]) -> Zone {
    Zone {
        id: ShapeId(id.into()),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width: w,
        height: h,
        members: members.iter().map(|m| ShapeId((*m).into())).collect(),
    }
}

#[test]
fn right_click_on_empty_space_opens_the_empty_menu_at_the_point() {
    let mut s = session_of(doc_with("a"));
    let at = right_click(&mut s, pt(600.0, 500.0));
    let menu = s.menu().expect("menu open");
    assert_eq!((menu.kind, menu.at), (MenuKind::Empty, at));
    let [MenuEntry::Item(item)] = &menu.entries[..] else {
        panic!("one item");
    };
    assert_eq!(
        (item.action, item.label),
        (MenuAction::NewShape, "New shape")
    );
}

#[test]
fn closing_and_clicking_elsewhere_dismiss_the_menu() {
    let mut s = session_of(doc_with("a"));
    right_click(&mut s, pt(600.0, 500.0));
    s.handle(Input::CloseMenu, T);
    assert!(s.menu().is_none());
    let at = right_click(&mut s, pt(600.0, 500.0));
    s.handle(
        Input::PointerDown {
            pos: at,
            button: Button::Left,
            mods: Modifiers::default(),
        },
        T,
    );
    assert!(s.menu().is_none());
}

#[test]
fn new_shape_is_centered_neutral_and_editing_and_one_undo_removes_it() {
    let mut s = session_of(doc_with("a"));
    right_click(&mut s, pt(600.0, 500.0));
    s.handle(Input::MenuChoose(MenuAction::NewShape), T);
    assert!(s.menu().is_none());
    let doc = s.document();
    assert_eq!(doc.shapes.len(), 2);
    let new = &doc.shapes[1];
    let h = s.shape_height(new);
    assert_eq!(new.color, Color::Neutral);
    assert!(new.text.paragraphs.is_empty());
    assert!((new.x as f32 + new.width as f32 / 2.0 - 600.0).abs() <= 0.5);
    assert!((new.y as f32 + h / 2.0 - 500.0).abs() <= 0.5);
    assert_eq!(s.editing().map(|e| e.id.clone()), Some(new.id.clone()));
    s.handle(
        Input::Text {
            text: "hi".into(),
            replace: None,
        },
        T,
    );
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!(s.document().shapes.len(), 1);
}

#[test]
fn new_shape_joins_the_innermost_zone_and_undo_leaves_it() {
    let doc = Document {
        zones: vec![
            zone("o", 300, 300, 600, 600, &["i"]),
            zone("i", 400, 400, 400, 400, &[]),
        ],
        ..doc_with("a")
    };
    let mut s = session_of(doc);
    right_click(&mut s, pt(600.0, 500.0));
    s.handle(Input::MenuChoose(MenuAction::NewShape), T);
    let id = s.document().shapes[1].id.clone();
    assert_eq!(s.document().zones[1].members, [id]);
    assert_eq!(s.document().zones[0].members.len(), 1);
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert!(s.document().zones[1].members.is_empty());
}

#[test]
fn outside_every_zone_it_joins_none() {
    let doc = Document {
        zones: vec![zone("o", 300, 300, 100, 100, &[])],
        ..doc_with("a")
    };
    let mut s = session_of(doc);
    right_click(&mut s, pt(600.0, 500.0));
    s.handle(Input::MenuChoose(MenuAction::NewShape), T);
    assert!(s.document().zones[0].members.is_empty());
}
