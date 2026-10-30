//! Text edits in the code panel reach the canvas: after a pause, on leaving the panel, on
//! Save and before canvas edits; never while a line is invalid.

mod common;

use std::time::Duration;
use willow_core::{Document, Input, Link, Session, ShapeId};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What the app does after every input: runs a due regeneration (off the UI thread there)
/// and hands the text back.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        s.regenerated(job.run());
    }
}

/// A session over `doc` with the panel open, focused and regenerated.
fn panel_on(doc: Document) -> Session {
    let mut s = common::session_of(doc);
    s.handle(Input::TogglePanel, ms(0));
    settle(&mut s);
    s
}

/// Types `to` over the first `from` in the panel text at `now`, line breaks as Enter. (A
/// paste would apply at once.)
fn replace(s: &mut Session, from: &str, to: &str, now: Duration) {
    let at = s.panel().text().find(from).expect("in the text");
    let mut over = at..at + from.len();
    for (i, line) in to.split('\n').enumerate() {
        if i > 0 {
            s.handle(Input::Enter, now);
            // Over the indent Enter carried down.
            let c = s.panel().caret();
            over = s.panel().text()[..c].rfind('\n').map_or(0, |n| n + 1)..c;
        }
        let (text, replace) = (line.to_owned(), Some(over.clone()));
        s.handle(Input::Text { text, replace }, now);
    }
    settle(s);
}

/// What the app does when the text check's wake comes: checks off the UI thread, hands the
/// result back.
fn pause(s: &mut Session, now: Duration) {
    if let Some(job) = s.take_text_check(now) {
        s.text_checked(job.run());
    }
    settle(s);
}

fn ids(s: &Session) -> Vec<&str> {
    (s.document().shapes.iter())
        .map(|s| s.id.0.as_str())
        .collect()
}

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

#[test]
fn a_typed_shape_line_reaches_the_canvas_after_the_pause() {
    let mut s = panel_on(Document::new());
    let t = ms(1000);
    replace(&mut s, "a: \"\"\n", "a: \"\"\nb: \"Fences\" below a\n", t);
    assert_eq!(s.text_wake(t), Some(ms(150)));
    // Each keystroke restarts the wait.
    replace(&mut s, "Fences", "Gates", t + ms(100));
    pause(&mut s, t + ms(150));
    assert_eq!(ids(&s), ["a"]);
    assert_eq!(s.text_wake(t + ms(150)), Some(ms(100)));
    pause(&mut s, t + ms(250));
    assert_eq!(ids(&s), ["a", "b"]);
    assert_eq!(s.document().shapes[1].text, common::text("Gates"));
    assert_eq!(s.document().links, [Link(id("a"), id("b"))]);
    // Nothing more is due, and the text stays as typed.
    assert_eq!(s.text_wake(t + ms(250)), None);
    assert!(
        s.panel()
            .text()
            .contains("b: \"Gates\" below a\n\n@layout a")
    );
}

/// `a` drawn under `b`, which sits above it, linked and listed in their zone otherwise than
/// the text reads, `a`'s text written unlike the loader would (an empty run).
fn stacked() -> Document {
    let mut doc = common::doc_with("");
    let mut b = willow_core::Shape::new(id("b"), 0, -200);
    b.text = common::text("Top");
    doc.shapes.push(b);
    doc.links.push(Link(id("a"), id("b")));
    doc.zones.push(willow_core::Zone {
        id: id("z"),
        title: "Zone".into(),
        color: Default::default(),
        x: -40,
        y: -260,
        width: 300,
        height: 400,
        members: vec![id("a"), id("b")],
    });
    doc
}

#[test]
fn text_that_says_the_same_changes_nothing_and_adds_no_undo_step() {
    let mut s = panel_on(stacked());
    let before = s.document().clone();
    replace(&mut s, "a: \"\"", "a: \"x\"", ms(0));
    replace(&mut s, "a: \"x\"", "a: \"\"", ms(10));
    pause(&mut s, ms(160));
    assert_eq!(*s.document(), before);
    assert!(!s.can_undo());
}

