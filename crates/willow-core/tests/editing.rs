mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Button, Input, Key, Modifiers, Motion, Point, Session};

const T: Duration = Duration::ZERO;

fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The text's top-left on screen: the centered 210 x 48.5 shape at (395, 376), plus padding.
const TEXT: Point = Point { x: 411.0, y: 389.0 };
const EMPTY: Point = Point { x: 50.0, y: 50.0 };

fn session(text: &str) -> Session {
    session_of(doc_with(text))
}

fn press(s: &mut Session, button: Button, pos: Point) {
    let mods = Modifiers::default();
    s.handle(Input::PointerDown { pos, button, mods }, T);
}

fn release(s: &mut Session, button: Button, pos: Point) {
    let mods = Modifiers::default();
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

fn click(s: &mut Session, pos: Point) {
    press(s, Button::Left, pos);
    release(s, Button::Left, pos);
}

fn drag(s: &mut Session, button: Button, from: Point, to: Point) {
    press(s, button, from);
    let mods = Modifiers::default();
    s.handle(Input::PointerMove { pos: to, mods }, T);
    release(s, button, to);
}

fn typ(s: &mut Session, text: &str) {
    s.handle(
        Input::Text {
            text: text.into(),
            replace: None,
        },
        T,
    );
}

fn plain(s: &Session) -> String {
    s.document().shapes[0].text.plain()
}

fn caret(s: &Session) -> Option<usize> {
    s.editing().map(|e| e.caret.offset)
}

/// Selected and editing, by clicking the shape twice.
fn edit_at(s: &mut Session, pos: Point) {
    click(s, pos);
    click(s, pos);
}

#[test]
fn a_click_on_the_selected_shape_edits_with_the_caret_where_clicked() {
    let mut s = session("aaaa bbbb");
    let on = pt(TEXT.x + 31.0, TEXT.y + 5.0);

    click(&mut s, on);
    assert_eq!(caret(&s), None);

    click(&mut s, on);
    assert_eq!(caret(&s), Some(3));
    assert!(s.selection().is_some());
}

#[test]
fn f2_edits_the_selected_shape_with_the_caret_at_the_end() {
    let mut s = session("aaaa bbbb");
    s.handle(Input::StartEditing, T);
    assert_eq!(caret(&s), None, "nothing selected");

    click(&mut s, TEXT);
    s.handle(Input::StartEditing, T);
    assert_eq!(caret(&s), Some(9));
}

#[test]
fn typing_on_a_selected_shape_edits_it_and_appends_but_space_types_nothing() {
    let mut s = session("ab");
    typ(&mut s, "x");
    assert_eq!(
        (plain(&s), caret(&s)),
        ("ab".into(), None),
        "nothing selected"
    );

    click(&mut s, TEXT);
    s.handle(Input::KeyDown(Key::Space), T);
    typ(&mut s, " ");
    s.handle(Input::KeyUp(Key::Space), T);
    assert_eq!((plain(&s), caret(&s)), ("ab".into(), None));

    // A replacement range from before editing started is ignored: it still appends.
    let text = "c".into();
    s.handle(
        Input::Text {
            text,
            replace: Some(0..0),
        },
        T,
    );
    typ(&mut s, " d");
    assert_eq!((plain(&s), caret(&s)), ("abc d".into(), Some(5)));
}

#[test]
fn enter_and_esc_leave_editing_and_keep_the_shape_selected() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    let sel = s.selection().cloned();

    s.handle(Input::StopEditing, T);
    assert_eq!(caret(&s), None);
    assert_eq!(s.selection().cloned(), sel);
}

#[test]
fn a_click_on_empty_canvas_while_editing_only_leaves_editing() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    let vp = s.viewport();

    drag(&mut s, Button::Left, EMPTY, pt(90.0, 90.0));
    assert_eq!(caret(&s), None);
    assert!(s.selection().is_some());
    assert_eq!(s.viewport(), vp);
}

