//! IME composition: marked text goes through the editor session.
mod common;

use std::ops::Range;
use std::time::Duration;

use common::{doc_with, session_of};
use willow_core::{Button, Input, Link, Modifiers, Point, Recovery, Session, Shape, ShapeId};

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
    mark_at(s, text, selected, T);
}

fn mark_at(s: &mut Session, text: &str, selected: Option<Range<usize>>, now: Duration) {
    let (text, replace) = (text.into(), None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        now,
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
    s.handle(Input::Escape, T);
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
    s.handle(Input::Escape, T);
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
    s.handle(Input::Escape, T);
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
fn a_recovery_waits_for_the_composition_and_comes_due_when_it_ends() {
    let mut s = editing("ab");
    commit(&mut s, "x");
    mark(&mut s, "k", None);
    let due = Duration::from_secs(5);
    assert_eq!(s.recovery_wake(due), None);
    assert_eq!(s.take_recovery(due), None);
    assert_eq!(state(&s), (Some(3..4), 4, 4));

    mark(&mut s, "", None);
    assert_eq!(s.recovery_wake(due), Some(Duration::ZERO));
    let Some(Recovery::Write { document, .. }) = s.take_recovery(due) else {
        panic!("a write due");
    };
    assert_eq!(document.shapes[0].text.plain(), "abx");
}

#[test]
fn a_saved_document_reads_unsaved_only_while_composing() {
    let mut s = editing("ab");
    assert!(s.is_saved());
    mark(&mut s, "k", None);
    assert!(!s.is_saved());
    mark(&mut s, "", None);
    assert!(s.is_saved());
}

#[test]
fn heights_hold_the_marked_text_while_it_lasts() {
    let mut s = editing("ab");
    let plain = s.heights()[0];
    mark(&mut s, &"k ".repeat(100), None);
    assert!(s.heights()[0] > plain);
    mark(&mut s, "", None);
    assert_eq!(s.heights()[0], plain);
}

#[test]
fn heights_follow_typed_text_and_its_undo() {
    let mut s = editing("ab");
    let plain = s.heights()[0];
    commit(&mut s, &"k ".repeat(100));
    assert!(s.heights()[0] > plain);
    s.handle(Input::Undo, T);
    assert_eq!(s.heights()[0], plain);
}

#[test]
fn ranges_past_the_end_or_inside_a_character_are_clamped() {
    let mut s = editing("ab");
    s.handle(
        Input::Mark {
            text: "x".into(),
            replace: Some(10..20),
            selected: Some(5..9),
        },
        T,
    );
    assert_eq!((plain(&s), state(&s)), ("abx".into(), (Some(2..3), 3, 3)));

    // Inside "か" (bytes 3..6) the selection falls back to its start.
    mark(&mut s, "かな", Some(1..4));
    assert_eq!(state(&s), (Some(2..8), 5, 2));
    s.handle(Input::Move(willow_core::Motion::Left), T);
    s.handle(Input::Unmark, T);
    assert_eq!(plain(&s), "abかな");
}

/// The shape `b` linked one creation gap below the shape being edited, `ab` held by the document.
fn with_neighbor() -> Session {
    let mut doc = doc_with("ab");
    let a = doc.shapes[0].clone();
    let b = ShapeId("b".into());
    doc.links.push(Link(a.id.clone(), b.clone()));
    doc.shapes.push(Shape::new(b, a.x, a.y + 97));
    let mut s = session_of(doc);
    s.select([a.id]);
    s.handle(Input::StartEditing, T);
    s
}

fn neighbor(s: &Session) -> i32 {
    let r = s.drawn_rect(&ShapeId("b".into())).expect("b");
    r.y as i32
}

/// How far `b` is drawn off its rest position just after a change at `T`, slides only.
fn neighbor_slide(s: &Session) -> f32 {
    let (from, to) = (
        Point {
            x: -999.0,
            y: -999.0,
        },
        Point { x: 999.0, y: 999.0 },
    );
    let f = s.draw(from, to, T);
    f.shapes
        .iter()
        .find(|v| v.shape.id.0 == "b")
        .expect("b")
        .slide
        .y
}

#[test]
fn every_composition_change_reruns_the_pass_and_slides_and_a_cancel_settles_back() {
    let mut s = with_neighbor();
    let rest = neighbor(&s);
    let plain = s.heights();

    mark(&mut s, &"k ".repeat(100), None);
    let pushed = neighbor(&s);
    assert!(pushed > rest, "{pushed}");
    assert_eq!(neighbor_slide(&s), (rest - pushed) as f32);

    mark(&mut s, &"k ".repeat(50), None);
    let less = neighbor(&s);
    assert!(rest < less && less < pushed, "{less}");

    mark_at(&mut s, "", None, Duration::from_millis(300));
    assert_eq!(neighbor(&s), rest);
    assert_eq!(s.heights(), plain);
    assert_eq!(neighbor_slide(&s), (less - rest) as f32);
    assert_eq!(s.document().shapes[1].y, rest, "never a saved position");
    assert!(s.is_saved());
    s.handle(Input::Escape, T);
    assert!(!s.can_undo());
}

#[test]
fn a_commit_keeps_where_the_composition_pushed_the_neighbor() {
    let mut s = with_neighbor();
    mark(&mut s, &"k ".repeat(100), None);
    let pushed = neighbor(&s);
    commit(&mut s, &"k ".repeat(100));
    assert_eq!(neighbor(&s), pushed);
}

#[test]
fn a_recovery_after_a_composition_pairs_plain_heights_with_positions_that_match() {
    let mut s = with_neighbor();
    let rest = neighbor(&s);
    commit(&mut s, "x");
    let typed = s.heights();
    mark(&mut s, &"k ".repeat(100), None);
    assert!(neighbor(&s) > rest);
    mark(&mut s, "", None);
    let Some(Recovery::Write { heights, drawn, .. }) = s.take_recovery(Duration::from_secs(5))
    else {
        panic!("a write due");
    };
    assert_eq!((heights, drawn[1].1), (typed, rest));
}

#[test]
fn regeneration_waits_for_the_end_of_editing_and_never_reads_a_composition() {
    let mut s = with_neighbor();
    s.show_panel();
    mark(&mut s, "zzz", None);
    assert!(s.take_regeneration().is_none());
    mark(&mut s, "", None);
    s.handle(Input::Escape, T);
    let r = s.take_regeneration().expect("the panel opened");
    s.regenerated(r.run());
    assert!(!s.panel().text().contains("zzz"), "{}", s.panel().text());
}

#[test]
fn a_cancelled_composition_keeps_panel_text_that_holds_the_document() {
    let bytes = b"willow 1\n\na: \"ab\"\n\n@layout a 0,0 w210\n@layout oops\n".to_vec();
    let load = willow_core::load(&bytes).unwrap();
    let mut s = Session::open(
        load,
        bytes.clone(),
        &willow_core::PAPER,
        Box::new(common::Fake),
    );
    let text = s.panel().text().to_string();
    assert!(text.contains("oops"), "{text}");
    s.select([ShapeId("a".into())]);
    s.handle(Input::StartEditing, T);
    mark(&mut s, "k", None);
    mark(&mut s, "", None);
    s.handle(Input::Escape, T);
    assert_eq!(s.panel().text(), text);
    assert!(s.take_regeneration().is_none());
}
