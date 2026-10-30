//! Typing in the code panel: caret, selection, IME and clipboard over its plain text.

mod common;

use std::time::Duration;
use willow_core::{
    Button, Document, Input, Modifiers, Motion, PAPER, Point, Rgba, Session, ShapeId,
};

const T: Duration = Duration::ZERO;

/// An open, focused panel showing `text`, caret at the start.
fn panel(text: &str) -> Session {
    let mut s = common::session_of(Document::new());
    s.handle(Input::TogglePanel, T);
    s.set_panel_text(text.into());
    s
}

fn typed(s: &mut Session, text: &str) {
    let (text, replace) = (text.to_string(), None);
    s.handle(Input::Text { text, replace }, T);
}

#[test]
fn typing_inserts_at_the_caret_and_moves_it_on() {
    let mut s = panel("willow 1\n");
    typed(&mut s, "ab");
    assert_eq!(s.panel().text(), "abwillow 1\n");
    assert_eq!(s.panel().caret(), 2);
    assert_eq!(s.panel().selection(), 2..2);
    // The document never sees it.
    assert_eq!(*s.document(), Document::new());
}

#[test]
fn arrows_move_by_cluster_and_word_and_shift_selects() {
    let mut s = panel("a: \"日本\" below b\n");
    let mut key = |i: Input| {
        s.handle(i, T);
        (s.panel().caret(), s.panel().selection())
    };
    assert_eq!(key(Input::Move(Motion::WordRight)), (1, 1..1));
    assert_eq!(key(Input::Move(Motion::Right)), (2, 2..2));
    assert_eq!(key(Input::Select(Motion::WordRight)), (7, 2..7));
    // Left collapses a selection to its start.
    assert_eq!(key(Input::Move(Motion::Left)), (2, 2..2));
    assert_eq!(key(Input::Select(Motion::Right)), (3, 2..3));
    assert_eq!(key(Input::Select(Motion::Right)), (4, 2..4));
    assert_eq!(key(Input::Select(Motion::Right)), (7, 2..7));
    assert_eq!(key(Input::Move(Motion::Right)), (7, 7..7));
    assert_eq!(key(Input::Move(Motion::WordLeft)), (4, 4..4));
    assert_eq!(key(Input::SelectAll), (20, 0..20));
}

/// The panel is 48 columns wide here: line 1 wraps after its space, then mid-word, into
/// rows 0..11, 11..59 and 59..61; line 2 (`short`) starts at 62.
fn wrapped() -> Session {
    panel(&format!("{} {}\nshort", "a".repeat(10), "b".repeat(50)))
}

#[test]
fn up_and_down_follow_soft_wrapped_rows_keeping_the_column() {
    let mut s = wrapped();
    let mut key = |i: Input| {
        s.handle(i, T);
        s.panel().caret()
    };
    for _ in 0..5 {
        key(Input::Move(Motion::Right));
    }
    let downs: Vec<usize> = (0..4).map(|_| key(Input::Move(Motion::Down))).collect();
    assert_eq!(downs, [16, 61, 67, 67]);
    let ups: Vec<usize> = (0..4).map(|_| key(Input::Move(Motion::Up))).collect();
    assert_eq!(ups, [61, 16, 5, 0]);
    // Shift+Down selects down to the same column.
    key(Input::Move(Motion::Right));
    assert_eq!(key(Input::Select(Motion::Down)), 12);
    assert_eq!(s.panel().selection(), 1..12);
    // Typing sets a new column to keep.
    let mut s = wrapped();
    s.handle(Input::Move(Motion::Down), T);
    typed(&mut s, "x");
    s.handle(Input::Move(Motion::Up), T);
    assert_eq!(s.panel().caret(), 1);
}

