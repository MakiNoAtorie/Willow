//! What only Desk draws, read from the draw query: the card tab, the lifted selection, zone
//! fills from the picker color, uppercase letter-spaced titles.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, MenuAction, Modifiers, PAPER, Point, Rgba, Session, Shape,
    ShapeId, THEMES, Theme, Zone,
};

const T: Duration = Duration::ZERO;
const LATER: Duration = Duration::from_secs(5);
const SCREEN: (Point, Point) = (
    Point { x: 0.0, y: 0.0 },
    Point {
        x: 1000.0,
        y: 800.0,
    },
);

fn desk() -> &'static Theme {
    THEMES.iter().copied().find(|t| t.name == "Desk").unwrap()
}

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Rose shape "a" at (100, 100) inside rose zone "z" titled "Act I", in `theme`.
fn session(theme: &'static Theme) -> Session {
    let mut a = Shape::new(id("a"), 100, 100);
    a.color = Color::Rose;
    let mut s = session_of(Document {
        shapes: vec![a],
        links: vec![],
        zones: vec![Zone {
            id: id("z"),
            title: "Act I".into(),
            color: Color::Rose,
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            members: vec![id("a")],
        }],
    });
    s.set_theme(theme);
    s
}

#[test]
fn a_card_has_a_tab_in_its_picker_color_and_paper_has_none() {
    let s = session(desk());
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    assert_eq!(f.shapes[0].tab, Rgba(0xd8574cff));
    assert_eq!((desk().shape.tab, PAPER.shape.tab), (6.0, 0.0));
}

#[test]
fn a_selected_card_lifts_4px_as_its_ring_fades_in_and_hit_testing_stays() {
    let mut s = session(desk());
    let placed = s.drawn_rect(&id("a")).unwrap();
    s.select([id("a")]);
    let lift = |s: &Session, now| s.draw(SCREEN.0, SCREEN.1, now).shapes[0].lift;
    assert_eq!(lift(&s, T), Point { x: 0.0, y: 0.0 });
    let mid = lift(&s, Duration::from_millis(80));
    assert!(mid.y < -0.5 && mid.y > -4.0, "{mid:?}");
    assert_eq!(lift(&s, LATER), Point { x: 0.0, y: -4.0 });
    assert_eq!(s.drawn_rect(&id("a")).unwrap(), placed);

    let mut p = session(&PAPER);
    p.select([id("a")]);
    assert_eq!(lift(&p, LATER), Point { x: 0.0, y: 0.0 });
}

#[test]
fn a_zone_fills_from_its_picker_color_in_desk_and_its_pastel_in_paper() {
    let fill = |s: &Session| s.draw(SCREEN.0, SCREEN.1, T).zones[0].fill;
    // 16% of the way from the canvas #e6dfd1 to the picker #d8574c.
    assert_eq!(fill(&session(desk())), Rgba(0xe4c9bcff));
    // 45% of the way from #f6f3ec to the pastel #f3d8d3.
    assert_eq!(fill(&session(&PAPER)), Rgba(0xf5e7e1ff));
}

#[test]
fn desk_titles_are_uppercase_and_letter_spaced_in_the_text_and_the_caret() {
    let mut s = session(desk());
    let text = |s: &Session| s.draw(SCREEN.0, SCREEN.1, T).zones[0].text.clone();
    assert_eq!(text(&s), "ACT I");
    assert_eq!(text(&session(&PAPER)), "Act I");

    let pos = s.viewport().to_screen(Point { x: 20.0, y: 10.0 });
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(Input::MenuChoose(MenuAction::Rename), T);
    let caret = s.draw(SCREEN.0, SCREEN.1, T).zones[0].caret.expect("caret");
    // The Fake measures bold letters 11 and a space 4: "ACT I" is 48 at 14, so 41.14 at 12,
    // plus .08 em (0.96) for each of 5 letters; the title starts 16 in.
    assert!((caret.pos.x - (16.0 + 48.0 * 12.0 / 14.0 + 5.0 * 0.96)).abs() < 1e-3);
}
