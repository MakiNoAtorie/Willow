//! Undo in the code panel: applied text edits merge into one step until a boundary, and every
//! step brings back the panel text it had when that wasn't plain regenerated text.

mod common;

use std::time::Duration;
use willow_core::{Input, Motion, Session};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What the app does after every input: runs a due regeneration and hands the text back.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        s.regenerated(job.run());
    }
}

/// What the app does when the text check's wake comes.
fn pause(s: &mut Session, now: Duration) {
    if let Some(job) = s.take_text_check(now) {
        s.text_checked(job.run());
    }
    settle(s);
}

/// A session over the document `text` loads into, the panel open, focused and regenerated.
fn panel_of(text: &str) -> Session {
    let loaded = willow_core::load(text.as_bytes()).unwrap();
    let mut s = Session::open(
        loaded,
        text.into(),
        &willow_core::PAPER,
        Box::new(common::Fake),
    );
    s.resize(1000.0, 800.0);
    s.handle(Input::TogglePanel, ms(0));
    settle(&mut s);
    s
}

/// Puts the caret right after the first `after` in the panel text.
fn caret_after(s: &mut Session, after: &str, now: Duration) {
    let at = s.panel().text().find(after).expect("in the text") + after.len();
    let text = s.panel().text().to_owned();
    // Text replacing an empty range at `at` types nothing and leaves the caret there.
    let (t, replace) = (String::new(), Some(at..at));
    s.handle(Input::Text { text: t, replace }, now);
    assert_eq!(s.panel().text(), text);
}

/// Types `text` at the caret, one character at a time, all at `now`.
fn type_in(s: &mut Session, text: &str, now: Duration) {
    for c in text.chars() {
        let (text, replace) = (c.to_string(), None);
        s.handle(Input::Text { text, replace }, now);
    }
}

fn shape_text(s: &Session, id: &str) -> String {
    let shape = s.document().shapes.iter().find(|s| s.id.0 == id);
    shape.expect("the shape is there").text.plain().to_owned()
}

const TWO: &str = "willow 1\n\na: \"Gates\"\nb: \"Fences\" below a\n\n\
@layout a 0,0 w210\n@layout b 0,100 w210\n";

#[test]
fn typing_merges_into_one_step_until_a_one_second_pause() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    type_in(&mut s, "!", ms(900));
    pause(&mut s, ms(1050));
    assert_eq!(shape_text(&s, "a"), "Gates!!");
    // 1 s since the last keystroke: the next typing is a step of its own.
    type_in(&mut s, "?", ms(1900));
    pause(&mut s, ms(2050));
    assert_eq!(shape_text(&s, "a"), "Gates!!?");
    s.handle(Input::Undo, ms(2100));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Gates!!");
    s.handle(Input::Undo, ms(2200));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Gates");
    assert!(!s.can_undo());
}

/// Undoes at `now` and lands the regeneration that follows.
fn undo(s: &mut Session, now: Duration) {
    s.handle(Input::Undo, now);
    settle(s);
}

fn texts(s: &Session) -> (String, String) {
    (shape_text(s, "a"), shape_text(s, "b"))
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.into(), b.into())
}

#[test]
fn moving_the_caret_to_another_line_ends_the_step() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    // Along the line, the next typing still merges.
    s.handle(Input::Move(Motion::Left), ms(200));
    s.handle(Input::Move(Motion::Right), ms(250));
    type_in(&mut s, "!", ms(300));
    pause(&mut s, ms(450));
    s.handle(Input::Move(Motion::Down), ms(500));
    caret_after(&mut s, "Fences", ms(800));
    type_in(&mut s, "?", ms(800));
    pause(&mut s, ms(950));
    assert_eq!(texts(&s), pair("Gates!!", "Fences?"));
    undo(&mut s, ms(1000));
    assert_eq!(texts(&s), pair("Gates!!", "Fences"));
    undo(&mut s, ms(1100));
    assert_eq!(texts(&s), pair("Gates", "Fences"));
}

#[test]
fn a_paste_and_a_cut_are_each_a_step_of_their_own() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    s.handle(Input::Paste("X".into()), ms(400));
    pause(&mut s, ms(550));
    type_in(&mut s, "?", ms(700));
    pause(&mut s, ms(850));
    s.handle(Input::Select(Motion::Left), ms(1000));
    s.handle(Input::Cut, ms(1000));
    pause(&mut s, ms(1150));
    type_in(&mut s, ".", ms(1300));
    pause(&mut s, ms(1450));
    let mut seen = vec![shape_text(&s, "a")];
    for n in 0..5 {
        undo(&mut s, ms(1500 + n * 100));
        seen.push(shape_text(&s, "a"));
    }
    let steps = [
        "Gates!X.", "Gates!X", "Gates!X?", "Gates!X", "Gates!", "Gates",
    ];
    assert_eq!(seen, steps);
    assert!(!s.can_undo());
}

