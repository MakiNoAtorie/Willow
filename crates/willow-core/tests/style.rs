mod common;

use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Button, Input, Modifiers, Motion, Point, Run, Session};

const T: Duration = Duration::ZERO;

fn started(text: &str) -> Session {
    let mut s = session_of(doc_with(text));
    let pos = Point { x: 411.0, y: 389.0 };
    let mods = Modifiers::default();
    let button = Button::Left;
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(Input::StartEditing, T);
    s
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

/// The first paragraph's runs as (text, bold, italic).
fn runs(s: &Session) -> Vec<(String, bool, bool)> {
    s.document().shapes[0].text.paragraphs[0]
        .runs
        .iter()
        .map(|r: &Run| (r.text.clone(), r.bold, r.italic))
        .collect()
}

fn r(t: &str, b: bool, i: bool) -> (String, bool, bool) {
    (t.into(), b, i)
}

#[test]
fn toggling_a_selection_splits_runs_and_toggling_back_merges_them() {
    let mut s = started("abcd");
    s.set_text_selection(1..3);
    s.handle(Input::ToggleBold, T);
    assert_eq!(
        runs(&s),
        [
            r("a", false, false),
            r("bc", true, false),
            r("d", false, false)
        ]
    );

    s.handle(Input::ToggleItalic, T);
    assert_eq!(runs(&s)[1], r("bc", true, true));

    s.handle(Input::ToggleBold, T);
    s.handle(Input::ToggleItalic, T);
    assert_eq!(runs(&s), [r("abcd", false, false)]);
}

#[test]
fn a_mixed_selection_becomes_bold_first() {
    let mut s = started("abcd");
    s.set_text_selection(0..2);
    s.handle(Input::ToggleBold, T);
    s.set_text_selection(0..4);
    s.handle(Input::ToggleBold, T);
    assert_eq!(runs(&s), [r("abcd", true, false)]);
}

#[test]
fn with_no_selection_the_style_applies_to_the_next_typed_text_only() {
    let mut s = started("ab");
    s.handle(Input::ToggleBold, T);
    // Nothing changes until something is typed.
    assert_eq!(runs(&s), [r("ab", false, false)]);
    typ(&mut s, "x");
    // Typing on continues the style of the text before it, as usual; the pending toggle is spent.
    typ(&mut s, "y");
    s.handle(Input::Move(willow_core::Motion::Left), T);
    s.handle(Input::Move(willow_core::Motion::Left), T);
    typ(&mut s, "z");
    assert_eq!(runs(&s), [r("abz", false, false), r("xy", true, false)]);
}

#[test]
fn a_pending_style_is_dropped_when_the_caret_moves() {
    let mut s = started("ab");
    s.handle(Input::ToggleItalic, T);
    s.handle(Input::Move(willow_core::Motion::Left), T);
    typ(&mut s, "x");
    assert_eq!(runs(&s), [r("axb", false, false)]);
}

#[test]
fn mixed_runs_justify_to_the_content_width() {
    let mut s = started("aaaa bbbb cccc dddd eeee ffff");
    s.set_text_selection(5..9);
    s.handle(Input::ToggleBold, T);
    let l = s.layout(&s.document().shapes[0]);
    let line = &l.lines[0];
    let text = s.document().shapes[0].text.plain();
    let right = (line.clusters.iter())
        .filter(|c| &text[c.range.clone()] != " ")
        .map(|c| c.x + c.width)
        .fold(0.0, f32::max);
    assert_eq!(right, 178.0);
}

#[test]
fn formatting_is_part_of_the_sessions_single_undo_step() {
    let mut s = started("abcd");
    typ(&mut s, "x");
    s.set_text_selection(0..2);
    s.handle(Input::ToggleBold, T);
    s.handle(Input::Escape, T);
    s.handle(Input::Undo, T);
    assert_eq!(runs(&s), [r("abcd", false, false)]);
    assert!(!s.can_undo());
}

/// Each paragraph's runs as (text, bold).
fn bold_runs(s: &Session) -> Vec<Vec<(String, bool)>> {
    (s.document().shapes[0].text.paragraphs.iter())
        .map(|p| p.runs.iter().map(|r| (r.text.clone(), r.bold)).collect())
        .collect()
}

fn b(t: &str, bold: bool) -> (String, bool) {
    (t.into(), bold)
}

#[test]
fn a_toggle_across_a_paragraph_break_turns_off_only_when_every_paragraph_has_it() {
    let mut s = started("a\nb");
    s.handle(Input::SelectAll, T);
    s.handle(Input::ToggleBold, T);
    assert_eq!(bold_runs(&s), [[b("a", true)], [b("b", true)]]);
    s.handle(Input::ToggleBold, T);
    assert_eq!(
        bold_runs(&s),
        [[b("a", false)], [b("b", false)]],
        "all bold across the break: off"
    );

    s.set_text_selection(0..1);
    s.handle(Input::ToggleBold, T);
    s.handle(Input::SelectAll, T);
    s.handle(Input::ToggleBold, T);
    assert_eq!(bold_runs(&s), [[b("a", true)], [b("b", true)]], "mixed: on");
}

#[test]
fn a_line_selected_with_its_paragraph_break_toggles_off() {
    let mut s = started("a\nb");
    s.handle(Input::SelectAll, T);
    s.handle(Input::ToggleBold, T);
    s.handle(Input::Move(Motion::Up), T);
    s.handle(Input::Move(Motion::LineStart), T);
    s.handle(Input::Select(Motion::Down), T);
    assert_eq!(s.selected_text().as_deref(), Some("a\n"));
    s.handle(Input::ToggleBold, T);
    assert_eq!(bold_runs(&s), [[b("a", false)], [b("b", true)]]);
}