#[test]
fn end_on_a_wrapped_row_stays_on_that_row() {
    let mut s = wrapped();
    let mut key = |i: Input| {
        s.handle(i, T);
        s.panel().caret()
    };
    key(Input::Move(Motion::Down));
    assert_eq!(key(Input::Move(Motion::LineEnd)), 59);
    // Still row 2: down reaches row 3's end, up comes back to row 2's end.
    assert_eq!(key(Input::Move(Motion::Down)), 61);
    assert_eq!(key(Input::Move(Motion::Up)), 59);
    assert_eq!(key(Input::Move(Motion::LineStart)), 11);
    assert_eq!(key(Input::Move(Motion::LineEnd)), 59);
    // Typing there goes on at the end of the row.
    typed(&mut s, "c");
    assert_eq!(s.panel().caret(), 60);
}

#[test]
fn tab_inserts_two_spaces_and_enter_keeps_the_indent() {
    let mut s = panel("zone z {\n  a: \"x\"\n}");
    s.handle(Input::Move(Motion::Down), T);
    s.handle(Input::Move(Motion::LineEnd), T);
    s.handle(Input::Enter, T);
    s.handle(Input::Tab, T);
    typed(&mut s, "b");
    assert_eq!(s.panel().text(), "zone z {\n  a: \"x\"\n    b\n}");
    // Shift+Enter too; a caret inside the indent keeps only the indent before it.
    s.handle(Input::NewLine, T);
    assert_eq!(s.panel().text(), "zone z {\n  a: \"x\"\n    b\n    \n}");
    s.handle(Input::Move(Motion::Left), T);
    s.handle(Input::Move(Motion::Left), T);
    s.handle(Input::Move(Motion::Left), T);
    s.handle(Input::Enter, T);
    assert_eq!(s.panel().text(), "zone z {\n  a: \"x\"\n    b\n \n    \n}");
    assert_eq!(s.panel().caret(), 27);
    // Tab replaces the selection.
    s.handle(Input::SelectAll, T);
    s.handle(Input::Tab, T);
    assert_eq!((s.panel().text(), s.panel().caret()), ("  ", 2));
}

fn mark(s: &mut Session, text: &str, selected: Option<std::ops::Range<usize>>, at: Duration) {
    let (text, replace) = (text.to_string(), None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        at,
    );
}

#[test]
fn ime_marked_text_shows_until_committed_and_counts_as_typed_only_then() {
    let mut s = panel("a: \"\"");
    s.handle(Input::Move(Motion::LineEnd), T);
    s.handle(Input::Move(Motion::Left), T);
    assert_eq!(s.panel().typed_at(), None);
    let ms = Duration::from_millis;
    mark(&mut s, "に", None, ms(10));
    mark(&mut s, "にほ", None, ms(20));
    assert_eq!(s.panel().text(), "a: \"にほ\"");
    assert_eq!(s.panel().marked(), Some(4..10));
    assert_eq!(s.panel().caret(), 10);
    // The IME selects part of its text.
    mark(&mut s, "日本", Some(0..3), ms(30));
    assert_eq!(s.panel().text(), "a: \"日本\"");
    assert_eq!((s.panel().anchor(), s.panel().caret()), (4, 7));
    assert_eq!(s.panel().typed_at(), None);
    let (text, replace) = ("日本".to_string(), None);
    s.handle(Input::Text { text, replace }, ms(40));
    assert_eq!(s.panel().text(), "a: \"日本\"");
    assert_eq!((s.panel().marked(), s.panel().caret()), (None, 10));
    assert_eq!(s.panel().typed_at(), Some(ms(40)));
    // Composing over a selection replaces it; an empty composition takes it back out.
    s.handle(Input::Select(Motion::Left), T);
    mark(&mut s, "x", None, T);
    assert_eq!(s.panel().text(), "a: \"日x\"");
    mark(&mut s, "", None, T);
    assert_eq!((s.panel().text(), s.panel().marked()), ("a: \"日\"", None));
    assert_eq!(s.panel().caret(), 7);
}

#[test]
fn any_other_input_commits_the_marked_text_first() {
    let mut s = panel("ab");
    s.handle(Input::Move(Motion::Right), T);
    mark(&mut s, "か", None, T);
    s.handle(Input::Move(Motion::Left), Duration::from_secs(1));
    assert_eq!(s.panel().text(), "aかb");
    assert_eq!((s.panel().marked(), s.panel().caret()), (None, 1));
    assert_eq!(s.panel().typed_at(), Some(Duration::from_secs(1)));
}

