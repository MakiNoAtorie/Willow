//! Context menus: the request a right-click resolves to, and what New shape does.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{
    Button, Color, Document, Input, MenuAction, MenuEntry, MenuKind, Modifiers, Point, Session,
    ShapeId, Shortcut, Zone,
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
    s.handle(Input::Escape, T);
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
    s.handle(Input::Escape, T);
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

/// Shapes a, b, c in a row, linked a-b-c, all neutral.
fn chain_of_three() -> Session {
    let mut doc = doc_with("a");
    for (id, x) in [("b", 300), ("c", 600)] {
        doc.shapes
            .push(willow_core::Shape::new(ShapeId(id.into()), x, 0));
    }
    let id = |s: &str| ShapeId(s.into());
    assert!(doc.link(&id("a"), &id("b")) && doc.link(&id("b"), &id("c")));
    session_of(doc)
}

fn fill_of(s: &Session, id: &str, now: Duration) -> willow_core::Rgba {
    let f = s.draw(pt(0.0, 0.0), pt(1000.0, 800.0), now);
    f.shapes.iter().find(|v| v.shape.id.0 == id).unwrap().fill
}

#[test]
fn right_click_on_an_unselected_shape_selects_it_and_marks_its_color() {
    let mut s = chain_of_three();
    s.select([ShapeId("a".into())]);
    right_click(&mut s, pt(350.0, 20.0));
    assert_eq!(s.selection(), [ShapeId("b".into())]);
    let menu = s.menu().expect("menu open");
    assert_eq!(menu.kind, MenuKind::Shape);
    let colors = MenuEntry::Colors {
        current: Some(Color::Neutral),
    };
    assert!(menu.entries.contains(&colors));
}

#[test]
fn right_click_inside_a_multi_selection_keeps_it() {
    let mut s = chain_of_three();
    s.select(["a", "b"].map(|i| ShapeId(i.into())));
    right_click(&mut s, pt(350.0, 20.0));
    assert_eq!(s.selection().len(), 2);
}

#[test]
fn recolor_paints_the_chain_as_one_undo_step() {
    let mut s = chain_of_three();
    right_click(&mut s, pt(350.0, 20.0));
    s.handle(Input::MenuChoose(MenuAction::Recolor(Color::Rose)), T);
    assert!(s.menu().is_none());
    assert!(s.document().shapes.iter().all(|x| x.color == Color::Rose));
    s.handle(Input::Undo, T);
    assert!(
        s.document()
            .shapes
            .iter()
            .all(|x| x.color == Color::Neutral)
    );
}

#[test]
fn recolor_ripples_out_from_the_clicked_shape() {
    let mut s = chain_of_three();
    let theme = s.theme();
    let (old, new) = (theme.pastel(Color::Neutral), theme.pastel(Color::Rose));
    right_click(&mut s, pt(350.0, 20.0));
    s.handle(Input::MenuChoose(MenuAction::Recolor(Color::Rose)), T);
    let ms = Duration::from_millis;
    assert!(s.animating(ms(100)));
    assert_eq!(fill_of(&s, "b", T), old);
    assert_ne!(fill_of(&s, "b", ms(100)), old);
    assert_eq!(fill_of(&s, "b", ms(200)), new);
    // The neighbours start 40 ms later per step.
    assert_eq!(fill_of(&s, "a", ms(40)), old);
    assert_eq!(fill_of(&s, "c", ms(40)), old);
    // Both neighbours are one link away, so they start together.
    assert_eq!(fill_of(&s, "a", ms(100)), fill_of(&s, "c", ms(100)));
    assert_ne!(fill_of(&s, "a", ms(100)), old);
    assert_eq!(fill_of(&s, "c", ms(240)), new);
    assert!(!s.animating(ms(240)));
}

#[test]
fn the_shape_menu_ends_with_a_color_heading_over_the_colors() {
    let mut s = chain_of_three();
    right_click(&mut s, pt(350.0, 20.0));
    let entries = &s.menu().expect("menu open").entries;
    let current = Some(Color::Neutral);
    assert_eq!(
        entries[entries.len() - 3..],
        [
            MenuEntry::Separator,
            MenuEntry::Heading(
                "Color",
                Some(Shortcut {
                    shift: true,
                    key: "C"
                })
            ),
            MenuEntry::Colors { current }
        ]
    );
}

