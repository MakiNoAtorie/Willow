//! Zone titles edited in place: Rename, a double-click on the band, or right after Create zone.
mod common;

use std::time::Duration;

use common::session_of;
use willow_core::{
    Button, CaretView, Color, Document, Input, MenuAction, MenuEntry, Modifiers, Motion, Point,
    Recovery, Session, Shape, ShapeId, Side, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

/// Zone "z" at (0, 0), 800 x 600, titled `title`, holding shape "a" (and loose shape "b").
fn session(title: &str) -> Session {
    session_of(Document {
        shapes: vec![
            Shape::new(id("a"), 400, 300),
            Shape::new(id("b"), 1200, 300),
        ],
        links: vec![],
        zones: vec![Zone {
            id: id("z"),
            title: title.into(),
            color: Color::Neutral,
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            members: vec![id("a")],
        }],
    })
}

/// Screen point of canvas point (x, y).
fn screen(s: &Session, x: f32, y: f32) -> Point {
    s.viewport().to_screen(Point { x, y })
}

/// Right-clicks zone "z"'s band and chooses Rename.
fn rename(s: &mut Session) {
    let pos = screen(s, 20.0, 10.0);
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
    s.handle(Input::MenuChoose(MenuAction::Rename), T);
}

fn type_text(s: &mut Session, text: &str) {
    let (text, replace) = (text.into(), None);
    s.handle(Input::Text { text, replace }, T);
}

fn title(s: &Session) -> &str {
    &s.document().zones[0].title
}

fn caret(s: &Session) -> usize {
    s.editing().expect("editing").caret.offset
}

#[test]
fn rename_is_first_in_the_zone_menu_above_a_separator() {
    let mut s = session("Act I");
    let pos = screen(&s, 20.0, 10.0);
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    let entries = &s.menu().expect("zone menu").entries;
    assert!(
        matches!(entries[0], MenuEntry::Item(i) if i.action == MenuAction::Rename && i.label == "Rename")
    );
    assert_eq!(entries[1], MenuEntry::Separator);
}

#[test]
fn rename_types_into_the_title_moves_the_caret_and_enter_commits() {
    let mut s = session("Act");
    rename(&mut s);
    assert_eq!(caret(&s), 3);
    type_text(&mut s, " I");
    assert_eq!(title(&s), "Act I");
    s.handle(Input::Move(Motion::LineStart), T);
    assert_eq!(caret(&s), 0);
    s.handle(Input::Move(Motion::Right), T);
    assert_eq!(caret(&s), 1);
    s.handle(Input::Delete(Motion::Left), T);
    assert_eq!(title(&s), "ct I");
    s.handle(Input::Move(Motion::LineEnd), T);
    assert_eq!(caret(&s), 4);
    s.handle(Input::Move(Motion::WordLeft), T);
    assert_eq!(caret(&s), 3);
    s.handle(Input::Enter, T);
    assert!(s.editing().is_none());
    assert_eq!(title(&s), "ct I");
}

#[test]
fn esc_commits_and_a_title_takes_no_line_breaks_or_styles() {
    let mut s = session("");
    s.select([id("a")]);
    rename(&mut s);
    // Zones are never selected: editing a title clears the shape selection.
    assert!(s.selection().is_empty());
    type_text(&mut s, "Act");
    s.handle(Input::NewLine, T);
    s.handle(Input::ToggleBold, T);
    s.handle(Input::SelectAll, T);
    s.handle(Input::ToggleItalic, T);
    s.handle(Input::Move(Motion::LineEnd), T);
    s.handle(Input::Paste("one\r\ntwo".into()), T);
    assert_eq!(title(&s), "Actone two");
    // Canvas shortcuts don't fire while editing.
    s.handle(Input::Navigate(Side::Right), T);
    s.handle(Input::NewBeside, T);
    assert!(s.selection().is_empty());
    assert_eq!(s.document().shapes.len(), 2);
    s.handle(Input::Escape, T);
    assert!(s.editing().is_none());
    assert_eq!(title(&s), "Actone two");
}

#[test]
fn ime_marked_text_shows_in_the_title_and_only_its_commit_is_undoable() {
    let mut s = session("A");
    rename(&mut s);
    let (replace, selected) = (None, None);
    let text = "か".into();
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        T,
    );
    assert_eq!(title(&s), "Aか");
    assert_eq!(s.editing().unwrap().marked, Some(1..4));
    type_text(&mut s, "家");
    assert_eq!(title(&s), "A家");
    assert_eq!(caret(&s), 4);
    s.handle(Input::Enter, T);
    s.handle(Input::Undo, T);
    assert_eq!(title(&s), "A");
}

