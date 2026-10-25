mod common;

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use common::{doc_with, session_of};
use willow_core::{
    Button, Input, LaunchPlan, Modifiers, PAPER, Point, Recovery, RecoveryFile, Reopen, Session,
    launch_plan, load,
};

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// The text's top-left on screen: the centered shape at (395, 376), plus padding.
const TEXT: Point = Point { x: 411.0, y: 389.0 };

/// A session with its shape selected, so typing edits it.
fn session() -> Session {
    let mut s = session_of(doc_with("ab"));
    let (pos, mods) = (TEXT, Modifiers::default());
    let button = Button::Left;
    s.handle(Input::PointerDown { pos, button, mods }, Duration::ZERO);
    s.handle(Input::PointerUp { pos, button, mods }, Duration::ZERO);
    s
}

fn typ(s: &mut Session, text: &str, now: Duration) {
    let (text, replace) = (text.into(), None);
    s.handle(Input::Text { text, replace }, now);
}

#[test]
fn a_recovery_is_due_five_seconds_after_the_last_change() {
    let mut s = session();
    assert_eq!(s.recovery_wake(sec(1)), None);
    typ(&mut s, "x", sec(10));
    assert_eq!(s.recovery_wake(sec(10)), Some(sec(5)));
    // Too early: nothing to write yet.
    assert_eq!(s.take_recovery(sec(12)), None);
    typ(&mut s, "y", sec(12));
    assert_eq!(s.recovery_wake(sec(13)), Some(sec(4)));

    let Some(Recovery::Write { document: doc, .. }) = s.take_recovery(sec(17)) else {
        panic!("a write due at 17 s");
    };
    assert_eq!(doc.shapes[0].text.plain(), "abxy");
    // Written: nothing more until the next change.
    assert_eq!(s.recovery_wake(sec(17)), None);
    assert_eq!(s.take_recovery(sec(30)), None);
}

#[test]
fn changes_that_keep_coming_are_recovered_every_thirty_seconds() {
    let mut s = session();
    // A change every 2 s from 10 s on: never 5 s quiet.
    for t in (10..=40).step_by(2) {
        typ(&mut s, "x", sec(t));
    }
    assert_eq!(s.recovery_wake(sec(40)), Some(Duration::ZERO));
    assert!(s.take_recovery(sec(40)).is_some());
    // The next 30 s run from the first change after the write.
    for t in (42..=70).step_by(2) {
        typ(&mut s, "x", sec(t));
    }
    assert_eq!(s.recovery_wake(sec(70)), Some(sec(2)));
    assert_eq!(s.take_recovery(sec(71)), None);
    assert!(s.take_recovery(sec(72)).is_some());
}

#[test]
fn nothing_is_due_while_saved() {
    let mut s = session();
    typ(&mut s, "x", sec(10));
    s.handle(Input::Escape, sec(10));
    s.save(|_| Ok(())).unwrap();
    assert_eq!(s.recovery_wake(sec(10)), None);
    assert_eq!(s.take_recovery(sec(60)), None);

    // Undo back to the saved document: saved again, nothing to recover.
    typ(&mut s, "y", sec(61));
    s.handle(Input::Undo, sec(62));
    assert!(s.is_saved());
    assert_eq!(s.recovery_wake(sec(62)), None);
    assert_eq!(s.take_recovery(sec(90)), None);
    // The next change starts afresh.
    s.handle(Input::Redo, sec(200));
    assert_eq!(s.recovery_wake(sec(200)), Some(sec(5)));
}

#[test]
fn undoing_back_to_saved_after_a_recovery_write_deletes_it() {
    let mut s = session();
    typ(&mut s, "x", sec(10));
    assert!(matches!(
        s.take_recovery(sec(15)),
        Some(Recovery::Write { .. })
    ));
    s.handle(Input::Undo, sec(20));
    assert!(s.is_saved());
    assert_eq!(s.recovery_wake(sec(20)), Some(sec(5)));
    assert_eq!(s.take_recovery(sec(25)), Some(Recovery::Delete));
    assert_eq!(s.recovery_wake(sec(25)), None);
}

#[test]
fn a_file_deleted_on_disk_makes_a_recovery_due() {
    let bytes = b"willow 1\n\na: \"A\"\n\n@layout a 0,0 w210\n";
    let loaded = load(bytes).unwrap();
    let mut s = Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(common::Fake));
    s.disk_changed(None);
    assert_eq!(s.recovery_wake(sec(0)), Some(sec(5)));
    assert!(matches!(
        s.take_recovery(sec(5)),
        Some(Recovery::Write { .. })
    ));
}

#[test]
fn a_failed_save_leaves_the_recovery_due() {
    let mut s = session();
    typ(&mut s, "x", sec(10));
    let failed = s.save(|_| Err(std::io::ErrorKind::PermissionDenied.into()));
    assert!(failed.is_err());
    assert_eq!(s.recovery_wake(sec(10)), Some(sec(5)));
}

fn at(n: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + sec(n)
}

fn entry(id: &str, path: Option<&str>, recovered: u64, document: Option<u64>) -> RecoveryFile {
    RecoveryFile {
        id: id.into(),
        path: path.map(PathBuf::from),
        recovered_at: at(recovered),
        document_modified: document.map(at),
    }
}

fn reopen(id: &str, path: Option<&str>) -> Reopen {
    Reopen {
        id: id.into(),
        path: path.map(PathBuf::from),
    }
}

#[test]
fn launch_opens_recoveries_newer_than_their_document_and_deletes_older_ones() {
    let plan = launch_plan(vec![
        entry("newer", Some("/a.willow"), 20, Some(10)),
        entry("older", Some("/b.willow"), 10, Some(20)),
        entry("gone", Some("/c.willow"), 10, None),
        entry("untitled", None, 10, None),
    ]);
    assert_eq!(
        plan,
        LaunchPlan {
            open: vec![
                reopen("newer", Some("/a.willow")),
                reopen("gone", None),
                reopen("untitled", None),
            ],
            delete: vec!["older".into()],
            new_document: false,
        }
    );
}

#[test]
fn launch_with_nothing_to_recover_opens_one_new_document() {
    let plan = launch_plan(vec![]);
    assert_eq!(plan.open, vec![]);
    assert!(plan.new_document);

    let plan = launch_plan(vec![entry("older", Some("/b.willow"), 10, Some(20))]);
    assert_eq!(plan.delete, vec!["older".to_string()]);
    assert!(plan.new_document);
}