#[test]
fn new_text_from_outside_keeps_the_caret_inside_it() {
    let mut s = panel("a: \"日本\"");
    s.handle(Input::Move(Motion::Right), T);
    s.handle(Input::SelectAll, T);
    mark(&mut s, "か", None, T);
    // Regeneration lands: the caret ends where the text does, or before a cut character.
    s.set_panel_text("x".into());
    assert_eq!((s.panel().selection(), s.panel().marked()), (1..1, None));
    s.set_panel_text("日".into());
    typed(&mut s, "y");
    assert_eq!(s.panel().text(), "y日");
}

#[test]
fn the_clipboard_is_the_panel_selection_as_plain_text_not_shapes() {
    let mut s = panel("willow 1\n\na: \"x\"\n");
    s.select([ShapeId("a".into())]);
    // Nothing selected in the panel: nothing to cut or copy, not even the selected shape.
    assert!(!s.can_cut() && !s.can_copy() && s.can_paste());
    assert_eq!(s.clipboard_text(), None);
    s.handle(Input::Select(Motion::Down), T);
    assert!(s.can_cut() && s.can_copy());
    assert_eq!(s.clipboard_text().as_deref(), Some("willow 1\n"));
    s.handle(Input::Cut, T);
    assert_eq!(s.panel().text(), "\na: \"x\"\n");
    // Willow text pastes as it is, not as shapes; line breaks become `\n`, tabs two spaces.
    s.handle(Input::Paste("willow 1\r\n\tb: \"y\"\u{7}\r".into()), T);
    assert_eq!(s.panel().text(), "willow 1\n  b: \"y\"\n\na: \"x\"\n");
    assert_eq!(s.panel().caret(), 18);
    assert_eq!(*s.document(), Document::new());
    assert_eq!(s.selection(), [ShapeId("a".into())]);
}

/// Where to point at row `row`, a little right of column `col`'s left edge. The canvas is
/// 1000 px wide; the panel's text starts past its 1 px hairline and 40 px gutter, columns are
/// 7.5 px and rows 20.625 px from 60 px down.
fn at(row: f32, col: f32) -> Point {
    Point {
        x: 1041.0 + col * 7.5 + 2.0,
        y: 60.0 + row * 20.625 + 10.0,
    }
}

fn press(s: &mut Session, pos: Point, shift: bool) {
    let mods = Modifiers {
        shift,
        ..Modifiers::default()
    };
    s.handle(
        Input::PointerDown {
            pos,
            button: Button::Left,
            mods,
        },
        T,
    );
}

#[test]
fn a_press_puts_the_caret_there_and_a_drag_or_shift_press_selects() {
    let mut s = wrapped();
    s.handle(Input::Escape, T);
    press(&mut s, at(1.0, 3.0), false);
    assert!(s.panel().focused);
    assert_eq!(s.panel().selection(), 14..14);
    let mods = Modifiers::default();
    s.handle(
        Input::PointerMove {
            pos: at(2.0, 1.0),
            mods,
        },
        T,
    );
    assert_eq!((s.panel().anchor(), s.panel().caret()), (14, 60));
    let (pos, button) = (at(2.0, 1.0), Button::Left);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(
        Input::PointerMove {
            pos: at(3.0, 1.0),
            mods,
        },
        T,
    );
    assert_eq!(s.panel().caret(), 60);
    press(&mut s, at(0.0, 2.0), true);
    assert_eq!((s.panel().anchor(), s.panel().caret()), (14, 2));
    // Past a wrapped row's end the caret stays on that row; below the text, the last row.
    press(&mut s, at(1.0, 90.0), false);
    s.handle(Input::Move(Motion::LineStart), T);
    assert_eq!(s.panel().caret(), 11);
    press(&mut s, at(9.0, 2.0), false);
    assert_eq!(s.panel().caret(), 64);
}