#[test]
fn leaving_the_panel_ends_the_step() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    s.handle(Input::Escape, ms(300));
    settle(&mut s);
    s.handle(Input::FocusPanel, ms(400));
    caret_after(&mut s, "Gates!", ms(400));
    type_in(&mut s, "?", ms(500));
    pause(&mut s, ms(650));
    undo(&mut s, ms(700));
    assert_eq!(shape_text(&s, "a"), "Gates!");
    undo(&mut s, ms(800));
    assert_eq!(shape_text(&s, "a"), "Gates");
}

#[test]
fn undo_applies_pending_text_to_the_step_first_and_ends_it() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    type_in(&mut s, "!", ms(500));
    // Still pending: one Undo takes back the whole stretch.
    undo(&mut s, ms(550));
    assert_eq!(shape_text(&s, "a"), "Gates");
    assert!(!s.can_undo());
    s.handle(Input::Redo, ms(600));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Gates!!");
    caret_after(&mut s, "Gates!!", ms(700));
    type_in(&mut s, "?", ms(700));
    pause(&mut s, ms(850));
    undo(&mut s, ms(900));
    assert_eq!(shape_text(&s, "a"), "Gates!!");
}

/// Line numbers (from 1) of the rows drawn invalid.
fn invalid_rows(s: &Session) -> Vec<usize> {
    (s.panel_rows().into_iter())
        .filter(|r| r.invalid)
        .filter_map(|r| r.number)
        .collect()
}

fn canonical(s: &Session) -> String {
    willow_core::save(s.document(), &s.heights(), &s.drawn_positions())
}

/// `TWO` with `b` linked below a missing `azz`: line 4 is invalid.
fn invalid_typed(now: Duration) -> Session {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "below a", now);
    type_in(&mut s, "zz", now);
    pause(&mut s, now + ms(150));
    assert_eq!(invalid_rows(&s), [4]);
    s
}

#[test]
fn undoing_a_canvas_edit_that_dropped_invalid_lines_brings_them_back() {
    let mut s = invalid_typed(ms(0));
    let typed = s.panel().text().to_owned();
    let held = s.document().clone();
    s.handle(Input::Escape, ms(200));
    settle(&mut s);
    assert_eq!(s.panel().text(), typed);
    s.select([willow_core::ShapeId("a".into())]);
    s.handle(Input::Nudge(willow_core::Side::Right), ms(300));
    settle(&mut s);
    assert_eq!(s.panel().text(), canonical(&s));
    let nudged = s.document().clone();
    undo(&mut s, ms(400));
    assert_eq!(*s.document(), held);
    assert_eq!(s.panel().text(), typed);
    assert_eq!(invalid_rows(&s), [4]);
    s.handle(Input::Redo, ms(500));
    settle(&mut s);
    assert_eq!(*s.document(), nudged);
    assert_eq!(s.panel().text(), canonical(&s));
    assert_eq!(invalid_rows(&s), Vec::<usize>::new());
}

#[test]
fn undo_and_redo_while_a_line_is_invalid_round_trip_the_typing() {
    let mut s = invalid_typed(ms(0));
    let (typed, before) = (s.panel().text().to_owned(), s.document().clone());
    // Nothing applied, yet Undo has the typing to take back.
    assert!(s.can_undo());
    for n in 0..2 {
        undo(&mut s, ms(200 + n * 200));
        assert_eq!(s.panel().text(), TWO, "{n}");
        assert_eq!(invalid_rows(&s), Vec::<usize>::new(), "{n}");
        assert!(!s.can_undo(), "{n}");
        s.handle(Input::Redo, ms(300 + n * 200));
        settle(&mut s);
        assert_eq!(s.panel().text(), typed, "{n}");
        assert_eq!(invalid_rows(&s), [4], "{n}");
        assert_eq!(*s.document(), before, "{n}");
    }
}

