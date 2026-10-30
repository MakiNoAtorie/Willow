mod common;

use std::time::Duration;

use willow_core::{Input, PAPER, Recovery, Session, ShapeId, load};

fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake))
}

fn ids(s: &Session) -> Vec<&str> {
    (s.document().shapes.iter())
        .map(|s| s.id.0.as_str())
        .collect()
}

/// `b` links to a missing ID; `c` has no layout.
const BAD: &[u8] =
    b"willow 1\r\n\r\na: \"Gates\"\r\nb: \"Fences\" below zz\r\nc: \"Walls\" below a\r\n\r\n@layout a 0,0 w210\r\n@layout b 0,140 w210\r\n";

fn saved(s: &mut Session) -> Vec<u8> {
    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    written
}

#[test]
fn an_invalid_file_opens_in_the_panel_as_it_is_with_what_parses() {
    let mut s = open(BAD);
    assert!(s.panel().open);
    assert_eq!(s.panel().text().as_bytes(), BAD);
    assert!(s.is_saved());
    assert_eq!(ids(&s), ["a", "b", "c"]);
    let a = ShapeId("a".into());
    assert_eq!(
        s.document().links,
        [willow_core::Link(a, ShapeId("c".into()))]
    );
    // `c` is placed one creation gap below `a`.
    assert!(s.document().shapes[2].y > 0);
    // Nothing regenerates the text away.
    assert!(s.take_regeneration().is_none());
    assert_eq!(saved(&mut s), BAD);
    assert!(s.is_saved());
}

const FILE: &[u8] = b"willow 1\n\na: \"Gates\"\n\n@layout a 0,0 w210\n";

#[test]
fn an_invalid_change_on_disk_loads_the_same_way_as_one_undo_step() {
    let mut s = open(FILE);
    s.disk_changed(Some(BAD));
    assert!(s.panel().open);
    assert_eq!(s.panel().text().as_bytes(), BAD);
    assert!(s.is_saved());
    assert_eq!(ids(&s), ["a", "b", "c"]);
    assert!(s.take_regeneration().is_none());
    assert_eq!(saved(&mut s), BAD);
    s.handle(Input::Undo, Duration::ZERO);
    assert_eq!(ids(&s), ["a"]);
}

#[test]
fn a_valid_change_on_disk_lets_go_of_the_held_text_even_with_the_same_document() {
    let mut s = open(b"willow 1\n\na: \"Gates\"\n}\n\n@layout a 0,0 w210\n");
    s.disk_changed(Some(FILE));
    assert!(s.is_saved() && !s.can_undo());
    let job = s.take_regeneration().expect("the text regenerates");
    s.regenerated(job.run());
    assert_eq!(s.panel().text().as_bytes(), FILE);
    assert_eq!(saved(&mut s), FILE);
}

#[test]
fn a_recovery_file_writes_the_held_text() {
    let mut s = open(BAD);
    // Deleted on disk: unsaved, with a recovery due.
    s.disk_changed(None);
    let due = Duration::from_secs(5);
    let text = String::from_utf8(BAD.to_vec()).unwrap();
    assert_eq!(s.take_recovery(due), Some(Recovery::Text(text)));
}

#[test]
fn an_invalid_recovery_reopens_the_same_way_marked_unsaved() {
    let s = Session::recover(load(BAD).unwrap(), BAD, &PAPER, Box::new(common::Fake));
    assert!(s.panel().open);
    assert_eq!(s.panel().text().as_bytes(), BAD);
    assert!(!s.is_saved());
    assert_eq!(ids(&s), ["a", "b", "c"]);
    // Nothing read from the document's file yet: any bytes there count as a change.
    assert_eq!(s.bytes(), b"");
}

#[test]
fn invalid_rows_are_marked_and_ignored_lines_drawn_in_the_layout_gray() {
    let mut s = open(b"willow 1\n\na: \"A\" below zz\nnonsense \"here\"\n\n@layout a 0,0 w210\n");
    s.resize(1000.0, 800.0);
    let rows = s.panel_rows();
    let invalid: Vec<bool> = rows.iter().map(|r| r.invalid).collect();
    assert_eq!(invalid, [false, false, true, false, false, false, false]);
    let gray = (rows[3].spans.iter()).all(|sp| sp.color == PAPER.code.layout && !sp.bold);
    assert!(gray && rows[3].spans.iter().map(|sp| sp.range.len()).sum::<usize>() == 15);
    // The invalid line keeps its syntax colors.
    assert_eq!(rows[2].spans[0].color, PAPER.code.id);
}

#[test]
fn closing_and_reopening_the_panel_keeps_the_held_text_until_the_document_changes() {
    let mut s = open(BAD);
    s.handle(Input::TogglePanel, Duration::ZERO);
    s.handle(Input::TogglePanel, Duration::ZERO);
    assert!(s.take_regeneration().is_none());
    assert_eq!(s.panel().text().as_bytes(), BAD);
    // A canvas edit works on the held model and regenerates from it.
    s.handle(Input::Escape, Duration::ZERO);
    s.select([ShapeId("a".into())]);
    s.handle(Input::Nudge(willow_core::Side::Right), Duration::ZERO);
    let job = s.take_regeneration().expect("the text regenerates");
    s.regenerated(job.run());
    assert!(!s.panel().text().contains("zz"));
    assert!(s.panel_rows().iter().all(|r| !r.invalid));
}

#[test]
fn a_valid_recovery_reopens_marked_unsaved() {
    let s = Session::recover(load(FILE).unwrap(), FILE, &PAPER, Box::new(common::Fake));
    assert!(!s.panel().open && !s.is_saved());
}
