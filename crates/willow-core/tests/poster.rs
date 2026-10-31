//! What only Poster draws, read from the draw query: borders that stay at low zoom, the hard
//! shadow's selection, title pills, overshoot and the stepped color change.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, Color, Document, Input, MenuAction, Modifiers, PAPER, POSTER, Point, Rgba, Session,
    Shape, ShapeId, Theme, Zone,
};

const T: Duration = Duration::ZERO;
const SCREEN: (Point, Point) = (
    Point { x: 0.0, y: 0.0 },
    Point {
        x: 1000.0,
        y: 800.0,
    },
);

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn zone(name: &str, title: &str, x: i32, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: title.into(),
        color: Color::Rose,
        x,
        y: 0,
        width: 800,
        height: 600,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

/// Rose shape "a" at (100, 100) inside rose zone "z" titled "Act I", in `theme`.
fn session(theme: &'static Theme) -> Session {
    let mut a = Shape::new(id("a"), 100, 100);
    a.color = Color::Rose;
    let mut s = session_of(Document {
        shapes: vec![a],
        links: vec![],
        zones: vec![zone("z", "Act I", 0, &["a"])],
    });
    s.set_theme(theme);
    s
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn at_zoom(s: &mut Session, zoom: f32) {
    let mut v = s.viewport();
    v.zoom = zoom;
    s.set_viewport(v);
}

#[test]
fn borders_stay_at_low_zoom_never_under_a_screen_pixel() {
    let mut s = session(&POSTER);
    let borders = |s: &Session| {
        let f = s.draw(SCREEN.0, SCREEN.1, T);
        (f.shape_border, f.zone_edge, f.shadows)
    };
    assert_eq!(borders(&s), (2.5, 2.5, true));
    at_zoom(&mut s, 0.1);
    // One screen pixel is 10 canvas units at 10%; the hard shadows are gone.
    assert_eq!(borders(&s), (10.0, 10.0, false));

    // Paper keeps no shape border and its zone edge scales away with the zoom.
    let mut p = session(&PAPER);
    at_zoom(&mut p, 0.1);
    let f = p.draw(SCREEN.0, SCREEN.1, T);
    assert_eq!((f.shape_border, f.zone_edge), (0.0, 1.0));
}

#[test]
fn zones_mix_their_pastel_into_the_canvas_inside_an_ink_edge() {
    let mut a = Shape::new(id("a"), 100, 100);
    a.color = Color::Rose;
    let mut s = session_of(Document {
        shapes: vec![a],
        links: vec![],
        zones: vec![
            zone("z", "Act I", 0, &["n"]),
            zone("n", "Ferry", 50, &["a"]),
        ],
    });
    s.set_theme(&POSTER);
    let f = s.draw(SCREEN.0, SCREEN.1, T);
    // 50% of the way from the canvas #fbf6ea to the pastel #ffb8b0, nested 85%.
    assert_eq!(f.zones[0].fill, Rgba(0xfdd7cdff));
    assert_eq!(f.zones[1].fill, Rgba(0xfec1b9ff));
    assert_eq!(f.zones[0].edge, Rgba(0x111111ff));
    assert_eq!(f.shapes[0].fill, Rgba(0xffb8b0ff));
}

#[test]
fn a_title_sits_in_a_pill_constant_on_screen_when_zoomed_out() {
    let mut s = session(&POSTER);
    let v = |s: &Session| {
        let f = s.draw(SCREEN.0, SCREEN.1, T);
        (f.zones[0].pill.expect("pill"), f.zones[0].text.clone())
    };
    let (pill, text) = v(&s);
    assert_eq!(text, "ACT I");
    // The Fake's bold "ACT I" is 48 at 14.5, so 39.72 at 12, plus .06 em (.72) per letter.
    let width = 48.0 * 12.0 / 14.5 + 5.0 * 0.72;
    // 16 and 12 in from the 2.5 edge; 9 either side of the text; 21.4 tall, the mockups' 21.
    assert_eq!((pill.0.x, pill.0.y), (18.5, 14.5));
    assert!(
        (pill.1.x - (18.5 + 9.0 + width + 9.0)).abs() < 1e-3,
        "{pill:?}"
    );
    assert!(
        (pill.1.y - (14.5 + 2.0 + 17.4 + 2.0)).abs() < 1e-3,
        "{pill:?}"
    );

    at_zoom(&mut s, 0.5);
    let (pill, _) = v(&s);
    assert_eq!((pill.0.x, pill.0.y), (37.0, 29.0));
    assert!((pill.1.y - pill.0.y - 2.0 * 21.4).abs() < 1e-3, "{pill:?}");

    assert!(
        session(&PAPER).draw(SCREEN.0, SCREEN.1, T).zones[0]
            .pill
            .is_none()
    );
}

#[test]
fn an_untitled_zone_shows_a_pill_only_while_its_title_is_edited() {
    let mut a = Shape::new(id("a"), 100, 100);
    a.color = Color::Rose;
    let mut s = session_of(Document {
        shapes: vec![a],
        links: vec![],
        zones: vec![zone("z", "", 0, &["a"])],
    });
    s.set_theme(&POSTER);
    let pill = |s: &Session| s.draw(SCREEN.0, SCREEN.1, T).zones[0].pill;
    assert!(pill(&s).is_none());

    let pos = s.viewport().to_screen(Point { x: 20.0, y: 10.0 });
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(Input::MenuChoose(MenuAction::Rename), T);
    assert!(pill(&s).is_some());
}

#[test]
fn a_selected_shape_overshoots_up_left_and_hit_testing_stays() {
    let mut s = session(&POSTER);
    let placed = s.drawn_rect(&id("a")).unwrap();
    s.select([id("a")]);
    let shape = |s: &Session, n| {
        let f = s.draw(SCREEN.0, SCREEN.1, ms(n));
        (f.shapes[0].lift, f.shapes[0].ring)
    };
    assert_eq!(shape(&s, 0).0, Point { x: 0.0, y: 0.0 });
    // cubic-bezier(.3,1.6,.5,1) peaks past 1 before it settles.
    let (lift, ring) = shape(&s, 50);
    assert!(ring > 1.05, "{ring}");
    assert!(lift.x < -3.1 && lift.y < -3.1, "{lift:?}");
    assert!(s.animating(ms(99)));
    assert_eq!(shape(&s, 100), (Point { x: -3.0, y: -3.0 }, 1.0));
    assert!(!s.animating(ms(100)));
    assert_eq!(s.drawn_rect(&id("a")).unwrap(), placed);
}

#[test]
fn a_new_shape_overshoots_its_size_but_not_its_opacity() {
    let mut s = session(&POSTER);
    let r = s.drawn_rect(&id("a")).unwrap();
    let bottom = Point {
        x: r.x + r.width / 2.0,
        y: r.y + r.height,
    };
    let pos = s.viewport().to_screen(bottom);
    let (button, mods) = (Button::Left, Modifiers::default());
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    let appear = |n| s.draw(SCREEN.0, SCREEN.1, ms(n)).shapes[1].appear;

    let peak = appear(100).expect("appearing");
    // cubic-bezier(.3,1.4,.5,1) has passed 1 by then, rising 8 units from 96%.
    assert!(peak.scale > 1.0, "{peak:?}");
    assert!(peak.dy > 0.0, "{peak:?}");
    assert_eq!(peak.opacity, 1.0);
    assert!(appear(160).is_none());
}

#[test]
fn the_color_change_snaps_in_two_steps() {
    let mut s = session(&POSTER);
    let pos = s.viewport().to_screen(Point { x: 120.0, y: 110.0 });
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(Input::MenuChoose(MenuAction::Recolor(Color::Sage)), T);
    let fill = |n| s.draw(SCREEN.0, SCREEN.1, ms(n)).shapes[0].fill;
    // Rose #ffb8b0 until 80 ms, halfway to sage #b6eda9 until 160 ms.
    assert_eq!(fill(79), Rgba(0xffb8b0ff));
    assert_eq!(fill(81), Rgba(0xdbd3adff));
    assert_eq!(fill(159), Rgba(0xdbd3adff));
    assert_eq!(fill(160), Rgba(0xb6eda9ff));
    assert!(!s.animating(ms(160)));
}

#[test]
fn the_zoom_pill_fades_over_300_ms() {
    let mut s = session(&POSTER);
    let pos = Point { x: 500.0, y: 400.0 };
    s.handle(Input::Pinch { pos, delta: 0.5 }, T);
    let opacity = |n| s.zoom_pill(ms(n)).map(|p| p.opacity);
    assert_eq!(opacity(700), Some(1.0));
    assert!(opacity(999).is_some());
    assert_eq!(opacity(1000), None);
}