#[test]
fn double_click_selects_a_word_and_triple_click_a_line() {
    let mut s = wrapped();
    press(&mut s, at(0.0, 9.0), false);
    s.handle(Input::DoubleClick { pos: at(0.0, 9.0) }, T);
    assert_eq!(s.panel().selection(), 0..10);
    s.handle(Input::DoubleClick { pos: at(3.0, 2.0) }, T);
    assert_eq!(s.panel().selection(), 62..67);
    s.handle(Input::TripleClick { pos: at(1.0, 2.0) }, T);
    assert_eq!(s.panel().selection(), 0..62);
    s.handle(Input::TripleClick { pos: at(3.0, 2.0) }, T);
    assert_eq!(s.panel().selection(), 62..67);
}

#[test]
fn delete_takes_the_selection_or_up_to_where_the_motion_goes() {
    let mut s = panel("a: \"x\" below b");
    s.handle(Input::Move(Motion::LineEnd), T);
    s.handle(Input::Delete(Motion::WordLeft), T);
    assert_eq!(s.panel().text(), "a: \"x\" below ");
    s.handle(Input::Delete(Motion::Left), T);
    assert_eq!(s.panel().text(), "a: \"x\" below");
    s.handle(Input::Select(Motion::WordLeft), T);
    s.handle(Input::Delete(Motion::Right), T);
    assert_eq!(s.panel().text(), "a: \"x\" ");
    assert_eq!(s.panel().caret(), 7);
    s.handle(Input::Delete(Motion::Right), T);
    assert_eq!(s.panel().text(), "a: \"x\" ");
}

/// Panel drawing coordinates (`PanelRow`'s): x from inside the hairline, so text starts at
/// the 40 px gutter, y from the panel's top.
fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// `rects` to the nearest 1/8 px: column widths aren't exact in `f32`.
fn round(rects: Vec<(Point, Point)>) -> Vec<(Point, Point)> {
    let r = |q: Point| p((q.x * 8.0).round() / 8.0, (q.y * 8.0).round() / 8.0);
    rects.into_iter().map(|(a, b)| (r(a), r(b))).collect()
}

#[test]
fn the_caret_blinks_where_it_stops_on_the_column_grid() {
    let mut s = wrapped();
    press(&mut s, at(1.0, 3.0), false);
    let c = s.panel_caret(T).unwrap();
    assert_eq!(
        (c.pos, c.height, c.visible),
        (p(62.5, 80.625), 20.625, true)
    );
    assert_eq!(s.panel_point(14), p(62.5, 80.625));
    assert_eq!(s.next_wake(T), Some(Duration::from_millis(530)));
    assert!(!s.panel_caret(Duration::from_millis(600)).unwrap().visible);
    assert_eq!(s.next_wake(Duration::from_secs(5)), None);
    // At a wrapped row's end it shows there, not at the next row's start.
    s.handle(Input::Move(Motion::LineEnd), T);
    assert_eq!(s.panel_caret(T).unwrap().pos, p(400.0, 80.625));
    // None once the canvas has the keys.
    s.handle(Input::Escape, T);
    assert_eq!(s.panel_caret(T), None);
}

#[test]
fn selection_and_marked_text_are_one_rect_per_row() {
    let mut s = wrapped();
    s.handle(Input::TripleClick { pos: at(0.0, 0.0) }, T);
    s.handle(Input::Move(Motion::Left), T);
    for _ in 0..5 {
        s.handle(Input::Move(Motion::Right), T);
    }
    for _ in 0..11 {
        s.handle(Input::Select(Motion::Right), T);
    }
    assert_eq!(
        round(s.panel_rects(s.panel().selection())),
        [
            (p(77.5, 60.0), p(122.5, 80.625)),
            (p(40.0, 80.625), p(77.5, 101.25))
        ]
    );
    // A selected line break shows as one more column.
    let mut s = panel("ab\ncd");
    s.handle(Input::Move(Motion::Right), T);
    for _ in 0..3 {
        s.handle(Input::Select(Motion::Right), T);
    }
    assert_eq!(
        round(s.panel_rects(1..4)),
        [
            (p(47.5, 60.0), p(62.5, 80.625)),
            (p(40.0, 80.625), p(47.5, 101.25))
        ]
    );
}

