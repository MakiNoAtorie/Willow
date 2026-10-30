//! Changing only a link's direction word in the code panel re-places the shape one creation
//! gap from its anchor on the new side, with the part of its chain beyond it, in the same
//! undo step as the text edit.

mod common;

use std::time::Duration;
use willow_core::{Input, Session};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What the app does after every input: a due regeneration, run and handed back.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        s.regenerated(job.run());
    }
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

/// Types `to` (one line) over the first `from` in the panel text and lets the check and
/// apply run.
fn edit(s: &mut Session, from: &str, to: &str, now: Duration) {
    let at = s.panel().text().find(from).expect("in the text");
    let (text, replace) = (to.to_owned(), Some(at..at + from.len()));
    s.handle(Input::Text { text, replace }, now);
    settle(s);
    if let Some(job) = s.take_text_check(now + ms(150)) {
        s.text_checked(job.run());
    }
    settle(s);
}

fn spot(s: &Session, id: &str) -> (i32, i32) {
    let shape = (s.document().shapes.iter()).find(|s| s.id.0 == id);
    let shape = shape.expect("the shape is there");
    (shape.x, shape.y)
}

/// `a` with `b` below it and `c` below `b`, one chain.
const CHAIN: &str = "willow 1\n\na: \"A\"\nb: \"B\" below a\nc: \"C\" below b\n\n\
@layout a 0,0 w210\n@layout b 0,200 w210\n@layout c 0,400 w210\n";

#[test]
fn each_direction_word_re_places_the_shape_a_gap_away_and_carries_its_far_chain() {
    let height = {
        let s = panel_of(CHAIN);
        s.shape_height(&s.document().shapes[0]).round() as i32
    };
    // `a` is at 0,0 and 210 wide; `b` is the same size, centered on `a`'s side midpoint.
    let cases = [
        ("above a", (0, -height - 48)),
        ("right of a", (210 + 48, 0)),
        ("left of a", (-210 - 48, 0)),
    ];
    for (word, to) in cases {
        let mut s = panel_of(CHAIN);
        let (b, c) = (spot(&s, "b"), spot(&s, "c"));
        edit(&mut s, "below a", word, ms(1000));
        assert_eq!(spot(&s, "b"), to, "{word}");
        let (dx, dy) = (to.0 - b.0, to.1 - b.1);
        assert_eq!(spot(&s, "c"), (c.0 + dx, c.1 + dy), "{word}: far chain");
        assert_eq!(spot(&s, "a"), (0, 0), "{word}: anchor");
    }
    // From beside, back below: one gap under `a`.
    let beside = CHAIN.replace("0,200", "258,0").replace("0,400", "258,200");
    let mut s = panel_of(&beside.replace("below a", "right of a"));
    edit(&mut s, "right of a", "below a", ms(1000));
    assert_eq!(spot(&s, "b"), (0, height + 48), "below a");
    assert_eq!(spot(&s, "c"), (0, height + 48 + 200), "below a: far chain");
}

#[test]
fn a_changed_layout_line_wins_and_an_unchanged_word_moves_nothing() {
    // Word and layout typed together: the layout places it, and nothing is carried.
    let mut s = panel_of(CHAIN);
    let c = spot(&s, "c");
    let text = CHAIN
        .replace("below a", "right of a")
        .replace("0,200", "70,90");
    s.handle(Input::SelectAll, ms(1000));
    s.handle(Input::Paste(text), ms(1000));
    settle(&mut s);
    if let Some(job) = s.take_text_check(ms(1150)) {
        s.text_checked(job.run());
    }
    assert_eq!(spot(&s, "b"), (70, 90));
    assert_eq!(spot(&s, "c"), c);
    // The word stays and something else changes: positions stay.
    let mut s = panel_of(CHAIN);
    let before = ["a", "b", "c"].map(|i| spot(&s, i));
    edit(&mut s, "\"B\"", "\"Bee\"", ms(1000));
    assert_eq!(["a", "b", "c"].map(|i| spot(&s, i)), before);
}

#[test]
fn one_undo_restores_the_old_word_and_positions() {
    let mut s = panel_of(CHAIN);
    let before = s.document().clone();
    edit(&mut s, "below a", "right of a", ms(1000));
    assert_ne!(*s.document(), before);
    s.handle(Input::Undo, ms(2000));
    assert_eq!(*s.document(), before);
    settle(&mut s);
    assert!(s.panel().text().contains("b: \"B\" below a"));
}

#[test]
fn a_word_the_writer_left_unchanged_moves_nothing_though_the_layout_disagrees() {
    let mut s = panel_of(CHAIN);
    // `b`'s layout puts it right of `a`; its word still says below.
    edit(&mut s, "@layout b 0,200", "@layout b 258,0", ms(1000));
    assert_eq!(spot(&s, "b"), (258, 0));
    // Something else changes: the word is as it was, so `b` stays.
    edit(&mut s, "\"C\"", "\"Cee\"", ms(3000));
    assert_eq!(spot(&s, "b"), (258, 0));
    // Changing the word now re-places it.
    edit(&mut s, "below a", "above a", ms(5000));
    assert_ne!(spot(&s, "b"), (258, 0));
}

#[test]
fn an_invalid_files_words_are_the_ones_compared() {
    // As read: `b` sits right of `a` but says below; the last line is invalid.
    let file = CHAIN.replace("@layout b 0,200", "@layout b 258,0") + "@layout zz\n";
    let loaded = willow_core::load(file.as_bytes()).unwrap();
    assert!(loaded.invalid());
    let mut s = Session::open(
        loaded,
        file.clone().into(),
        &willow_core::PAPER,
        Box::new(common::Fake),
    );
    s.resize(1000.0, 800.0);
    s.handle(Input::FocusPanel, ms(0));
    edit(&mut s, "@layout zz\n", "", ms(1000));
    assert_eq!(spot(&s, "b"), (258, 0));
}
