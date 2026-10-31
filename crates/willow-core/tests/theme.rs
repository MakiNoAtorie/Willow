//! Switching theme: only the look changes, and the document stays as it was.
mod common;

use std::ops::Range;
use std::time::Duration;

use common::text;
use willow_core::{
    Document, Input, Link, Measure, PAPER, Paragraph, Point, Session, Shape, ShapeId, THEMES,
    Theme, Viewport,
};

const T: Duration = Duration::ZERO;
/// Paper's line is 22.5 tall, Desk's 21.
const LONG: &str = "aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa aaaa";

fn desk() -> &'static Theme {
    THEMES.iter().copied().find(|t| t.name == "Desk").unwrap()
}

/// Paper fits `LONG` on one line and Desk on ten: 30 wide clusters leave five letters, in a
/// 180-wide column.
struct WideInDesk;

impl Measure for WideInDesk {
    fn advances(&self, _: &Paragraph, clusters: &[Range<usize>], t: &Theme) -> Vec<f32> {
        let width = if t.name == "Desk" { 30.0 } else { 1.0 };
        vec![width; clusters.len()]
    }
}

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// `a` holds `LONG`; `b` sits `y` below its top, `x` to the right, linked to it if `linked`.
fn session(x: i32, y: i32, linked: bool) -> Session {
    let mut a = Shape::new(id("a"), 0, 0);
    a.text = text(LONG);
    a.extra_height = 5;
    let doc = Document {
        shapes: vec![a, Shape::new(id("b"), x, y)],
        links: (linked.then(|| Link(id("a"), id("b"))).into_iter()).collect(),
        ..Document::default()
    };
    Session::new(doc, &PAPER, Box::new(WideInDesk))
}

fn height(s: &Session, name: &str) -> f32 {
    s.drawn_rect(&id(name)).expect(name).height
}

#[test]
fn a_switch_measures_every_shape_again_for_the_new_theme() {
    let mut s = session(0, 400, false);
    // One 22.5 line, 13 + 13 padding, 5 extra.
    assert_eq!(height(&s, "a"), 53.5);
    s.set_theme(desk());
    assert_eq!(s.theme().name, "Desk");
    // Ten 21 lines, 18 + 13 padding, 5 extra.
    assert_eq!(height(&s, "a"), 246.0);
    assert_eq!(s.heights()[0], 246.0);
    s.set_theme(&PAPER);
    assert_eq!(height(&s, "a"), 53.5);
}

#[test]
fn a_switch_clears_neighbors_at_once_and_keeps_the_document_and_its_history() {
    let mut s = session(0, 100, false);
    let doc = s.document().clone();
    s.set_theme(desk());
    // 246 tall + the 24 gap.
    assert_eq!(s.drawn_rect(&id("b")).unwrap().y, 270.0);
    assert_eq!(*s.document(), doc);
    assert!(s.is_saved());
    assert!(!s.can_undo());
    assert!(!s.animating(T), "a switch slides nothing");
    assert_eq!(s.recovery_wake(T), None);
    s.set_theme(&PAPER);
    assert_eq!(s.drawn_rect(&id("b")).unwrap().y, 100.0);
}

/// What the app does after a switch or an input.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        let done = job.run();
        s.regenerated(done);
    }
}

fn saved_text(s: &mut Session) -> String {
    let mut written = String::new();
    s.save(|b| {
        written = String::from_utf8_lossy(b).into();
        Ok(())
    })
    .unwrap();
    written
}

/// `b` is 297 below `a`'s center and 260 right of it in Paper; Desk's tall `a` brings its
/// center level enough for `b` to read as right of it.
fn switching_words() -> Session {
    let mut s = session(260, 300, true);
    s.handle(Input::TogglePanel, T);
    settle(&mut s);
    assert!(s.panel().text().contains("b: \"\" below a"));
    s
}

#[test]
fn the_panel_catches_up_with_the_new_word_order_and_stays_saved() {
    let mut s = switching_words();
    s.set_theme(desk());
    settle(&mut s);
    assert!(s.panel().text().contains("b: \"\" right of a"));
    assert!(s.is_saved());
    assert!(!s.can_undo());
}

#[test]
fn the_next_save_writes_the_new_word_order_even_with_the_panel_closed() {
    let mut s = switching_words();
    s.handle(Input::TogglePanel, T);
    s.set_theme(desk());
    assert!(saved_text(&mut s).contains("b: \"\" right of a"));
}

#[test]
fn a_switch_leaves_text_the_writer_typed_alone() {
    let mut s = switching_words();
    s.handle(Input::FocusPanel, T);
    let typed = "not a shape line";
    s.handle(
        Input::Text {
            text: typed.into(),
            replace: Some(0..s.panel().text().encode_utf16().count()),
        },
        T,
    );
    assert_eq!(s.panel().text(), typed);
    s.set_theme(desk());
    settle(&mut s);
    assert_eq!(s.panel().text(), typed);
    assert_eq!(saved_text(&mut s), typed);
}

#[test]
fn every_theme_keeps_the_gaps_and_never_draws_text_under_about_7_px() {
    for t in THEMES {
        assert_eq!((t.creation_gap, t.min_gap), (48.0, 24.0), "{}", t.name);
        let mut s = Session::new(Document::new(), t, Box::new(WideInDesk));
        let (origin, size) = (Point::default(), Point { x: 900.0, y: 900.0 });
        let mut text_at = |zoom: f32| {
            s.set_viewport(Viewport { origin, zoom, size });
            s.draw(origin, size, T).text_opacity
        };
        // Walk the zoom range: the smallest zoom that still draws text.
        let (mut zoom, mut lowest) = (t.zoom.max, t.zoom.max);
        while zoom > t.zoom.min {
            if text_at(zoom) > 0.0 {
                lowest = zoom;
            }
            zoom -= 0.001;
        }
        let px = t.font_size * lowest;
        assert!(px >= 6.9, "{}: text at {px} px", t.name);
    }
}