#[test]
fn undo_while_invalid_returns_to_the_last_valid_text_and_keeps_the_document() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    let valid = s.panel().text().to_owned();
    caret_after(&mut s, "below a", ms(300));
    type_in(&mut s, "zz", ms(300));
    // Still pending when Undo comes: checked first, found invalid.
    undo(&mut s, ms(350));
    assert_eq!(s.panel().text(), valid);
    assert_eq!(shape_text(&s, "a"), "Gates!");
    undo(&mut s, ms(450));
    assert_eq!(shape_text(&s, "a"), "Gates");
    s.handle(Input::Redo, ms(550));
    s.handle(Input::Redo, ms(560));
    settle(&mut s);
    assert!(
        s.panel()
            .text()
            .contains("Gates!\"\nb: \"Fences\" below azz")
    );
    assert_eq!(invalid_rows(&s), [4]);
}

#[test]
fn a_reload_from_disk_is_undone_with_the_panel_text_it_replaced() {
    let invalid = TWO.replace("below a", "below azz");
    let mut s = panel_of(&invalid);
    // The invalid file opened the panel; `panel_of` toggled it shut.
    s.handle(Input::TogglePanel, ms(0));
    s.handle(Input::Escape, ms(0));
    assert!(s.panel().open);
    let held = s.document().clone();
    s.disk_changed(Some(TWO.replace("Gates", "Posts").as_bytes()));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Posts");
    assert_eq!(s.panel().text(), canonical(&s));
    undo(&mut s, ms(100));
    assert_eq!(*s.document(), held);
    assert_eq!(s.panel().text(), invalid);
    assert_eq!(invalid_rows(&s), [4]);
    s.handle(Input::Redo, ms(200));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Posts");
    assert_eq!(s.panel().text(), canonical(&s));
    // And the other way: a valid document reloaded as an invalid file.
    s.disk_changed(Some(invalid.as_bytes()));
    settle(&mut s);
    assert_eq!(s.panel().text(), invalid);
    undo(&mut s, ms(300));
    assert_eq!(shape_text(&s, "a"), "Posts");
    assert_eq!(s.panel().text(), canonical(&s));
    s.handle(Input::Redo, ms(400));
    settle(&mut s);
    assert_eq!(s.panel().text(), invalid);
    assert_eq!(invalid_rows(&s), [4]);
}

#[test]
fn undoing_a_reload_brings_back_invalid_typing_that_was_saved() {
    let mut s = invalid_typed(ms(0));
    let typed = s.panel().text().to_owned();
    s.save(|_| Ok(())).unwrap();
    s.disk_changed(Some(TWO.replace("Gates", "Posts").as_bytes()));
    settle(&mut s);
    assert_eq!(shape_text(&s, "a"), "Posts");
    undo(&mut s, ms(300));
    assert_eq!(s.panel().text(), typed);
    assert_eq!(invalid_rows(&s), [4]);
    // The file on disk is the reloaded one now.
    assert!(!s.is_saved());
}

#[test]
fn undoing_typing_back_to_an_invalid_files_text_is_saved_again() {
    let invalid = TWO.replace("below a", "below azz");
    let mut s = panel_of(&invalid);
    s.handle(Input::TogglePanel, ms(0));
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    pause(&mut s, ms(150));
    assert!(!s.is_saved());
    undo(&mut s, ms(200));
    assert_eq!(s.panel().text(), invalid);
    assert!(s.is_saved());
}

#[test]
fn typing_around_a_paste_within_the_pause_is_three_steps() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Gates", ms(0));
    type_in(&mut s, "!", ms(0));
    s.handle(Input::Paste("X".into()), ms(50));
    type_in(&mut s, "?", ms(100));
    pause(&mut s, ms(250));
    let mut seen = vec![shape_text(&s, "a")];
    for n in 0..3 {
        undo(&mut s, ms(300 + n * 100));
        seen.push(shape_text(&s, "a"));
    }
    assert_eq!(seen, ["Gates!X?", "Gates!X", "Gates!", "Gates"]);
    assert!(!s.can_undo());
}

#[test]
fn typing_on_two_lines_within_the_pause_is_two_steps() {
    let mut s = panel_of(TWO);
    caret_after(&mut s, "Fences", ms(0));
    type_in(&mut s, "?", ms(0));
    // Up to the end of `a`'s line, then back inside its quotes.
    s.handle(Input::Move(Motion::Up), ms(50));
    s.handle(Input::Move(Motion::LineEnd), ms(50));
    s.handle(Input::Move(Motion::Left), ms(50));
    type_in(&mut s, "!", ms(100));
    pause(&mut s, ms(250));
    assert_eq!(texts(&s), pair("Gates!", "Fences?"));
    undo(&mut s, ms(300));
    assert_eq!(texts(&s), pair("Gates", "Fences?"));
    undo(&mut s, ms(400));
    assert_eq!(texts(&s), pair("Gates", "Fences"));
    assert!(!s.can_undo());
}
