//! Shapes and zones a file gives no layout are placed when it opens. With the fake measurer a
//! one-line shape is 48.5 tall; Paper's creation gap is 48 and its zone padding 40.
mod common;

use willow_core::{PAPER, Session, load};

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
    // Centered on the anchor's side: b and e are 10 wider than a.
    assert_eq!(at(&s, "b"), (-5, 97, 210));
    assert_eq!(at(&s, "c"), (248, 0, 210));
    assert_eq!(at(&s, "d"), (-258, 0, 210));
    assert_eq!(at(&s, "e"), (-5, -97, 210));
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
fn a_zero_width_or_a_far_out_layout_is_clamped_as_a_repair_and_duplicates_without_overflow() {
    let mut s = open(
        "willow 1\n\n\
         a: \"A\"\n\n\
         @layout a 2147483600,0 w0\n",
    );
    assert!(!s.is_saved());
    let (x, _, w) = at(&s, "a");
    assert!(w > 0 && x.checked_add(w as i32).is_some(), "{x} {w}");
    let a = s.document().shapes[0].id.clone();
    s.select([a]);
    s.handle(willow_core::Input::Duplicate, std::time::Duration::ZERO);
    assert_eq!(s.document().shapes.len(), 2);
    let (x, _, w) = (s.document().shapes.iter().map(|s| (s.x, s.y, s.width)))
        .max()
        .unwrap();
    assert!(x.checked_add(w as i32).is_some(), "{x} {w}");
}
