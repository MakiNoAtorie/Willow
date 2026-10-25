mod common;

use std::time::Duration;

use willow_core::{Input, PAPER, Session, load};

fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake))
}

fn text(s: &Session) -> String {
    s.document().shapes[0].text.plain()
}

const FILE: &[u8] = b"willow 1\n\na: \"Gates\"\n\n@layout a 0,0 w210\n";
const THEIRS: &[u8] = b"willow 1\n\na: \"Fences\"\n\n@layout a 0,0 w210\n";

#[test]
fn bytes_equal_to_the_last_read_change_nothing() {
    let mut s = open(FILE);
    s.disk_changed(Some(FILE));
    assert!(s.is_saved());
    assert!(!s.can_undo());
    assert_eq!(text(&s), "Gates");
}

#[test]
fn its_own_save_never_reloads() {
    let mut s = open(FILE);
    let a = s.document().shapes[0].id.clone();
    s.move_shape(&a, 10, 0);
    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    s.disk_changed(Some(&written));
    assert!(s.is_saved());
    assert_eq!(s.document().shapes[0].x, 10);
}

#[test]
fn a_change_with_nothing_unsaved_reloads_as_one_undo_step() {
    let mut s = open(FILE);
    s.disk_changed(Some(THEIRS));
    assert_eq!(text(&s), "Fences");
    assert!(s.is_saved());
    assert_eq!(s.bytes(), THEIRS);

    s.handle(Input::Undo, Duration::ZERO);
    assert_eq!(text(&s), "Gates");
    assert!(!s.can_undo());
    assert!(!s.is_saved());
    s.handle(Input::Redo, Duration::ZERO);
    assert_eq!(text(&s), "Fences");
    assert!(s.is_saved());
}

#[test]
fn a_change_with_unsaved_edits_keeps_this_version_unsaved() {
    let mut s = open(FILE);
    let a = s.document().shapes[0].id.clone();
    s.move_shape(&a, 10, 0);
    s.disk_changed(Some(THEIRS));
    assert_eq!(text(&s), "Gates");
    assert_eq!(s.document().shapes[0].x, 10);
    // Undoing the edit doesn't make it match the disk.
    s.handle(Input::Undo, Duration::ZERO);
    assert!(!s.is_saved());
}

#[test]
fn a_missing_file_leaves_the_document_open_and_unsaved() {
    let mut s = open(FILE);
    s.disk_changed(None);
    assert_eq!(text(&s), "Gates");
    assert!(!s.is_saved());
    // The next save writes it again.
    s.save(|_| Ok(())).unwrap();
    assert!(s.is_saved());
}

#[test]
fn unreadable_bytes_keep_this_version_unsaved() {
    let mut s = open(FILE);
    s.disk_changed(Some(b"not willow"));
    assert_eq!(text(&s), "Gates");
    assert!(!s.is_saved());
}

#[test]
fn a_reload_with_the_same_document_in_other_bytes_adds_no_undo_step_and_keeps_editing() {
    let mut s = open(FILE);
    let a = s.document().shapes[0].id.clone();
    s.select([a]);
    s.handle(Input::Enter, Duration::ZERO);
    assert!(s.editing().is_some());
    let crlf = b"willow 1\r\n\r\na: \"Gates\"\r\n\r\n@layout a 0,0 w210\r\n";
    s.disk_changed(Some(crlf));
    assert!(s.editing().is_some());
    assert!(!s.can_undo());
    assert!(s.is_saved());
    assert_eq!(s.bytes(), crlf);
}

#[test]
fn a_reload_ends_a_held_gesture() {
    use willow_core::{Button, Modifiers, Point};
    let mut s = open(
        b"willow 1\n\na: \"Gates\"\nb: \"Walls\"\n\n@layout a 0,0 w210\n@layout b 0,140 w210\n",
    );
    let (button, mods) = (Button::Left, Modifiers::default());
    let pos = s.viewport().to_screen(Point { x: 100.0, y: 160.0 });
    s.handle(Input::PointerDown { pos, button, mods }, Duration::ZERO);
    s.disk_changed(Some(THEIRS));
    let pos = s.viewport().to_screen(Point { x: 300.0, y: 300.0 });
    s.handle(Input::PointerMove { pos, mods }, Duration::ZERO);
    s.handle(Input::PointerUp { pos, button, mods }, Duration::ZERO);
    assert_eq!(s.document().shapes.len(), 1);
    assert_eq!((s.document().shapes[0].x, s.document().shapes[0].y), (0, 0));
}