#[test]
fn ids_stay_put_in_draw_order_and_the_selection() {
    let mut s = panel_on(stacked());
    s.select([id("a"), id("b")]);
    replace(&mut s, "\"Top\"", "\"Top shape\"", ms(0));
    pause(&mut s, ms(150));
    assert_eq!(ids(&s), ["a", "b"]);
    assert_eq!(s.document().shapes[1].text, common::text("Top shape"));
    // The checked text selects the shape whose line the caret is on.
    assert_eq!(s.selection(), [id("b")]);
    assert_eq!(s.document().links, [Link(id("a"), id("b"))]);
    assert_eq!(s.document().zones[0].members, [id("a"), id("b")]);
    // A changed ID token is a new shape: the line naming the old ID is invalid then.
    replace(&mut s, "b: \"Top shape\"", "c: \"Top shape\"", ms(200));
    pause(&mut s, ms(350));
    assert_eq!(ids(&s), ["a", "b"]);
    // `c` isn't a shape yet: the selection stays.
    assert_eq!(s.selection(), [id("b")]);
    replace(&mut s, "below b", "below c", ms(400));
    pause(&mut s, ms(550));
    // The new shape goes on top; the old one is gone, and the caret's line is `a`'s.
    assert_eq!(ids(&s), ["a", "c"]);
    assert_eq!(s.selection(), [id("a")]);
    assert_eq!(s.document().links, [Link(id("c"), id("a"))]);
}

/// Line numbers (from 1) of the rows drawn invalid.
fn invalid_rows(s: &Session) -> Vec<usize> {
    (s.panel_rows().into_iter())
        .filter(|r| r.invalid)
        .filter_map(|r| r.number)
        .collect()
}