#[test]
fn a_recovery_write_waits_for_the_titles_composition() {
    let mut s = session("A");
    rename(&mut s);
    type_text(&mut s, "b");
    let (text, replace, selected) = ("か".into(), None, None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        T,
    );
    let due = Duration::from_secs(5);
    assert_eq!(s.take_recovery(due), None);
    assert_eq!(title(&s), "Abか");
    let (text, replace, selected) = (String::new(), None, None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        T,
    );
    let Some(Recovery::Write { document, .. }) = s.take_recovery(due) else {
        panic!("a write due");
    };
    assert_eq!(document.zones[0].title, "Ab");
}

/// The draw query's view of zone "z": shown title, caret, selection and marked underline.
type TitleView = (
    bool,
    Option<CaretView>,
    Vec<(Point, Point)>,
    Option<(Point, Point)>,
);

fn title_view(s: &Session) -> TitleView {
    let f = s.draw(
        Point::default(),
        Point {
            x: 1000.0,
            y: 800.0,
        },
        T,
    );
    let v = (f.zones.iter()).find(|v| v.zone.id == id("z")).unwrap();
    (v.show_title, v.caret, v.selection.clone(), v.marked)
}

#[test]
fn the_draw_query_shows_an_edited_title_even_when_it_overflows() {
    let mut s = session("Act");
    rename(&mut s);
    // A title 13 units tall, measured bold as it is drawn: the Fake's 11 per bold letter at
    // 15 is 28.6 for "Act", 16 in. Its 19.5-unit line box starts at the title's top, 12 down,
    // where the app paints it with the glyphs centered as in shape text.
    let (shown, caret, sel, marked) = title_view(&s);
    assert!(shown);
    let c = caret.expect("caret");
    assert_eq!((c.pos, c.height), (Point { x: 44.6, y: 12.0 }, 19.5));
    assert!(sel.is_empty() && marked.is_none());

    s.handle(Input::Select(Motion::LineStart), T);
    let (_, _, sel, _) = title_view(&s);
    assert_eq!(
        sel,
        [(Point { x: 16.0, y: 12.0 }, Point { x: 44.6, y: 31.5 })]
    );

    // Marked text underlines at the em box's bottom, (19.5 + 13) / 2 down the line.
    s.handle(Input::Move(Motion::LineEnd), T);
    let (text, replace, selected) = ("か".into(), None, None);
    s.handle(
        Input::Mark {
            text,
            replace,
            selected,
        },
        T,
    );
    let (_, _, _, marked) = title_view(&s);
    assert_eq!(
        marked,
        Some((Point { x: 44.6, y: 28.25 }, Point { x: 57.6, y: 28.25 }))
    );

    // 100 letters overflow the 800-wide zone; editing still shows them, committing hides them.
    type_text(&mut s, &"x".repeat(100));
    assert!(title_view(&s).0);
    s.handle(Input::Enter, T);
    assert_eq!(title_view(&s), (false, None, vec![], None));
}

/// Right-clicks loose shape "b" alone and chooses Create zone.
fn create_zone_around_b(s: &mut Session) {
    s.select([id("b")]);
    let (pos, button, mods) = (
        screen(s, 1210.0, 310.0),
        Button::Right,
        Modifiers::default(),
    );
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::MenuChoose(MenuAction::CreateZone), T);
}

