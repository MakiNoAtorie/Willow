//! IME composition: marked text goes through the editor session.
mod common;

use std::ops::Range;
use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Button, Input, Modifiers, Point, Recovery, Session};

const T: Duration = Duration::ZERO;
/// Inside the centered default shape.
const IN: Point = Point { x: 420.0, y: 395.0 };

/// A session editing `text`, caret at the end.
fn editing(text: &str) -> Session {
    let mut s = session_of(doc_with(text));
    click(&mut s, IN);
    s.handle(Input::StartEditing, T);
    s
}

fn click(s: &mut Session, pos: Point) {
    let (button, mods) = (Button::Left, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

fn mark(s: &mut Session, text: &str, selected: Option<Range<usize>>) {
    let (text, replace) = (text.into(), None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        T,
    );
}

fn commit(s: &mut Session, text: &str) {
    let (text, replace) = (text.into(), None);
    s.handle(Input::Text { text, replace }, T);
}

fn plain(s: &Session) -> String {
    s.document().shapes[0].text.plain()
}

/// Marked range, caret and anchor.
fn state(s: &Session) -> (Option<Range<usize>>, usize, usize) {
    let e = s.editing().expect("editing");
    (e.marked.clone(), e.caret.offset, e.anchor)
}

#[test]
fn marked_text_is_shown_replaced_and_committed_and_only_the_commit_is_undoable() {
    let mut s = editing("ab");

    mark(&mut s, "k", None);
    assert_eq!((plain(&s), state(&s)), ("abk".into(), (Some(2..3), 3, 3)));
    assert!(!s.can_undo());

    // Each composition update replaces the marked text.
    mark(&mut s, "か", None);
    mark(&mut s, "かん", None);
    assert_eq!(
        (plain(&s), state(&s)),
        ("abかん".into(), (Some(2..8), 8, 8))
    );
    // The IME's selection is relative to the marked text.
    mark(&mut s, "漢字", Some(0..3));
    assert_eq!(
        (plain(&s), state(&s)),
        ("ab漢字".into(), (Some(2..8), 5, 2))
    );
    assert!(!s.can_undo());

    // Committing replaces the marked text and is the session's only change.
    commit(&mut s, "漢字");
    assert_eq!((plain(&s), state(&s)), ("ab漢字".into(), (None, 8, 8)));
    assert!(s.can_undo());
    s.handle(Input::StopEditing, T);
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");
    s.handle(Input::Redo, T);
    assert_eq!(plain(&s), "ab漢字");
}

#[test]
fn a_cancelled_composition_leaves_the_text_and_adds_no_undo_step() {
    let mut s = editing("ab");
    mark(&mut s, "か", None);
    mark(&mut s, "", None);
    assert_eq!((plain(&s), state(&s)), ("ab".into(), (None, 2, 2)));
    mark(&mut s, "か", None);
    commit(&mut s, "");
    assert_eq!(plain(&s), "ab");
    s.handle(Input::StopEditing, T);
    assert!(!s.can_undo());
}

#[test]
fn composing_over_other_text_deletes_it_as_an_undoable_edit() {
    let mut s = editing("abc");
    let (text, replace) = ("か".into(), Some(1..2));
    s.handle(
        Input::Mark {
            text,
            replace,
            selected: None,
        },
        T,
    );
    assert_eq!((plain(&s), state(&s)), ("aかc".into(), (Some(1..4), 4, 4)));
    // Cancelled: the deletion stays, and undo brings "b" back.
    mark(&mut s, "", None);
    assert_eq!(plain(&s), "ac");
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "abc");
}

#[test]
fn leaving_editing_or_unmarking_commits_the_marked_text_as_it_is() {
    let mut s = editing("ab");
    mark(&mut s, "か", None);
    s.handle(Input::Unmark, T);
    assert_eq!((plain(&s), state(&s)), ("abか".into(), (None, 5, 5)));

    mark(&mut s, "な", None);
    s.handle(Input::StopEditing, T);
    assert_eq!(plain(&s), "abかな");
    s.handle(Input::Undo, T);
    assert_eq!(plain(&s), "ab");

    // A dead key: its mark, then the composed character.
    let mut s = editing("ab");
    mark(&mut s, "\u{b4}", Some(2..2));
    commit(&mut s, "\u{e9}");
    assert_eq!((plain(&s), state(&s)), ("ab\u{e9}".into(), (None, 4, 4)));
}

#[test]
fn composing_on_a_selected_shape_starts_editing_at_the_end() {
    let mut s = session_of(doc_with("ab"));
    click(&mut s, IN);
    mark(&mut s, "か", None);
    assert_eq!((plain(&s), state(&s)), ("abか".into(), (Some(2..5), 5, 5)));
}

#[test]
fn the_candidate_window_goes_where_the_marked_text_is() {
    let mut s = editing("ab");
    mark(&mut s, "かな", None);
    // Canvas point of an offset in the edited text: the shape is at (0, 0), "ab" is 20 wide.
    let pad = &willow_core::PAPER.shape;
    let p = s.text_point(2).unwrap();
    assert_eq!((p.x, p.y), (pad.pad_x + 20.0, pad.pad_y));
    assert_eq!(s.text_point(8).unwrap().x, pad.pad_x + 50.0);
}

#[test]
fn composed_text_takes_the_pending_style() {
    let mut s = editing("ab");
    s.handle(Input::ToggleBold, T);
    mark(&mut s, "か", None);
    assert!(s.document().shapes[0].text.paragraphs[0].runs[1].bold);
    commit(&mut s, "漢");
    let runs = &s.document().shapes[0].text.paragraphs[0].runs;
    assert_eq!((runs[1].text.as_str(), runs[1].bold), ("漢", true));
}

#[test]
fn save_commits_the_marked_text_first() {
    let mut s = editing("ab");
    mark(&mut s, "k", None);
    let mut written = String::new();
    s.save(|b| {
        written = String::from_utf8(b.to_vec()).unwrap();
        Ok(())
    })
    .unwrap();
    assert!(written.contains("a: \"abk\""), "{written}");
    assert_eq!(state(&s), (None, 3, 3));
    assert!(s.is_saved());
}

#[test]
fn a_recovery_leaves_out_the_marked_text_and_keeps_composing() {
    let mut s = editing("ab");
    commit(&mut s, "x");
    mark(&mut s, "k", None);
    let Some(Recovery::Write { document, .. }) = s.take_recovery(Duration::from_secs(5)) else {
        panic!("a write due");
    };
    assert_eq!(document.shapes[0].text.plain(), "abx");
    assert_eq!(state(&s), (Some(3..4), 4, 4));
}
