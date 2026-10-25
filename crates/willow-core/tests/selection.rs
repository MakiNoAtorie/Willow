//! Text selection and the clipboard's plain-text side, in the shape being edited.
mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Button, Input, Modifiers, Motion, Point, Session};

const T: Duration = Duration::ZERO;

/// Paper's line height.
const LH: f32 = 22.5;

/// Line 0 "aaaa bbbb cccc dddd " (0..20) is justified to 178: words at x 0, 46, 92, 138;
/// line 1 "eeee ffff" (20..29).
const TWO_LINES: &str = "aaaa bbbb cccc dddd eeee ffff";

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Screen point `x` across and `line` lines down the text, a little below the line's top.
/// The shape is at the canvas origin, so its text starts at canvas (16, 13).
fn at(s: &Session, x: f32, line: f32) -> Point {
    s.viewport().to_screen(pt(16.0 + x, 13.0 + line * LH + 5.0))
}

fn mouse(s: &mut Session, input: fn(Point, Modifiers) -> Input, pos: Point, shift: bool) {
    let mods = Modifiers {
        shift,
        ..Modifiers::default()
    };
    s.handle(input(pos, mods), T);
}

fn down(pos: Point, mods: Modifiers) -> Input {
    let button = Button::Left;
    Input::PointerDown { pos, button, mods }
}

fn up(pos: Point, mods: Modifiers) -> Input {
    let button = Button::Left;
    Input::PointerUp { pos, button, mods }
}

fn mv(pos: Point, mods: Modifiers) -> Input {
    Input::PointerMove { pos, mods }
}

fn click(s: &mut Session, pos: Point) {
    mouse(s, down, pos, false);
    mouse(s, up, pos, false);
}

fn drag(s: &mut Session, from: Point, to: Point) {
    mouse(s, down, from, false);
    mouse(s, mv, to, false);
    mouse(s, up, to, false);
}

/// Editing `text`, caret at offset 0.
fn editing(text: &str) -> Session {
    let mut s = session_of(doc_with(text));
    let p = at(&s, 0.0, 0.0);
    click(&mut s, p);
    click(&mut s, p);
    s
}

/// (anchor, caret) offsets.
fn sel(s: &Session) -> (usize, usize) {
    let e = s.editing().expect("editing");
    (e.anchor, e.caret.offset)
}

#[test]
fn dragging_in_the_edited_text_selects_across_a_soft_wrap() {
    let mut s = editing(TWO_LINES);

    let (from, to) = (at(&s, 66.0, 0.0), at(&s, 20.0, 1.0));
    drag(&mut s, from, to);
    assert_eq!(sel(&s), (7, 22));
    assert_eq!(s.selected_text().as_deref(), Some("bb cccc dddd ee"));

    let (from, to) = (at(&s, 20.0, 1.0), at(&s, 66.0, 0.0));
    drag(&mut s, from, to);
    assert_eq!(sel(&s), (22, 7), "backwards keeps the press as the anchor");
    assert_eq!(s.selected_text().as_deref(), Some("bb cccc dddd ee"));
}

/// Canvas rect from text-local corners: the text starts at canvas (16, 13).
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> (Point, Point) {
    (pt(16.0 + x0, 13.0 + y0), pt(16.0 + x1, 13.0 + y1))
}

#[test]
fn selection_rects_cover_each_line_and_stop_at_a_wrap_where_the_drag_did() {
    let mut s = editing(TWO_LINES);

    let (from, to) = (at(&s, 66.0, 0.0), at(&s, 20.0, 1.0));
    drag(&mut s, from, to);
    assert_eq!(
        s.selection_rects(),
        vec![rect(66.0, 0.0, 178.0, LH), rect(0.0, LH, 20.0, 2.0 * LH)]
    );

    // Past the end of the first line: the caret stays at that line's end, not the next start.
    let (from, to) = (at(&s, 66.0, 0.0), at(&s, 300.0, 0.0));
    drag(&mut s, from, to);
    let e = s.editing().unwrap();
    assert_eq!((e.anchor, e.caret.offset, e.caret.upstream), (7, 20, true));
    assert_eq!(s.selection_rects(), vec![rect(66.0, 0.0, 178.0, LH)]);
}

