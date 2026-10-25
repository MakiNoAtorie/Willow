//! The Tab directional picker: Tab over the selected shape, then an arrow creates a linked shape
//! there exactly as a midpoint click does.
mod common;

use std::time::Duration;

use willow_core::{Color, Document, Input, Link, Session, Shape, ShapeId, Side};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Rose shapes a (selected), b below it linked to a, and c far right; Tab inserts between a
/// and b on the bottom.
fn session() -> Session {
    let doc = Document {
        shapes: [("a", 0, 0), ("b", 0, 100), ("c", 3000, 0)]
            .map(|(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .to_vec(),
        links: vec![Link(id("a"), id("b"))],
        ..Document::default()
    };
    let mut s = common::session_of(doc);
    s.select([id("a")]);
    s
}

fn input(s: &mut Session, i: Input) {
    s.handle(i, T);
}

fn typed(s: &mut Session, text: &str) {
    let text = text.into();
    input(
        s,
        Input::Text {
            text,
            replace: None,
        },
    );
}

#[test]
fn tab_then_an_arrow_creates_as_a_midpoint_click_does() {
    for side in [Side::Top, Side::Right, Side::Bottom, Side::Left] {
        let mut want = session();
        want.create_linked(&id("a"), side);
        let mut s = session();
        input(&mut s, Input::Tab);
        assert!(s.tab_picker_open(), "{side:?}");
        input(&mut s, Input::PickSide(side));
        assert!(!s.tab_picker_open(), "{side:?}");
        assert_eq!(s.document(), want.document(), "{side:?}");
        assert_eq!(s.selection(), want.selection(), "{side:?}");
        let new = want.editing().map(|e| e.id.clone());
        assert_eq!(s.editing().map(|e| e.id.clone()), new, "{side:?}");
        // One undo step with the first editing session.
        typed(&mut s, "Hi");
        input(&mut s, Input::Escape);
        input(&mut s, Input::Undo);
        assert_eq!(s.document(), session().document(), "{side:?}");
        assert!(!s.can_undo(), "{side:?}");
    }
}

#[test]
fn tab_while_editing_ends_that_session_and_shows_the_picker() {
    let mut s = session();
    input(&mut s, Input::StartEditing);
    typed(&mut s, "Hi");
    input(&mut s, Input::Tab);
    assert!(s.editing().is_none());
    assert!(s.tab_picker_open());
    assert_eq!(s.selection(), [id("a")]);
    // The typing was its own undo step, closed before the picker's creation.
    input(&mut s, Input::PickSide(Side::Right));
    typed(&mut s, "New");
    input(&mut s, Input::Undo);
    assert_eq!(s.document().shapes.len(), 3);
    assert_eq!(s.document().shapes[0].text.plain(), "Hi");
}

#[test]
fn esc_and_tab_close_another_key_closes_and_is_consumed_a_click_closes_and_acts() {
    for close in [Input::Escape, Input::Tab] {
        let mut s = session();
        input(&mut s, Input::Tab);
        input(&mut s, close.clone());
        assert!(!s.tab_picker_open(), "{close:?}");
        assert_eq!(s.selection(), [id("a")], "{close:?}");
    }
    let keys = [
        Input::Navigate(Side::Bottom),
        Input::Nudge(Side::Right),
        Input::DeleteShapes,
        Input::Enter,
    ];
    for key in keys {
        let mut s = session();
        input(&mut s, Input::Tab);
        input(&mut s, key.clone());
        assert!(!s.tab_picker_open(), "{key:?}");
        assert_eq!(s.document(), session().document(), "{key:?}");
        assert_eq!(s.selection(), [id("a")], "{key:?}");
        assert!(s.editing().is_none(), "{key:?}");
    }
    let mut s = session();
    input(&mut s, Input::Tab);
    typed(&mut s, "x");
    assert!(!s.tab_picker_open());
    assert_eq!(s.document(), session().document());
    assert!(s.editing().is_none());
    // A click on b closes the picker and selects b.
    let mut s = session();
    input(&mut s, Input::Tab);
    let at = s
        .viewport()
        .to_screen(willow_core::Point { x: 100.0, y: 120.0 });
    let (button, mods) = (willow_core::Button::Left, Default::default());
    input(
        &mut s,
        Input::PointerDown {
            pos: at,
            button,
            mods,
        },
    );
    input(
        &mut s,
        Input::PointerUp {
            pos: at,
            button,
            mods,
        },
    );
    assert!(!s.tab_picker_open());
    assert_eq!(s.selection(), [id("b")]);
}

#[test]
fn tab_with_nothing_or_several_selected_does_nothing() {
    for sel in [vec![], vec![id("a"), id("b")]] {
        let mut s = session();
        s.select(sel.clone());
        input(&mut s, Input::Tab);
        assert!(!s.tab_picker_open(), "{sel:?}");
        input(&mut s, Input::PickSide(Side::Right));
        assert_eq!(s.document(), session().document(), "{sel:?}");
        assert_eq!(s.selection(), sel, "{sel:?}");
    }
}

#[test]
fn the_view_follows_a_shape_created_off_screen() {
    let mut s = session();
    s.select([id("c")]);
    input(&mut s, Input::Tab);
    let before = s.viewport().origin;
    input(&mut s, Input::PickSide(Side::Right));
    s.tick(Duration::from_secs(1));
    assert!(s.viewport().origin.x > before.x + 2000.0);
}

#[test]
fn the_frame_shows_the_picker_at_the_selected_shapes_edge_midpoints() {
    let mut s = session();
    let frame = |s: &Session| {
        let (from, to) = (
            willow_core::Point::default(),
            willow_core::Point { x: 1e4, y: 1e4 },
        );
        s.draw(from, to, T).tab_picker
    };
    assert!(frame(&s).is_empty());
    input(&mut s, Input::Tab);
    let h = s.shape_height(&s.document().shapes[0]);
    let p = |x: f32, y: f32| willow_core::Point { x, y };
    assert_eq!(
        frame(&s),
        [
            (Side::Top, p(105.0, 0.0)),
            (Side::Right, p(210.0, h / 2.0)),
            (Side::Bottom, p(105.0, h)),
            (Side::Left, p(0.0, h / 2.0)),
        ]
    );
}