#[test]
fn nothing_applies_while_a_line_is_invalid_and_the_underlines_follow_the_text() {
    let mut s = panel_on(Document::new());
    let before = s.document().clone();
    replace(
        &mut s,
        "a: \"\"\n",
        "a: \"Gates\"\nb: \"Fences\" below zz\n",
        ms(0),
    );
    pause(&mut s, ms(150));
    assert_eq!(*s.document(), before);
    assert_eq!(invalid_rows(&s), [4]);
    // A line moves down: its underline goes with it.
    replace(&mut s, "a: \"Gates\"\n", "\na: \"Gates\"\n", ms(200));
    pause(&mut s, ms(350));
    assert_eq!(invalid_rows(&s), [5]);
    assert_eq!(*s.document(), before);
    replace(&mut s, "below zz", "below a", ms(400));
    pause(&mut s, ms(550));
    assert_eq!(invalid_rows(&s), Vec::<usize>::new());
    assert_eq!(ids(&s), ["a", "b"]);
    assert_eq!(s.document().shapes[0].text, common::text("Gates"));
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

/// Where shape `id` is, as (x, y, width).
fn spot(s: &Session, id: &str) -> (i32, i32, u32) {
    let shape = (s.document().shapes.iter()).find(|s| s.id.0 == id);
    let shape = shape.expect("the shape is there");
    (shape.x, shape.y, shape.width)
}

/// Replaces the whole panel text with `text` at `now`, as pasted over a select-all.
fn retype(s: &mut Session, text: &str, now: Duration) {
    s.handle(Input::SelectAll, now);
    s.handle(Input::Paste(text.into()), now);
    settle(s);
}

#[test]
fn shapes_without_layout_are_placed_as_a_loaded_file_places_them_and_then_stay() {
    let mut s = panel_on(Document::new());
    let text = "willow 1\n\na: \"A\"\nb: \"B\" below a\nc: \"C\"\n\n@layout a 0,0 w210\n";
    retype(&mut s, text, ms(0));
    pause(&mut s, ms(150));
    let loader = panel_of(text);
    assert_eq!(ids(&s), ["a", "b", "c"]);
    for id in ["b", "c"] {
        assert_eq!(spot(&s, id), spot(&loader, id), "{id}");
    }
    // Still without layout, a shape the document has keeps its place.
    let b = spot(&s, "b");
    replace(&mut s, "@layout a 0,0", "@layout a 500,0", ms(200));
    replace(&mut s, "\"B\"", "\"Bee\"", ms(200));
    pause(&mut s, ms(350));
    assert_eq!(spot(&s, "a"), (500, 0, 210));
    assert_eq!(spot(&s, "b"), b);
}

const ZONED: &str = "willow 1\n\nzone z \"Zone\" {\n  a: \"A\"\n}\nc: \"C\"\nd: \"D\"\n\n\
@layout a 0,0 w210\n@layout c 0,100 w210\n@layout d 20,20 w100\n@layout z -40,-40 300x200\n";

#[test]
fn zone_blocks_alone_make_members_and_nothing_pushes_or_grows() {
    let mut s = panel_of(ZONED);
    let zone = s.document().zones[0].clone();
    let (a, c) = (spot(&s, "a"), spot(&s, "c"));
    // `d` sits inside the zone but its line is outside the block; `e` lands on `c`.
    let text = ZONED.replace("  a: \"A\"\n", "  a: \"A\"\n  e: \"E\" below a\n");
    retype(&mut s, &text, ms(0));
    pause(&mut s, ms(150));
    assert_eq!(ids(&s), ["a", "c", "d", "e"]);
    assert_eq!(spot(&s, "e"), spot(&panel_of(&text), "e"));
    assert_eq!((spot(&s, "a"), spot(&s, "c")), (a, c));
    let z = &s.document().zones[0];
    let rect = (z.x, z.y, z.width, z.height);
    assert_eq!(rect, (zone.x, zone.y, zone.width, zone.height));
    assert_eq!(z.members, [id("a"), id("e")]);
}

/// Canonical text for the document as it is now.
fn canonical(s: &Session) -> String {
    willow_core::save(s.document(), &s.heights())
}

#[test]
fn leaving_the_panel_applies_pending_text_at_once_then_regenerates_it() {
    let mut s = panel_on(Document::new());
    replace(&mut s, "a: \"\"\n", "a: \"Gates\"\nb: \"B\"\n", ms(0));
    s.handle(Input::Escape, ms(10));
    assert!(!s.panel().focused);
    assert_eq!(ids(&s), ["a", "b"]);
    // Canonical text: the new shape gets its `@layout` line.
    settle(&mut s);
    assert_eq!(s.panel().text(), canonical(&s));
    assert!(s.panel().text().contains("@layout b"));
    // Nothing is left to check.
    assert_eq!(s.text_wake(ms(10)), None);
}

#[test]
fn a_click_on_the_canvas_applies_pending_text_before_it_acts() {
    let mut s = panel_on(Document::new());
    replace(&mut s, "a: \"\"\n", "a: \"\"\nb: \"B\" below a\n", ms(0));
    // A click on `b`, which only the pending text has.
    let mut placed = panel_on(Document::new());
    replace(
        &mut placed,
        "a: \"\"\n",
        "a: \"\"\nb: \"B\" below a\n",
        ms(0),
    );
    pause(&mut placed, ms(150));
    let (x, y, _) = spot(&placed, "b");
    let pos = s.viewport().to_screen(willow_core::Point {
        x: x as f32 + 20.0,
        y: y as f32 + 10.0,
    });
    let (button, mods) = (willow_core::Button::Left, Default::default());
    s.handle(Input::PointerDown { pos, button, mods }, ms(10));
    s.handle(Input::PointerUp { pos, button, mods }, ms(20));
    assert_eq!(s.selection(), [id("b")]);
}

fn saved(s: &mut Session) -> String {
    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    String::from_utf8(written).unwrap()
}

const SIMPLE: &str = "willow 1\n\na: \"Gates\"\n\n@layout a 0,0 w210\n";

#[test]
fn pending_and_applied_text_is_unsaved_and_save_applies_it_and_writes_it_as_typed() {
    let mut s = panel_of(SIMPLE);
    assert!(s.is_saved());
    // Pending: not yet checked.
    replace(&mut s, "\"Gates\"\n", "\"Fences\"\nb: \"B\"\n", ms(0));
    assert!(!s.is_saved());
    assert!(s.recovery_wake(ms(0)).is_some());
    let text = s.panel().text().to_owned();
    assert_eq!(saved(&mut s), text);
    assert!(s.is_saved());
    assert_eq!(ids(&s), ["a", "b"]);
    // Applied after the pause.
    replace(&mut s, "\"B\"", "\"Bee\"", ms(100));
    pause(&mut s, ms(250));
    assert_eq!(s.document().shapes[1].text, common::text("Bee"));
    assert!(!s.is_saved());
    saved(&mut s);
    // Invalid text doesn't reach the document, but is unsaved all the same.
    replace(&mut s, "\"Bee\"", "\"Bee\" below zz", ms(300));
    pause(&mut s, ms(450));
    assert!(!s.is_saved());
    assert!(saved(&mut s).contains("below zz"));
    assert!(s.is_saved());
}

#[test]
fn undo_in_the_panel_applies_pending_text_first_then_takes_it_back_as_one_step() {
    let mut s = panel_of(SIMPLE);
    let before = s.document().clone();
    replace(&mut s, "\"Gates\"\n", "\"Fences\"\nb: \"B\"\n", ms(0));
    let typed = s.panel().text().to_owned();
    s.handle(Input::Undo, ms(10));
    settle(&mut s);
    assert_eq!(*s.document(), before);
    assert_eq!(s.panel().text(), canonical(&s));
    assert!(s.panel().focused);
    s.handle(Input::Redo, ms(20));
    settle(&mut s);
    assert_eq!(ids(&s), ["a", "b"]);
    // Never regenerated, the text comes back as typed.
    assert_eq!(s.panel().text(), typed);
}

#[test]
fn a_canvas_edit_while_a_line_is_invalid_works_on_the_held_document_and_regenerates() {
    let mut s = panel_of(SIMPLE);
    replace(
        &mut s,
        "\"Gates\"\n",
        "\"Fences\"\nb: \"B\" below zz\n",
        ms(0),
    );
    s.handle(Input::Escape, ms(10));
    settle(&mut s);
    // Invalid text stays as typed when focus leaves.
    assert!(s.panel().text().contains("below zz"));
    assert_eq!(ids(&s), ["a"]);
    s.select([id("a")]);
    s.handle(Input::Nudge(willow_core::Side::Right), ms(20));
    settle(&mut s);
    assert_eq!(s.document().shapes[0].text, common::text("Gates"));
    assert_eq!(s.panel().text(), canonical(&s));
    assert_eq!(invalid_rows(&s), Vec::<usize>::new());
}

#[test]
fn a_regeneration_taken_before_typing_never_replaces_the_typing() {
    let mut s = panel_of(SIMPLE);
    s.handle(Input::Escape, ms(0));
    settle(&mut s);
    s.select([id("a")]);
    s.handle(Input::Nudge(willow_core::Side::Right), ms(10));
    // Still running off the UI thread when the writer clicks into the panel and types.
    let job = s.take_regeneration().unwrap();
    s.handle(Input::FocusPanel, ms(20));
    replace(&mut s, "\"Gates\"", "\"Fences\"", ms(30));
    let typed = s.panel().text().to_owned();
    s.regenerated(job.run());
    assert_eq!(s.panel().text(), typed);
    // The typing went into text that has the canvas edit: applying it keeps the edit.
    pause(&mut s, ms(180));
    assert_eq!(s.document().shapes[0].text, common::text("Fences"));
    assert!(s.document().shapes[0].x > 0);
}

#[test]
fn colors_and_zone_blocks_typed_reach_the_canvas() {
    let mut s = panel_of(SIMPLE);
    replace(
        &mut s,
        "a: \"Gates\"\n",
        "a: \"Gates\" color sage\nb: \"B\" below a\n",
        ms(0),
    );
    pause(&mut s, ms(150));
    let colors: Vec<_> = s.document().shapes.iter().map(|s| s.color.name()).collect();
    assert_eq!(colors, ["sage", "sage"]);
    // A new zone without layout wraps its members as a loaded one does.
    let block = "zone y \"Yard\" {\n  a: \"Gates\" color sage\n}\n";
    replace(&mut s, "a: \"Gates\" color sage\n", block, ms(200));
    pause(&mut s, ms(350));
    let loader = panel_of(s.panel().text());
    assert_eq!(s.document().zones, loader.document().zones);
    assert_eq!(s.document().zones[0].members, [id("a")]);
    // Deleting the block's lines takes the zone away; its member stays.
    replace(&mut s, "zone y \"Yard\" {\n", "", ms(400));
    replace(&mut s, "}\n", "", ms(400));
    pause(&mut s, ms(550));
    assert!(s.document().zones.is_empty());
    assert_eq!(ids(&s), ["a", "b"]);
}

#[test]
fn a_regeneration_taken_before_the_panel_had_focus_doesnt_land_under_the_caret() {
    let mut s = panel_of(SIMPLE);
    s.handle(Input::Escape, ms(0));
    s.select([id("a")]);
    s.handle(Input::Nudge(willow_core::Side::Right), ms(100));
    let job = s.take_regeneration().expect("due after the nudge");
    let text = s.panel().text().to_owned();
    s.handle(Input::FocusPanel, ms(200));
    s.regenerated(job.run());
    assert_eq!(s.panel().text(), text);
    // Leaving without typing, the text catches up.
    s.handle(Input::Escape, ms(300));
    settle(&mut s);
    assert!(s.panel().text().contains("@layout a 24,0"));
    // Opening the panel, which focuses it, still regenerates.
    s.handle(Input::TogglePanel, ms(400));
    s.handle(Input::Nudge(willow_core::Side::Right), ms(500));
    s.handle(Input::TogglePanel, ms(600));
    settle(&mut s);
    assert!(s.panel().text().contains("@layout a 48,0"));
}
