mod common;

use std::time::Duration;
use willow_core::{Button, Document, Input, Modifiers, PAPER, Point, Rgba, Session, ShapeId, Side};

const T: Duration = Duration::ZERO;

/// What the app does after every input: runs a due regeneration (off the UI thread there)
/// and hands the text back.
fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        let done = job.run();
        s.regenerated(done);
    }
}

fn input(s: &mut Session, i: Input) {
    s.handle(i, T);
    settle(s);
}

#[test]
fn opening_the_panel_regenerates_canonical_text() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    assert!(s.panel().open);
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 0,0 w210\n"
    );
}

#[test]
fn showing_the_panel_regenerates_and_leaves_the_canvas_the_keys() {
    let mut s = common::session_of(Document::new());
    s.show_panel();
    settle(&mut s);
    assert!(s.panel().open && !s.panel().focused);
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 0,0 w210\n"
    );
}

#[test]
fn canvas_shortcuts_dont_fire_while_the_panel_has_focus() {
    let mut s = common::session_of(Document::new());
    let a = ShapeId("a".into());
    s.select([a.clone()]);
    input(&mut s, Input::TogglePanel);
    assert!(s.panel().focused);
    let doc = s.document().clone();
    let (text, replace) = ("x".to_string(), None);
    for i in [
        Input::DeleteShapes,
        Input::Nudge(Side::Right),
        Input::Duplicate,
        Input::NewBeside,
        Input::StartEditing,
        Input::Enter,
        Input::Text { text, replace },
        Input::Tab,
        Input::Navigate(Side::Bottom),
        Input::Cut,
        Input::Paste("b".into()),
    ] {
        input(&mut s, i.clone());
        assert_eq!(*s.document(), doc, "{i:?}");
        assert!(s.editing().is_none() && !s.tab_picker_open(), "{i:?}");
        assert_eq!(s.selection(), std::slice::from_ref(&a), "{i:?}");
    }
    // The Edit menu's clipboard items don't take the selected shapes either.
    assert!(!s.can_cut() && !s.can_copy() && s.clipboard_text().is_none());
    // Esc hands the keys back to the canvas; the panel stays open.
    input(&mut s, Input::Escape);
    assert!(s.panel().open && !s.panel().focused);
    assert_eq!(s.selection(), std::slice::from_ref(&a));
    assert!(s.can_copy());
    input(&mut s, Input::DeleteShapes);
    assert!(s.document().shapes.is_empty());
}

#[test]
fn a_click_on_the_canvas_focuses_it_and_acts() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    let pos = s.viewport().to_screen(Point { x: 100.0, y: 20.0 });
    let (button, mods) = (Button::Left, Modifiers::default());
    input(&mut s, Input::PointerDown { pos, button, mods });
    input(&mut s, Input::PointerUp { pos, button, mods });
    assert!(s.panel().open && !s.panel().focused);
    assert_eq!(s.selection(), [ShapeId("a".into())]);
    // A click in the panel focuses it again.
    input(&mut s, Input::FocusPanel);
    assert!(s.panel().focused);
}

#[test]
fn the_toggle_closes_an_open_panel_and_the_canvas_has_the_keys() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    input(&mut s, Input::TogglePanel);
    assert!(!s.panel().open && !s.panel().focused);
    // Opening from an editing session ends it: its keys now go to the panel.
    s.select([ShapeId("a".into())]);
    input(&mut s, Input::StartEditing);
    input(&mut s, Input::TogglePanel);
    assert!(s.editing().is_none() && s.panel().focused);
}

#[test]
fn opening_and_closing_keep_the_canvas_content_where_it_is() {
    let mut s = common::session_of(Document::new());
    let corner = Point { x: 0.0, y: 0.0 };
    let at = s.viewport().to_screen(corner);
    input(&mut s, Input::TogglePanel);
    // The app narrows the canvas by the panel's width.
    s.resize(1000.0 - s.panel().width, 800.0);
    assert_eq!(s.viewport().to_screen(corner), at);
    input(&mut s, Input::TogglePanel);
    s.resize(1000.0, 800.0);
    assert_eq!(s.viewport().to_screen(corner), at);
}

/// The visible rows' text and line numbers.
fn rows(s: &Session) -> Vec<(Option<usize>, String)> {
    let text = s.panel().text();
    (s.panel_rows().iter())
        .map(|r| (r.number, text[r.range.clone()].to_string()))
        .collect()
}

#[test]
fn long_lines_wrap_and_numbers_count_logical_lines() {
    let mut s = common::session_of(Document::new());
    // 420 px wide: 48 columns of 7.5 px after the hairline, gutter and right padding.
    let (x, y, z) = ("x".repeat(40), "y".repeat(20), "z".repeat(100));
    s.set_panel_text(format!("willow 1\n\n{x} {y}\n{z}\n"));
    let want = [
        (Some(1), "willow 1".to_string()),
        (Some(2), String::new()),
        (Some(3), format!("{x} ")),
        (None, y.clone()),
        (Some(4), "z".repeat(48)),
        (None, "z".repeat(48)),
        (None, "z".repeat(4)),
        (Some(5), String::new()),
    ];
    assert_eq!(rows(&s), want);
    let tops: Vec<f32> = s.panel_rows().iter().map(|r| r.y).collect();
    assert_eq!(tops[..3], [46.0, 66.625, 87.25]);
}