#[test]
fn create_zone_edits_the_empty_title_and_one_undo_takes_both_back() {
    let mut s = session("");
    create_zone_around_b(&mut s);
    let new = s.document().zones[1].id.clone();
    assert_eq!(s.editing().map(|e| (&e.id, e.zone)), Some((&new, true)));
    type_text(&mut s, "Coda");
    s.handle(Input::Enter, T);
    assert_eq!(s.document().zones[1].title, "Coda");
    s.handle(Input::Undo, T);
    assert_eq!(s.document().zones.len(), 1);
    s.handle(Input::Redo, T);
    assert_eq!(s.document().zones[1].title, "Coda");
}

#[test]
fn an_empty_new_title_is_kept_and_undone_with_the_zone() {
    let mut s = session("");
    create_zone_around_b(&mut s);
    s.handle(Input::Escape, T);
    assert_eq!(s.document().zones.len(), 2);
    assert_eq!(s.document().zones[1].title, "");
    s.handle(Input::Undo, T);
    assert_eq!(s.document().zones.len(), 1);
    assert!(!s.can_undo());
}

#[test]
fn a_rename_is_one_step_and_an_unchanged_one_none() {
    let mut s = session("Act");
    rename(&mut s);
    type_text(&mut s, " I");
    s.handle(Input::Delete(Motion::Left), T);
    s.handle(Input::Enter, T);
    assert_eq!(title(&s), "Act ");
    s.handle(Input::Undo, T);
    assert_eq!(title(&s), "Act");
    assert!(!s.can_undo());

    rename(&mut s);
    type_text(&mut s, "x");
    s.handle(Input::Delete(Motion::Left), T);
    s.handle(Input::Enter, T);
    assert!(!s.can_undo());
}

#[test]
fn a_double_click_on_the_band_renames_as_one_step() {
    let mut s = session("Act");
    s.select([id("a")]);
    let (pos, button, mods) = (screen(&s, 300.0, 10.0), Button::Left, Modifiers::default());
    for clicks in [1, 2] {
        s.handle(Input::PointerDown { pos, button, mods }, T);
        if clicks == 2 {
            s.handle(Input::DoubleClick { pos }, T);
        }
        s.handle(Input::PointerUp { pos, button, mods }, T);
    }
    assert_eq!(s.editing().map(|e| (&e.id, e.zone)), Some((&id("z"), true)));
    assert!(s.selection().is_empty());
    type_text(&mut s, " I");
    s.handle(Input::Enter, T);
    assert_eq!(title(&s), "Act I");
    s.handle(Input::Undo, T);
    assert_eq!(title(&s), "Act");
    assert!(!s.can_undo());
}

#[test]
fn the_double_click_press_moves_nothing_when_dragged() {
    let mut s = session("Act");
    let (pos, button, mods) = (screen(&s, 300.0, 10.0), Button::Left, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::DoubleClick { pos }, T);
    let to = screen(&s, 350.0, 60.0);
    s.handle(Input::PointerMove { pos: to, mods }, T);
    s.handle(
        Input::PointerUp {
            pos: to,
            button,
            mods,
        },
        T,
    );
    assert_eq!(s.document().zones[0].x, 0);
    assert!(s.editing().is_some());
}

#[test]
fn a_double_click_off_the_band_or_on_a_zone_edge_edits_no_title() {
    let mut s = session("Act");
    let (button, mods) = (Button::Left, Modifiers::default());
    // Inside the zone below the band, then on its top edge.
    for (x, y) in [(300.0, 200.0), (300.0, 1.0)] {
        let pos = screen(&s, x, y);
        s.handle(Input::PointerDown { pos, button, mods }, T);
        s.handle(Input::DoubleClick { pos }, T);
        s.handle(Input::PointerUp { pos, button, mods }, T);
        assert!(s.editing().is_none());
    }
}