#[test]
fn esc_closes_an_open_menu() {
    let mut s = chain_of_three();
    right_click(&mut s, pt(350.0, 20.0));
    s.handle(Input::Escape, T);
    assert!(s.menu().is_none());
    assert_eq!(s.selection(), [ShapeId("b".into())]);
}

#[test]
fn keyboard_commands_on_the_selection_close_an_open_menu() {
    use willow_core::Side;
    for input in [
        Input::Navigate(Side::Right),
        // Nothing above b: the selection stays, but the closed menu still redraws.
        Input::Navigate(Side::Top),
        Input::Nudge(Side::Right),
        Input::NudgeSize(Side::Right),
        Input::SelectChain,
        Input::Enter,
    ] {
        let mut s = chain_of_three();
        right_click(&mut s, pt(350.0, 20.0));
        assert!(s.handle(input.clone(), T).redraw, "{input:?}");
        assert!(s.menu().is_none(), "{input:?}");
        // So a Cut chosen next cuts nothing the menu wasn't opened on.
        s.handle(Input::MenuChoose(MenuAction::Cut), T);
        assert_eq!(s.document().shapes.len(), 3, "{input:?}");
    }
}

#[test]
fn ctrl_arrows_do_nothing_while_the_color_picker_is_open() {
    use willow_core::Side;
    for input in [Input::Nudge(Side::Right), Input::NudgeSize(Side::Right)] {
        let mut s = chain_of_three();
        s.select([ShapeId("b".into())]);
        s.handle(Input::OpenColorPicker, T);
        s.handle(input.clone(), T);
        assert!(s.color_picker_open(), "{input:?}");
        assert_eq!(s.document(), chain_of_three().document(), "{input:?}");
    }
}

#[test]
fn colors_go_to_the_shape_the_menu_was_opened_on_even_once_it_moved() {
    let mut s = chain_of_three();
    s.move_shape(&ShapeId("c".into()), 600, 300);
    right_click(&mut s, pt(650.0, 320.0));
    // c moves back from under the menu's point while the menu is open.
    s.move_shape(&ShapeId("c".into()), 600, 0);
    s.handle(Input::MenuChoose(MenuAction::Recolor(Color::Rose)), T);
    assert!(s.document().shapes.iter().all(|x| x.color == Color::Rose));
}

#[test]
fn remove_link_goes_to_the_link_the_menu_was_opened_on() {
    let mut s = chain_of_three();
    s.move_shape(&ShapeId("c".into()), 600, 300);
    // The b-c connector runs from b's right middle (510, 24.25) to c's left middle (600, 324.25).
    right_click(&mut s, pt(555.0, 174.25));
    assert_eq!(s.menu().map(|m| m.kind), Some(MenuKind::Connector));
    s.move_shape(&ShapeId("c".into()), 600, 0);
    s.handle(Input::MenuChoose(MenuAction::RemoveLink), T);
    assert_eq!(s.document().links.len(), 1);
}

#[test]
fn delete_new_beside_undo_and_redo_close_an_open_menu() {
    for input in [
        Input::DeleteShapes,
        Input::NewBeside,
        Input::Undo,
        Input::Redo,
    ] {
        let mut s = session_of(doc_with("a"));
        right_click(&mut s, pt(100.0, 20.0));
        assert!(s.menu().is_some());
        let r = s.handle(input.clone(), T);
        assert!(s.menu().is_none(), "{input:?}");
        assert!(r.redraw, "{input:?}");
    }
}

#[test]
fn deleting_the_edited_shape_ends_editing() {
    let mut s = session_of(doc_with("a"));
    let a = s.document().shapes[0].id.clone();
    s.select([a.clone()]);
    s.handle(Input::StartEditing, T);
    s.delete(&[a]);
    assert!(s.editing().is_none());
    assert!(s.document().shapes.is_empty());
}

#[test]
fn menu_cut_never_leaves_editing_on_a_missing_shape() {
    let mut s = session_of(doc_with("a"));
    right_click(&mut s, pt(100.0, 20.0));
    s.handle(Input::NewBeside, T);
    s.handle(Input::MenuChoose(MenuAction::Cut), T);
    let e = s.editing().map(|e| e.id.clone());
    assert!(
        e.is_none_or(|id| s.document().shapes.iter().any(|x| x.id == id)),
        "editing a missing shape"
    );
}
