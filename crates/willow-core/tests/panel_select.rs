//! The code panel and the canvas follow each other's selection.

mod common;

use std::time::Duration;
use willow_core::{Button, Input, Modifiers, Motion, PAPER, Point, Session, ShapeId, load};

const T: Duration = Duration::ZERO;

/// Lines 3 and 10 (counting from 0): `a`; 5 and 11: `c`; 2: a zone; 6: a duplicate ID;
/// 7: unknown; 8: a link to a missing ID.
const TEXT: &[u8] = b"willow 1\n\nzone z \"Z\" {\n  a: \"A\"\n}\nc: \"C\"\na: \"dup\"\nnonsense \"here\"\nd: \"D\" below zz\n\n@layout a 0,0 w210\n@layout c 0,140 w210\n";

fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    let mut s = Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake));
    s.resize(1000.0, 800.0);
    s
}

fn ids(s: &Session) -> Vec<&str> {
    s.selection().iter().map(|i| i.0.as_str()).collect()
}

fn at_line(line: usize) -> Point {
    Point {
        x: 1100.0,
        y: 60.0 + line as f32 * 20.625 + 5.0,
    }
}

/// Clicks in the panel on (0-based) `line`, which is short enough not to wrap.
fn click(s: &mut Session, line: usize) {
    let (pos, button, mods) = (at_line(line), Button::Left, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

#[test]
fn a_caret_on_a_shape_or_its_layout_line_selects_it_and_other_lines_leave_it() {
    let mut s = open(TEXT);
    for (line, want) in [
        (3, "a"),
        (2, "a"),
        (5, "c"),
        (6, "c"),
        (7, "c"),
        (8, "c"),
        (11, "c"),
        (10, "a"),
        (1, "a"),
    ] {
        click(&mut s, line);
        assert_eq!(ids(&s), [want], "line {line}");
    }
    // Keys move the caret the same way.
    click(&mut s, 10);
    s.handle(Input::Move(Motion::Down), T);
    assert_eq!(ids(&s), ["c"]);
    s.handle(Input::Select(Motion::Up), T);
    assert_eq!(ids(&s), ["a"]);
    // Nothing of this is an undo step or an edit.
    assert!(!s.can_undo() && !s.can_redo());
    assert_eq!(s.document().shapes.len(), 3);
}

#[test]
fn a_triple_click_selects_the_line_it_is_on_not_the_next() {
    let mut s = open(TEXT);
    click(&mut s, 11);
    assert_eq!(ids(&s), ["c"]);
    s.handle(Input::TripleClick { pos: at_line(3) }, T);
    assert_eq!(ids(&s), ["a"]);
}

/// The ID of the `i`th shape of `long`.
fn id(i: u8) -> String {
    format!("x{}{}", (b'a' + i / 26) as char, (b'a' + i % 26) as char)
}

/// 60 shapes `id(0)`..`id(59)`, one below the other, in a panel that is open and unfocused.
fn long() -> Session {
    let mut text = String::from("willow 1\n\n");
    for i in 0..60u8 {
        text += &format!("{}: \"x\"\n", id(i));
    }
    for i in 0..60u8 {
        text += &format!("@layout {} 0,{} w210\n", id(i), i as i32 * 140);
    }
    let mut s = open(text.as_bytes());
    s.handle(Input::TogglePanel, T);
    settle(&mut s);
    s.handle(Input::Escape, T);
    assert!(s.panel().open && !s.panel().focused);
    s
}

fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        let done = job.run();
        s.regenerated(done);
    }
}

/// The line number of the first line starting with `start`, counted from 1.
fn number_of(s: &Session, start: &str) -> usize {
    s.panel()
        .text()
        .lines()
        .position(|l| l.starts_with(start))
        .unwrap()
        + 1
}

fn shown(s: &Session, number: usize) -> bool {
    (s.panel_rows().iter()).any(|r| r.number == Some(number) && r.y >= 0.0 && r.y + 20.0 <= 800.0)
}

fn pick(s: &mut Session, ids: &[&str]) {
    s.select(ids.iter().map(|i| ShapeId((*i).into())));
}

#[test]
fn a_canvas_selection_scrolls_the_unfocused_panel_to_the_first_shape_in_story_order() {
    let mut s = long();
    let (s40, s55) = (
        number_of(&s, &format!("{}:", id(40))),
        number_of(&s, &format!("{}:", id(55))),
    );
    assert!(!shown(&s, s40) && !shown(&s, s55));
    pick(&mut s, &[&id(55), &id(40)]);
    assert!(shown(&s, s40) && !shown(&s, s55));
    // Selecting what already shows leaves the view alone.
    let before = s.panel_rows()[0].y;
    pick(&mut s, &[&id(40)]);
    assert_eq!(s.panel_rows()[0].y, before);
    // Back up.
    pick(&mut s, &[&id(2)]);
    assert!(shown(&s, number_of(&s, &format!("{}:", id(2)))));
    assert!(!s.can_undo());
}

#[test]
fn nothing_scrolls_the_panel_while_it_has_focus() {
    let mut s = long();
    s.handle(Input::FocusPanel, T);
    let before = s.panel_rows()[0].y;
    pick(&mut s, &[&id(50)]);
    assert_eq!(s.panel_rows()[0].y, before);
    assert!(!shown(&s, number_of(&s, &format!("{}:", id(50)))));
    assert_eq!(s.panel().caret(), 0);
}

#[test]
fn a_shape_the_text_gains_scrolls_into_view_once_the_text_catches_up() {
    let mut s = long();
    pick(&mut s, &[&id(59)]);
    s.handle(Input::Duplicate, T);
    // The copy is selected, but the text doesn't have it yet.
    let copy = s.selection()[0].0.clone();
    let has = |s: &Session| (s.panel().text().lines()).any(|l| l.starts_with(&format!("{copy}:")));
    assert!(!has(&s));
    settle(&mut s);
    assert!(shown(&s, number_of(&s, &format!("{copy}:"))));
}

#[test]
fn a_newly_typed_shape_line_selects_its_shape_once_checked() {
    let mut s = open(b"willow 1\n\na: \"A\"\n\n@layout a 0,0 w210\n");
    s.handle(Input::TogglePanel, T);
    settle(&mut s);
    let at = s.panel().text().find("\"A\"").unwrap() + 3;
    let (text, replace) = (String::new(), Some(at..at));
    s.handle(Input::Text { text, replace }, T);
    s.handle(Input::Enter, T);
    let (text, replace) = ("b: \"B\" below a".to_owned(), None);
    s.handle(Input::Text { text, replace }, T);
    assert!(s.selection().is_empty());
    let job = s.take_text_check(Duration::from_millis(150)).unwrap();
    s.text_checked(job.run());
    assert_eq!(ids(&s), ["b"]);
}
