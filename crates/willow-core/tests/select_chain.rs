//! Select chain (Ctrl/Cmd+L, and the shape and multi-selection menu items).
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Document, Input, Link, MenuAction, MenuEntry, Modifiers, Point, Session, Shape, ShapeId,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Chains a-b, c-d and a lone e.
fn session() -> Session {
    let at = [
        ("a", 0, 0),
        ("b", 300, 0),
        ("c", 0, 300),
        ("d", 300, 300),
        ("e", 0, 600),
    ];
    session_of(Document {
        shapes: at
            .iter()
            .map(|&(n, x, y)| Shape::new(id(n), x, y))
            .collect(),
        links: vec![Link(id("a"), id("b")), Link(id("c"), id("d"))],
        ..Document::default()
    })
}

fn sorted(s: &Session) -> Vec<String> {
    let mut v: Vec<String> = s.selection().iter().map(|i| i.0.clone()).collect();
    v.sort();
    v
}

fn right_click(s: &mut Session, x: f32, y: f32) {
    let pos = s.viewport().to_screen(Point { x, y });
    let mods = Modifiers::default();
    let button = Button::Right;
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

fn choose_select_chain(s: &mut Session) {
    let action = (s.menu().expect("menu").entries.iter())
        .find_map(|e| match e {
            MenuEntry::Item(i) if i.label == "Select chain" => Some(i.action),
            _ => None,
        })
        .expect("Select chain item");
    assert_eq!(action, MenuAction::SelectChain);
    s.handle(Input::MenuChoose(action), T);
}

#[test]
fn it_selects_the_chains_of_every_selected_shape() {
    let mut s = session();
    s.select([id("a"), id("c")]);
    s.handle(Input::SelectChain, T);
    assert_eq!(sorted(&s), ["a", "b", "c", "d"]);
}

#[test]
fn with_nothing_selected_it_does_nothing() {
    let mut s = session();
    s.handle(Input::SelectChain, T);
    assert!(s.selection().is_empty());
}

#[test]
fn it_is_no_undo_step() {
    let mut s = session();
    s.select([id("a")]);
    s.handle(Input::SelectChain, T);
    s.handle(Input::Undo, T);
    assert_eq!(sorted(&s), ["a", "b"]);
}

#[test]
fn while_editing_it_does_nothing() {
    let mut s = session();
    s.select([id("a")]);
    s.handle(Input::StartEditing, T);
    s.handle(Input::SelectChain, T);
    assert_eq!(sorted(&s), ["a"]);
}

#[test]
fn the_shape_menu_item_selects_the_chain() {
    let mut s = session();
    right_click(&mut s, 50.0, 20.0);
    choose_select_chain(&mut s);
    assert_eq!(sorted(&s), ["a", "b"]);
}

#[test]
fn the_multi_selection_menu_item_selects_all_their_chains() {
    let mut s = session();
    s.select([id("a"), id("e")]);
    right_click(&mut s, 50.0, 20.0);
    choose_select_chain(&mut s);
    assert_eq!(sorted(&s), ["a", "b", "e"]);
}

#[test]
fn menu_items_show_their_shortcuts() {
    let mut s = session();
    right_click(&mut s, 50.0, 20.0);
    let v: Vec<(&str, Option<(bool, &str)>)> = (s.menu().unwrap().entries.iter())
        .filter_map(|e| match e {
            MenuEntry::Item(i) => Some((i.label, i.shortcut.map(|k| (k.shift, k.key)))),
            MenuEntry::Heading(h, k) => Some((h, k.map(|k| (k.shift, k.key)))),
            _ => None,
        })
        .collect();
    assert!(v.contains(&("Select chain", Some((false, "L")))));
    assert!(v.contains(&("Color", Some((true, "C")))));
}