#[test]
fn keys_scroll_the_caret_into_view() {
    let lines: Vec<String> = (1..=1000)
        .map(|i| format!("@layout a {i},0 w210"))
        .collect();
    let mut s = panel(&lines.join("\n"));
    for _ in 0..40 {
        s.handle(Input::Move(Motion::Down), T);
    }
    // Row 40 sits just above the 14 px bottom padding of the 800 px panel.
    let y = s.panel_caret(T).unwrap().pos.y;
    assert!((y - (800.0 - 14.0 - 20.625)).abs() < 1e-3, "{y}");
    assert!(s.panel_rows().iter().any(|r| r.number == Some(41)));
    // Scrolled away, typing brings it back to the top, below the 60 px top padding.
    s.handle(Input::ScrollPanel(-1e9), T);
    typed(&mut s, "x");
    let y = s.panel_caret(T).unwrap().pos.y;
    assert!((y - 60.0).abs() < 1e-3, "{y}");
}

/// Each row in view as (text, color) runs.
fn colors(s: &Session) -> Vec<Vec<(String, Rgba)>> {
    let text = s.panel().text();
    (s.panel_rows().iter())
        .map(|r| (r.spans.iter()).map(|sp| (text[sp.range.clone()].to_string(), sp.color)))
        .map(Iterator::collect)
        .collect()
}

#[test]
fn half_typed_lines_color_only_themselves() {
    let mut s = panel("\nb: \"x\" below a\n");
    let (c, ink) = (PAPER.code, PAPER.ink);
    let rest = vec![
        ("b".to_string(), c.id),
        (": ".into(), ink),
        ("\"x\"".into(), c.string),
        (" ".into(), ink),
        ("below".into(), c.direction),
        (" a".into(), ink),
    ];
    // Each keystroke of an unclosed quote, a partial keyword, a half direction word.
    for line in ["a: \"Hi b", "zon", "a: \"\" bel", "@lay"] {
        s.handle(Input::Move(Motion::LineStart), T);
        s.handle(Input::Select(Motion::LineEnd), T);
        for ch in line.chars() {
            typed(&mut s, &ch.to_string());
            let rows = colors(&s);
            assert_eq!(rows[1], rest, "{line:?} at {ch:?}");
            let shown: String = rows[0].iter().map(|r| r.0.as_str()).collect();
            assert_eq!(s.panel().text().split('\n').next(), Some(shown.as_str()));
        }
    }
    assert_eq!(
        colors(&panel("a: \"Hi b\n"))[0],
        [
            ("a".to_string(), c.id),
            (": ".into(), ink),
            ("\"Hi b".into(), c.string)
        ]
    );
}

#[test]
fn keys_that_change_nothing_are_not_typing() {
    let mut s = panel("ab");
    s.handle(Input::Delete(Motion::Left), T);
    s.handle(Input::Paste(String::new()), T);
    s.handle(Input::Move(Motion::Right), T);
    assert_eq!(s.panel().typed_at(), None);
}

#[test]
fn the_caret_blinks_from_when_the_panel_takes_focus() {
    let mut s = common::session_of(Document::new());
    let t = Duration::from_secs(10);
    s.handle(Input::TogglePanel, t);
    assert_eq!(s.next_wake(t), Some(Duration::from_millis(530)));
}

#[test]
fn undo_and_redo_do_nothing_while_the_panel_has_focus() {
    let mut s = common::session_of(Document::new());
    s.select([ShapeId("a".into())]);
    s.handle(Input::DeleteShapes, T);
    s.handle(Input::TogglePanel, T);
    let opened = s.take_regeneration().unwrap().run();
    s.regenerated(opened);
    s.set_panel_text(String::new());
    typed(&mut s, "x");
    for i in [Input::Undo, Input::Redo] {
        s.handle(i, T);
        assert!(s.document().shapes.is_empty());
        assert_eq!(s.panel().text(), "x");
        assert!(s.take_regeneration().is_none());
    }
}