#[test]
fn rows_are_colored_by_syntax() {
    let mut s = common::session_of(Document::new());
    let c = PAPER.code;
    s.set_panel_text(
        "willow 1\nzone d \"Arc\" color sky {\n  a: \"Hi\" right of b color sage\n}\n@layout a 0,0 w210\n"
            .into(),
    );
    let text = s.panel().text().to_string();
    let spans: Vec<Vec<(String, Rgba, bool)>> = (s.panel_rows().iter())
        .map(|r| {
            (r.spans.iter())
                .map(|sp| (text[sp.range.clone()].to_string(), sp.color, sp.bold))
                .collect()
        })
        .collect();
    let (ink, sp) = (PAPER.ink, |t: &str, c, b| (t.to_string(), c, b));
    assert_eq!(
        spans[0],
        [sp("willow", c.keyword, false), sp(" 1", ink, false)]
    );
    assert_eq!(
        spans[1],
        [
            sp("zone", c.keyword, false),
            sp(" ", ink, false),
            sp("d", c.id, true),
            sp(" ", ink, false),
            sp("\"Arc\"", c.string, false),
            sp(" ", ink, false),
            sp("color", c.keyword, false),
            sp(" sky {", ink, false),
        ]
    );
    assert_eq!(
        spans[2],
        [
            sp("  ", ink, false),
            sp("a", c.id, true),
            sp(": ", ink, false),
            sp("\"Hi\"", c.string, false),
            sp(" ", ink, false),
            sp("right of", c.direction, false),
            sp(" b ", ink, false),
            sp("color", c.keyword, false),
            sp(" sage", ink, false),
        ]
    );
    assert_eq!(spans[3], [sp("}", ink, false)]);
    assert_eq!(
        spans[4],
        [
            sp("@layout", c.layout, false),
            sp(" a 0,0 w210", ink, false)
        ]
    );
}

#[test]
fn an_unclosed_quote_colors_to_the_end_of_its_line() {
    let mut s = common::session_of(Document::new());
    s.set_panel_text("a: \"Hi below b\nb: \"x\"\n".into());
    let text = s.panel().text().to_string();
    let r = &s.panel_rows()[0];
    let last = r.spans.last().unwrap();
    assert_eq!(text[last.range.clone()], *"\"Hi below b");
    assert_eq!(last.color, PAPER.code.string);
}

#[test]
fn only_visible_rows_are_laid_out_and_the_panel_scrolls() {
    let mut s = common::session_of(Document::new());
    let lines: Vec<String> = (1..=1000)
        .map(|i| format!("@layout a {i},0 w210"))
        .collect();
    s.set_panel_text(lines.join("\n"));
    // 800 px tall: rows from 46 px down, 20.625 px each.
    assert_eq!(s.panel_rows().len(), 37);
    // Ten rows up: line 8 is the first still showing, its top 15.875 px above.
    input(&mut s, Input::ScrollPanel(-206.25));
    let first = &s.panel_rows()[0];
    assert_eq!(first.number, Some(8));
    assert!((first.y + 15.875).abs() < 1e-3, "{}", first.y);
    // Scrolling stops with the last line above the bottom padding, and at the top.
    input(&mut s, Input::ScrollPanel(-1e9));
    let last = s.panel_rows().last().unwrap().clone();
    assert_eq!(last.number, Some(1000));
    assert!((last.y + 20.625 + 14.0 - 800.0).abs() < 1e-2, "{}", last.y);
    input(&mut s, Input::ScrollPanel(1e9));
    assert_eq!(s.panel_rows()[0].y, 46.0);
}

#[test]
fn a_closed_panel_never_regenerates() {
    let mut s = common::session_of(Document::new());
    s.select([ShapeId("a".into())]);
    input(&mut s, Input::Nudge(Side::Right));
    assert!(s.take_regeneration().is_none());
    assert_eq!(s.panel().text(), "");
    // Open, then closed again: later steps leave the text as it was.
    input(&mut s, Input::TogglePanel);
    input(&mut s, Input::TogglePanel);
    input(&mut s, Input::Nudge(Side::Right));
    assert!(s.take_regeneration().is_none());
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 24,0 w210\n"
    );
}

#[test]
fn each_finished_canvas_step_regenerates_while_open() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    input(&mut s, Input::Escape);
    s.select([ShapeId("a".into())]);
    input(&mut s, Input::Nudge(Side::Bottom));
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 0,24 w210\n"
    );
    input(&mut s, Input::Undo);
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 0,0 w210\n"
    );
}

#[test]
fn typing_in_a_shape_shows_when_its_editing_session_ends() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    input(&mut s, Input::Escape);
    s.select([ShapeId("a".into())]);
    input(&mut s, Input::StartEditing);
    for t in ["H", "i"] {
        let (text, replace) = (t.into(), None);
        input(&mut s, Input::Text { text, replace });
        assert_eq!(
            s.panel().text(),
            "willow 1\n\na: \"\"\n\n@layout a 0,0 w210\n"
        );
    }
    input(&mut s, Input::Escape);
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"Hi\"\n\n@layout a 0,0 w210\n"
    );
}

#[test]
fn a_drag_regenerates_on_release_not_per_frame() {
    let mut s = common::session_of(Document::new());
    input(&mut s, Input::TogglePanel);
    let before = s.panel().text().to_string();
    let mods = Modifiers::default();
    let a = s.viewport().to_screen(Point { x: 100.0, y: 20.0 });
    let b = Point {
        x: a.x + 48.0,
        y: a.y,
    };
    let (button, half) = (
        Button::Left,
        Point {
            x: a.x + 24.0,
            y: a.y,
        },
    );
    input(
        &mut s,
        Input::PointerDown {
            pos: a,
            button,
            mods,
        },
    );
    for pos in [half, b] {
        input(&mut s, Input::PointerMove { pos, mods });
        assert_eq!(s.panel().text(), before);
    }
    input(
        &mut s,
        Input::PointerUp {
            pos: b,
            button,
            mods,
        },
    );
    assert_eq!(
        s.panel().text(),
        "willow 1\n\na: \"\"\n\n@layout a 48,0 w210\n"
    );
}