#[test]
fn a_selected_paragraph_break_reaches_the_right_edge_and_an_empty_line_is_covered() {
    // "ab" (0..2), "" (3..3), "cd" (4..6).
    let mut s = editing("ab\n\ncd");

    let (from, to) = (at(&s, 10.0, 0.0), at(&s, 10.0, 2.0));
    drag(&mut s, from, to);
    assert_eq!(s.selected_text().as_deref(), Some("b\n\nc"));
    assert_eq!(
        s.selection_rects(),
        vec![
            rect(10.0, 0.0, 178.0, LH),
            rect(0.0, LH, 178.0, 2.0 * LH),
            rect(0.0, 2.0 * LH, 10.0, 3.0 * LH),
        ]
    );
}

#[test]
fn nothing_selected_draws_no_rects() {
    let s = editing(TWO_LINES);
    assert_eq!(s.selection_rects(), vec![]);
    assert_eq!(s.selected_text(), None);
}

#[test]
fn shift_motions_move_the_caret_and_keep_the_anchor() {
    let mut s = editing(TWO_LINES);
    let p = at(&s, 66.0, 0.0);
    click(&mut s, p);
    assert_eq!(sel(&s), (7, 7));
    let mut go = |m| {
        s.handle(Input::Select(m), T);
        sel(&s)
    };

    assert_eq!(go(Motion::Right), (7, 8));
    assert_eq!(go(Motion::WordRight), (7, 9));
    assert_eq!(go(Motion::WordRight), (7, 14));
    assert_eq!(
        go(Motion::Left),
        (7, 13),
        "a selection doesn't collapse while extending"
    );
    assert_eq!(go(Motion::WordLeft), (7, 10));
    assert_eq!(go(Motion::WordLeft), (7, 5), "back across the anchor");
    assert_eq!(go(Motion::Down), (7, 25), "near column 46 on the next line");
    assert_eq!(go(Motion::LineEnd), (7, 29));
    assert_eq!(go(Motion::Up), (7, 9), "near column 84 on the first line");
    assert_eq!(go(Motion::LineEnd), (7, 20));
    assert!(
        s.editing().unwrap().caret.upstream,
        "shown at the wrapped line's end"
    );
    assert_eq!(s.selected_text().as_deref(), Some("bb cccc dddd "));

    s.handle(Input::Move(Motion::Left), T);
    assert_eq!(
        sel(&s),
        (7, 7),
        "a plain motion collapses to the selection's start"
    );
}

#[test]
fn select_all_selects_the_whole_edited_text() {
    let mut s = editing("ab\ncd");
    s.handle(Input::SelectAll, T);
    assert_eq!(sel(&s), (0, 5));
    assert_eq!(s.selected_text().as_deref(), Some("ab\ncd"));
}

fn plain(s: &Session) -> String {
    s.document().shapes[0].text.plain()
}

#[test]
fn paste_brings_paragraph_breaks_and_cut_removes_the_selection_in_one_undo_step() {
    let mut s = editing("ab");
    assert!(s.can_paste());
    assert!(!s.can_cut() && !s.can_copy(), "nothing selected");
    s.handle(Input::Move(Motion::Right), T);

    s.handle(Input::Paste("x\r\ny\rz\n\tw".into()), T);
    assert_eq!(plain(&s), "ax\ny\nz\nwb");
    assert_eq!(s.document().shapes[0].text.paragraphs.len(), 4);
    assert_eq!(sel(&s), (8, 8), "the caret lands after the pasted text");

    s.handle(Input::Select(Motion::LineStart), T);
    assert!(s.can_cut() && s.can_copy());
    assert_eq!(s.selected_text().as_deref(), Some("w"));
    s.handle(Input::Cut, T);
    assert_eq!(plain(&s), "ax\ny\nz\nb");
    assert_eq!(sel(&s), (7, 7));

    s.handle(Input::SelectAll, T);
    s.handle(Input::Paste("q".into()), T);
    assert_eq!(plain(&s), "q", "paste replaces the selection");

    s.handle(Input::Escape, T);
    assert!(s.can_paste(), "shapes paste onto the canvas");
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");
    assert!(!s.can_undo());
}

#[test]
fn shift_click_in_the_edited_text_extends_the_selection() {
    let mut s = editing(TWO_LINES);
    let (a, b) = (at(&s, 66.0, 0.0), at(&s, 20.0, 1.0));
    click(&mut s, a);
    mouse(&mut s, down, b, true);
    mouse(&mut s, up, b, true);
    assert_eq!(sel(&s), (7, 22));
}
