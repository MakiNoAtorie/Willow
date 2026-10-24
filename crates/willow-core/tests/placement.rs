//! Shapes and zones a file gives no layout are placed when it opens. With the fake measurer a
//! one-line shape is 48.5 tall; Paper's creation gap is 48 and its zone padding 40.
mod common;

use willow_core::{Color, PAPER, Session, load};

fn open(text: &str) -> Session {
    let loaded = load(text.as_bytes()).unwrap();
    Session::open(loaded, text.into(), &PAPER, Box::new(common::Fake))
}

/// x, y, width of shape `id`.
fn at(s: &Session, id: &str) -> (i32, i32, u32) {
    let s = (s.document().shapes.iter())
        .find(|s| s.id.0 == id)
        .unwrap_or_else(|| panic!("no shape {id}"));
    (s.x, s.y, s.width)
}

/// x, y, width, height of zone `id`.
fn zone_at(s: &Session, id: &str) -> (i32, i32, u32, u32) {
    let z = (s.document().zones.iter()).find(|z| z.id.0 == id).unwrap();
    (z.x, z.y, z.width, z.height)
}

#[test]
fn a_linked_shape_goes_one_creation_gap_away_in_its_stated_direction() {
    let s = open(
        "willow 1\n\n\
         a: \"A\"\n\
         b: \"B\" below a\n\
         c: \"C\" right of a\n\
         d: \"D\" left of a\n\
         e: \"E\" above a\n\
         h: \"H\"\n\n\
         @layout a 0,0 w200\n\
         @layout h 10,20\n",
    );
    assert!(!s.is_saved());
    assert_eq!(at(&s, "b"), (0, 97, 210));
    assert_eq!(at(&s, "c"), (248, 0, 210));
    assert_eq!(at(&s, "d"), (-258, 0, 210));
    assert_eq!(at(&s, "e"), (0, -97, 210));
    // A missing width is the default width.
    assert_eq!(at(&s, "h"), (10, 20, 210));
}

#[test]
fn an_unlinked_shape_goes_below_the_lowest_and_chains_follow_their_anchor() {
    let s = open(
        "willow 1\n\n\
         i: \"I\" below j\n\
         j: \"J\"\n\
         k: \"K\"\n\
         m: \"M\" below k\n\
         f: \"F\"\n\n\
         @layout m 500,500 w210\n",
    );
    // j has no anchor, so it goes below m, the lowest; i follows j.
    assert_eq!(at(&s, "j"), (500, 597, 210));
    assert_eq!(at(&s, "i"), (500, 694, 210));
    // k is linked from m, which says it sits below k.
    assert_eq!(at(&s, "k"), (500, 404, 210));
    assert_eq!(at(&s, "f"), (500, 791, 210));
}

#[test]
fn a_file_without_any_layout_stacks_from_the_origin() {
    let s = open("willow 1\n\na: \"A\"\n\nb: \"B\"\n");
    assert_eq!(at(&s, "a"), (0, 0, 210));
    assert_eq!(at(&s, "b"), (0, 97, 210));
}

#[test]
fn a_zone_without_layout_wraps_its_members_with_padding() {
    let s = open(
        "willow 1\n\n\
         zone z \"Wrap\" {\n  g: \"G\"\n}\n\
         zone y \"Empty\" {\n}\n\n\
         @layout g 1000,1000 w100\n",
    );
    assert_eq!(zone_at(&s, "z"), (960, 960, 180, 129));
    // An empty one takes a small box below the lowest shape.
    assert_eq!(zone_at(&s, "y"), (1000, 1097, 290, 80));
}

#[test]
fn a_reload_from_disk_is_placed_too() {
    let mut s = open("willow 1\n\na: \"A\"\n\n@layout a 0,0 w210\n");
    s.disk_changed(Some(
        b"willow 1\n\na: \"A\"\nb: \"B\" right of a\n\n@layout a 0,0 w210\n",
    ));
    assert_eq!(at(&s, "b"), (258, 0, 210));
    assert!(!s.is_saved());
}

#[test]
fn an_unlinked_shape_goes_below_the_lowest_bottom_edge_not_the_lowest_top() {
    let s = open(
        "willow 1\n\n\
         a: \"A\"\n\
         b: \"B\"\n\
         c: \"C\"\n\n\
         @layout a 0,0 w210 h452\n\
         @layout b 300,100 w210\n",
    );
    // a is 500.5 tall, so its bottom is lower than b's (148.5).
    assert_eq!(at(&s, "c"), (0, 549, 210));
}

#[test]
fn conflicting_chain_colors_resolve_to_the_first_in_story_order() {
    // Story order walks a, b, c (below first), then d: c is the first to state a color,
    // though d comes first in the file and is the topmost to state one.
    let s = open(
        "willow 1\n\n\
         d: \"D\" right of a color rose\n\
         a: \"A\"\n\
         b: \"B\" below a\n\
         c: \"C\" below b color sky\n\n\
         @layout a 0,0 w210\n\
         @layout b 0,140 w210\n\
         @layout c 0,280 w210\n\
         @layout d 300,0 w210\n",
    );
    assert!(!s.is_saved());
    let colors: Vec<_> = s.document().shapes.iter().map(|s| s.color).collect();
    assert_eq!(colors, [Color::Sky; 4]);
}