#[test]
fn panning_is_ignored_while_editing() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    let vp = s.viewport();

    let mods = Modifiers::default();
    let delta = pt(10.0, 10.0);
    s.handle(
        Input::Wheel {
            pos: EMPTY,
            delta,
            mods,
        },
        T,
    );
    drag(&mut s, Button::Middle, EMPTY, pt(90.0, 90.0));
    s.handle(Input::KeyDown(Key::Space), T);
    drag(&mut s, Button::Left, TEXT, pt(90.0, 90.0));

    assert_eq!(s.viewport(), vp);
    assert!(s.editing().is_some());
}

#[test]
fn dragging_inside_the_edited_shape_never_moves_it() {
    let mut s = session("aaaa bbbb");
    edit_at(&mut s, TEXT);

    drag(&mut s, Button::Left, TEXT, pt(TEXT.x + 40.0, TEXT.y + 30.0));

    let shape = &s.document().shapes[0];
    assert_eq!((shape.x, shape.y), (0, 0));
    assert!(s.editing().is_some());
}

#[test]
fn the_shape_grows_downward_as_text_wraps_and_keeps_its_width() {
    let mut s = session("");
    click(&mut s, TEXT);
    let before = s.shape_height(&s.document().shapes[0]);

    typ(&mut s, "aaaa bbbb cccc dddd eeee");

    let shape = &s.document().shapes[0];
    assert_eq!(s.shape_height(shape), before + 22.5);
    assert_eq!((shape.width, shape.y), (210, 0));
}

#[test]
fn caret_moves_by_cluster_word_and_line() {
    // Line 0 "aaaa bbbb cccc dddd " (0..20), line 1 "eeee ffff" (20..29).
    let mut s = session("aaaa bbbb cccc dddd eeee ffff");
    edit_at(&mut s, pt(TEXT.x + 66.0, TEXT.y + 5.0));
    assert_eq!(caret(&s), Some(7));
    let mut go = |m| {
        s.handle(Input::Move(m), T);
        caret(&s).unwrap()
    };

    assert_eq!(go(Motion::Right), 8);
    assert_eq!(go(Motion::Left), 7);
    assert_eq!(go(Motion::WordRight), 9);
    assert_eq!(go(Motion::WordRight), 14);
    assert_eq!(go(Motion::WordLeft), 10);
    assert_eq!(go(Motion::LineEnd), 20);
    assert_eq!(go(Motion::LineStart), 0);
    assert_eq!(go(Motion::Down), 20);
    assert_eq!(go(Motion::LineEnd), 29);
}

#[test]
fn up_and_down_keep_the_column_across_a_short_line() {
    // Three lines: "aaaa bbbb cccc dddd " (justified), "x", "eeee ffff".
    let mut s = session("aaaa bbbb cccc dddd x\neeee ffff");
    // Offset 7 is at x 66 on the justified first line (text starts at canvas (16, 13)).
    let on = s.viewport().to_screen(pt(16.0 + 66.0, 13.0 + 5.0));
    edit_at(&mut s, on);
    let mut go = |m| {
        s.handle(Input::Move(m), T);
        caret(&s).unwrap()
    };

    assert_eq!(go(Motion::Down), 21, "end of the short line");
    assert_eq!(go(Motion::Down), 29, "back near column 66, at x 64");
    assert_eq!(go(Motion::Up), 21);
    assert_eq!(go(Motion::Up), 7);
}

#[test]
fn backspace_and_delete_remove_one_cluster_and_word_deletion_a_word() {
    let mut s = session("aé bb cc");
    click(&mut s, TEXT);
    s.handle(Input::StartEditing, T);

    s.handle(Input::Delete(Motion::WordLeft), T);
    assert_eq!(plain(&s), "aé bb ");
    s.handle(Input::Move(Motion::LineStart), T);
    s.handle(Input::Move(Motion::Right), T);
    s.handle(Input::Delete(Motion::Right), T);
    assert_eq!(plain(&s), "a bb ");
    s.handle(Input::Delete(Motion::Left), T);
    assert_eq!((plain(&s), caret(&s)), (" bb ".into(), Some(0)));
    s.handle(Input::Delete(Motion::Left), T);
    assert_eq!(plain(&s), " bb ");
}

