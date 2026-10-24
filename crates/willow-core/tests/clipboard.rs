//! Shape clipboard: Copy gives Willow text, Paste makes linked copies with fresh IDs at the
//! cursor, Duplicate beside the originals, Cut deletes like Delete.
mod common;

use std::time::Duration;

use common::{session_of, text};
use willow_core::{
    Button, Color, Document, Input, Link, MenuAction, MenuEntry, MenuKind, Modifiers, Point,
    Session, Shape, ShapeId, Zone,
};

const T: Duration = Duration::ZERO;

fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

fn shape(name: &str, x: i32, y: i32, color: Color) -> Shape {
    let mut s = Shape::new(id(name), x, y);
    s.text = text(name);
    s.color = color;
    s
}

/// a (0,0) above b (0,200) above c (0,400), a rose chain; d (600,0) sky, alone.
fn chain() -> Session {
    session_of(Document {
        shapes: vec![
            shape("a", 0, 0, Color::Rose),
            shape("b", 0, 200, Color::Rose),
            shape("c", 0, 400, Color::Rose),
            shape("d", 600, 0, Color::Sky),
        ],
        links: vec![Link(id("a"), id("b")), Link(id("b"), id("c"))],
        ..Document::default()
    })
}

fn pointer(s: &mut Session, canvas: Point) {
    let pos = s.viewport().to_screen(canvas);
    let mods = Modifiers::default();
    s.handle(Input::PointerMove { pos, mods }, T);
}

fn find<'a>(s: &'a Session, text: &str, skip: &[&str]) -> &'a Shape {
    (s.document().shapes.iter())
        .find(|x| x.text.plain() == text && !skip.contains(&x.id.0.as_str()))
        .expect("shape")
}

