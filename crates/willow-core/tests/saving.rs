mod common;

use std::io;
use std::time::Duration;

use willow_core::{Document, Input, PAPER, Session, load};

fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake))
}

const FILE: &[u8] = b"willow 1\n\na: \"Gates\"\n\n@layout a 0,0 w210\n";

#[test]
fn edits_make_a_session_unsaved_and_a_save_makes_it_saved() {
    let mut s = common::session_of(Document::new());
    assert!(s.is_saved());
    let a = s.document().shapes[0].id.clone();

    s.move_shape(&a, 10, 0);
    assert!(!s.is_saved());
    s.handle(Input::Undo, Duration::ZERO);
    assert!(s.is_saved());
    s.handle(Input::Redo, Duration::ZERO);
    assert!(!s.is_saved());

    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    assert_eq!(written, b"willow 1\n\na: \"\"\n\n@layout a 10,0 w210\n");
    assert!(s.is_saved());
    assert_eq!(s.bytes(), written);
}

#[test]
fn an_opened_file_is_saved_and_remembers_the_bytes_read() {
    let s = open(FILE);
    assert!(s.is_saved());
    assert_eq!(s.bytes(), FILE);
}

#[test]
fn a_file_that_needed_repairs_opens_unsaved() {
    // No layout for `a`.
    assert!(!open(b"willow 1\n\na: \"Gates\"\n").is_saved());
}

#[test]
fn a_failed_save_changes_nothing() {
    let mut s = open(FILE);
    let a = s.document().shapes[0].id.clone();
    s.move_shape(&a, 10, 0);

    let err = s.save(|_| Err(io::Error::from(io::ErrorKind::PermissionDenied)));
    assert_eq!(err.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    assert!(!s.is_saved());
    assert_eq!(s.bytes(), FILE);
    assert_eq!(s.document().shapes[0].x, 10);
}