#[test]
fn one_editing_session_is_one_undo_step() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    typ(&mut s, "x");
    typ(&mut s, "y");
    s.handle(Input::Delete(Motion::Left), T);
    s.handle(Input::StopEditing, T);
    assert_eq!(plain(&s), "xab");

    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");
    assert!(!s.can_undo());
    s.handle(Input::Redo, T);
    assert_eq!(plain(&s), "xab");
}

#[test]
fn a_session_that_changes_nothing_adds_no_undo_step() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    s.handle(Input::StopEditing, T);
    assert!(!s.can_undo());

    s.handle(Input::StartEditing, T);
    typ(&mut s, "x");
    s.handle(Input::Delete(Motion::Left), T);
    s.handle(Input::StopEditing, T);
    assert_eq!(plain(&s), "ab");
    assert!(!s.can_undo());
}

#[test]
fn undo_while_editing_reverts_the_session_so_far_and_leaves_editing() {
    let mut s = session("ab");
    click(&mut s, TEXT);
    typ(&mut s, "xy");

    s.handle(Input::Undo, T);
    assert_eq!((plain(&s), caret(&s)), ("ab".into(), None));
    assert!(s.selection().is_some());

    s.handle(Input::Redo, T);
    assert_eq!(plain(&s), "abxy");
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");
    assert!(!s.can_undo());
}

#[test]
fn the_caret_blinks_then_goes_solid_and_stops_animating() {
    let mut s = session("ab");
    click(&mut s, TEXT);
    s.handle(
        Input::Text {
            text: "x".into(),
            replace: None,
        },
        ms(1000),
    );
    let blink = s.theme().caret.blink.as_millis() as u64;
    let idle = s.theme().caret.idle.as_millis() as u64;

    let c = s.caret(ms(1000)).unwrap();
    assert!(c.visible);
    // After "abx", 30 units in.
    assert_eq!((c.pos, c.height), (pt(46.0, 13.0), 22.5));
    assert!(!s.caret(ms(1000 + blink)).unwrap().visible);

    assert!(s.caret(ms(1000 + idle)).unwrap().visible);
    assert!(!s.animating(ms(1000 + idle)));
    assert!(!s.tick(ms(1000 + idle)).animating);
}

#[test]
fn a_blinking_caret_asks_for_a_wake_at_each_blink_edge_not_for_frames() {
    let mut s = session("ab");
    click(&mut s, TEXT);
    s.handle(Input::StartEditing, ms(1000));
    // PAPER: blink 530 ms, solid after 5 s.
    assert!(!s.animating(ms(1100)));
    assert_eq!(s.next_wake(ms(1000)), Some(ms(530)));
    assert_eq!(s.next_wake(ms(1100)), Some(ms(430)));
    assert_eq!(s.next_wake(ms(1530)), Some(ms(530)));
    // The last edge before solid is at 4770 ms; the caret turns solid at 5000 ms.
    assert_eq!(s.next_wake(ms(1000 + 4800)), Some(ms(200)));
    assert_eq!(s.next_wake(ms(1000 + 5000)), None);

    s.handle(Input::StopEditing, ms(1200));
    assert_eq!(s.next_wake(ms(1200)), None);
}

#[test]
fn redo_while_editing_leaves_editing_like_undo() {
    let mut s = session("ab cd");
    click(&mut s, TEXT);
    s.handle(Input::StartEditing, T);
    s.handle(Input::Delete(Motion::WordLeft), T);
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab cd");

    s.handle(Input::StartEditing, T);
    s.handle(Input::SelectAll, T);
    s.handle(Input::Redo, T);
    assert_eq!((plain(&s), caret(&s)), ("ab ".into(), None));
    assert_eq!(s.selected_text(), None);
    assert!(s.selection().is_some());
}

#[test]
fn undo_in_a_session_that_changed_nothing_only_leaves_editing() {
    let mut s = session("ab");
    edit_at(&mut s, TEXT);
    typ(&mut s, "x");
    s.handle(Input::StopEditing, T);

    s.handle(Input::StartEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!((plain(&s), caret(&s)), ("xab".into(), None));
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");
}