fn linked(s: &Session, a: &ShapeId, b: &ShapeId) -> bool {
    (s.document().links.iter()).any(|Link(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
}

#[test]
fn copy_then_paste_keeps_internal_links_and_colors_with_fresh_ids() {
    let mut s = chain();
    s.select([id("b"), id("c")]);
    let copied = s.clipboard_text().expect("something to copy");
    assert!(copied.starts_with("willow 1\n"), "{copied}");
    pointer(&mut s, Point { x: 300.0, y: 300.0 });
    s.handle(Input::Paste(copied), T);

    let doc = s.document();
    assert_eq!(doc.shapes.len(), 6);
    let (b2, c2) = (find(&s, "b", &["b"]), find(&s, "c", &["c"]));
    for new in [&b2.id, &c2.id] {
        assert!(!["a", "b", "c", "d"].contains(&new.0.as_str()), "{new:?}");
    }
    assert!(linked(&s, &b2.id, &c2.id));
    assert_eq!((b2.color, c2.color), (Color::Rose, Color::Rose));
    // The link to a (unselected) is dropped; nothing links to the originals.
    assert_eq!(doc.links.len(), 3);
    assert_eq!(s.selection(), [b2.id.clone(), c2.id.clone()]);
    // One undo step takes the copies back.
    s.handle(Input::Undo, T);
    assert_eq!(s.document().shapes.len(), 4);
}

#[test]
fn plain_text_pastes_as_one_neutral_unlinked_shape() {
    let mut s = chain();
    s.select([id("d")]);
    let target = s.viewport().to_canvas(Point { x: 700.0, y: 300.0 });
    pointer(&mut s, target);
    s.handle(Input::Paste("Gull Rock\r\nat dawn".into()), T);
    let new = s.selection().to_vec();
    assert_eq!(new.len(), 1);
    let shape = (s.document().shapes.last()).unwrap();
    assert_eq!(shape.id, new[0]);
    assert_eq!(shape.text.plain(), "Gull Rock\nat dawn");
    assert_eq!(shape.color, Color::Neutral);
    assert_eq!(s.document().links.len(), 2);
    assert!(near(center_of(&s, &new), target));
}

#[test]
fn pasting_shapes_while_editing_inserts_their_texts_one_per_line() {
    let mut s = chain();
    s.select([id("a"), id("b")]);
    let copied = s.clipboard_text().unwrap();
    s.select([id("d")]);
    s.handle(Input::StartEditing, T);
    s.handle(Input::Paste(copied), T);
    s.handle(Input::Paste("!".into()), T);
    assert_eq!(s.document().shapes.len(), 4);
    assert_eq!(find(&s, "da\nb!", &[]).id, id("d"));
    // While editing, Copy gives the selected text as before.
    s.handle(Input::SelectAll, T);
    assert_eq!(s.clipboard_text().as_deref(), Some("da\nb!"));
}

#[test]
fn cut_copies_then_reconnects_like_delete() {
    let (mut cut, mut deleted) = (chain(), chain());
    cut.select([id("b")]);
    deleted.select([id("b")]);
    let copied = cut.clipboard_text().unwrap();
    cut.handle(Input::Cut, T);
    deleted.handle(Input::DeleteShapes, T);
    assert_eq!(cut.document(), deleted.document());
    assert!(linked(&cut, &id("a"), &id("c")));
    // What was cut pastes back.
    cut.handle(Input::Paste(copied), T);
    assert_eq!(cut.document().shapes.len(), 4);
    cut.handle(Input::Undo, T);
    cut.handle(Input::Undo, T);
    assert_eq!(cut.document(), chain().document());
}

fn right_click(s: &mut Session, canvas: Point) {
    let pos = s.viewport().to_screen(canvas);
    let (button, mods) = (Button::Right, Modifiers::default());
    s.handle(Input::PointerDown { pos, button, mods }, T);
    s.handle(Input::PointerUp { pos, button, mods }, T);
}

fn labels(s: &Session) -> Vec<&'static str> {
    (s.menu().unwrap().entries.iter())
        .filter_map(|e| match e {
            MenuEntry::Item(i) => Some(i.label),
            _ => None,
        })
        .collect()
}

#[test]
fn right_click_in_a_multi_selection_keeps_it_and_offers_multi_items() {
    let mut s = chain();
    s.select([id("a"), id("d")]);
    right_click(&mut s, Point { x: 650.0, y: 20.0 });
    assert_eq!(s.selection(), [id("a"), id("d")]);
    assert_eq!(s.menu().unwrap().kind, MenuKind::Multi);
    assert_eq!(labels(&s), ["Cut", "Copy", "Duplicate", "Delete"]);
    s.handle(Input::MenuChoose(MenuAction::Duplicate), T);
    assert!(s.menu().is_none());
    assert_eq!(s.document().shapes.len(), 6);

    right_click(&mut s, Point { x: 650.0, y: 20.0 });
    assert_eq!(s.selection(), [id("d")]);
    assert_eq!(s.menu().unwrap().kind, MenuKind::Shape);
    assert_eq!(labels(&s), ["Cut", "Copy", "Paste", "Duplicate"]);
    s.select([id("a"), id("b"), id("c")]);
    right_click(&mut s, Point { x: 20.0, y: 220.0 });
    s.handle(Input::MenuChoose(MenuAction::Delete), T);
    assert_eq!(s.document().shapes.len(), 3);
}

#[test]
fn paste_from_a_menu_goes_where_it_was_opened() {
    let mut s = chain();
    s.select([id("d")]);
    let copied = s.clipboard_text().unwrap();
    let target = Point { x: 650.0, y: 20.0 };
    right_click(&mut s, target);
    pointer(&mut s, Point { x: 0.0, y: 0.0 });
    s.handle(Input::Paste(copied), T);
    assert!(s.menu().is_none());
    assert!(near(center_of(&s, s.selection()), target));
}

/// The copies' bounding box center, canvas units.
fn center_of(s: &Session, ids: &[ShapeId]) -> Point {
    let shapes: Vec<&Shape> = (s.document().shapes.iter())
        .filter(|x| ids.contains(&x.id))
        .collect();
    let lo_x = shapes.iter().map(|x| x.x).min().unwrap() as f32;
    let hi_x = shapes.iter().map(|x| x.x + x.width as i32).max().unwrap() as f32;
    let lo_y = shapes.iter().map(|x| x.y as f32).fold(f32::MAX, f32::min);
    let hi_y = (shapes.iter())
        .map(|x| x.y as f32 + s.shape_height(x))
        .fold(f32::MIN, f32::max);
    Point {
        x: (lo_x + hi_x) / 2.0,
        y: (lo_y + hi_y) / 2.0,
    }
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= 0.5 && (a.y - b.y).abs() <= 0.5
}

#[test]
fn paste_centers_on_the_pointer_or_else_the_view_center() {
    let mut s = chain();
    s.select([id("a"), id("b"), id("d")]);
    let copied = s.clipboard_text().unwrap();
    let target = s.viewport().to_canvas(Point { x: 700.0, y: 300.0 });
    pointer(&mut s, target);
    s.handle(Input::Paste(copied.clone()), T);
    let first = s.selection().to_vec();
    assert_eq!(first.len(), 3);
    assert!(
        near(center_of(&s, &first), target),
        "{:?}",
        center_of(&s, &first)
    );

    // The pointer left the canvas: the view center.
    let mods = Modifiers::default();
    let pos = Point { x: -20.0, y: 300.0 };
    s.handle(Input::PointerMove { pos, mods }, T);
    s.handle(Input::Paste(copied), T);
    let second = s.selection().to_vec();
    let center = s.viewport().to_canvas(Point { x: 500.0, y: 400.0 });
    assert!(near(center_of(&s, &second), center));
    assert!(first.iter().all(|i| !second.contains(i)));
}

#[test]
fn paste_after_the_pointer_left_the_window_goes_to_the_view_center() {
    let mut s = chain();
    s.select([id("d")]);
    let copied = s.clipboard_text().unwrap();
    // Last seen well inside the canvas; then it leaves the window, with no move outside.
    let inside = s.viewport().to_canvas(Point { x: 700.0, y: 300.0 });
    pointer(&mut s, inside);
    s.handle(Input::PointerLeft, T);
    s.handle(Input::Paste(copied), T);
    let pasted = s.selection().to_vec();
    let center = s.viewport().to_canvas(Point { x: 500.0, y: 400.0 });
    assert!(near(center_of(&s, &pasted), center));
}

fn zone(name: &str, x: i32, y: i32, w: u32, h: u32, members: &[&str]) -> Zone {
    Zone {
        id: id(name),
        title: String::new(),
        color: Color::Neutral,
        x,
        y,
        width: w,
        height: h,
        members: members.iter().map(|m| id(m)).collect(),
    }
}

#[test]
fn paste_joins_the_innermost_zone_under_the_pointer() {
    let mut s = chain();
    // Canvas point under screen (700, 300); same first shape, so same viewport below.
    let t = s.viewport().to_canvas(Point { x: 700.0, y: 300.0 });
    let (tx, ty) = (t.x as i32, t.y as i32);
    let mut doc = s.document().clone();
    doc.zones = vec![
        zone("y", tx - 500, ty - 500, 1000, 1000, &[]),
        zone("z", tx - 100, ty - 100, 200, 200, &[]),
    ];
    let mut s2 = session_of(doc);
    s.select([id("a"), id("b")]);
    let copied = s.clipboard_text().unwrap();
    pointer(&mut s2, t);
    s2.handle(Input::Paste(copied), T);
    let new = s2.selection().to_vec();
    let zones = &s2.document().zones;
    assert_eq!(zones[1].members, new);
    assert!(zones[0].members.is_empty());
}

#[test]
fn duplicate_goes_one_gap_right_in_the_originals_zones() {
    let mut doc = chain().document().clone();
    doc.zones = vec![zone("z", -50, -50, 400, 400, &["a"])];
    let mut s = session_of(doc);
    s.select([id("a"), id("b"), id("d")]);
    s.handle(Input::Duplicate, T);
    let new = s.selection().to_vec();
    assert_eq!(new.len(), 3);
    let doc = s.document();
    let at = |i: &ShapeId| {
        let x = doc.shapes.iter().find(|x| &x.id == i).unwrap();
        (x.text.plain(), x.x, x.y)
    };
    // The originals span x 0 to 810; the gap is 48.
    let mut got: Vec<_> = new.iter().map(at).collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("a".into(), 858, 0),
            ("b".into(), 858, 200),
            ("d".into(), 1458, 0)
        ]
    );
    let a2 = (new.iter()).find(|i| at(i).0 == "a").unwrap();
    assert_eq!(doc.zones[0].members, [id("a"), a2.clone()]);
    assert_eq!(doc.links.len(), 3);
    s.handle(Input::Undo, T);
    assert_eq!(s.document().shapes.len(), 4);
    assert_eq!(s.document().zones[0].members, [id("a")]);
}
