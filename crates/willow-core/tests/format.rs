mod common;

use willow_core::{
    Color, Document, Link, LoadError, Paragraph, Run, Shape, ShapeId, Zone, load, save,
};

fn golden(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn shape(id: &str, x: i32, y: i32, text: &str) -> Shape {
    let mut s = Shape::new(ShapeId(id.into()), x, y);
    s.text = common::text(text);
    s
}

#[test]
fn saves_shapes_and_their_layout() {
    let mut b = shape("b", 290, 140, "");
    (b.width, b.extra_height) = (180, 60);
    let doc = Document {
        shapes: vec![b, shape("a", 0, 0, "The city closes its gates")],
        ..Document::default()
    };
    assert_eq!(saved(&doc).as_bytes(), golden("shapes.willow"));
}

#[test]
fn crlf_loads_without_a_repair_and_saves_as_lf() {
    let loaded = load(&golden("crlf.willow")).unwrap();
    assert!(!loaded.repaired);
    let a = &loaded.document.shapes[0];
    assert_eq!((a.text.plain().as_str(), a.x, a.y), ("Line", 5, -5));
    assert_eq!(
        saved(&loaded.document),
        "willow 1\n\na: \"Line\"\n\n@layout a 5,-5 w210\n"
    );
}

#[test]
fn a_byte_order_mark_and_spaces_after_the_header_load_without_a_repair() {
    let text = "\u{feff}willow 1 \t\n\na: \"Line\"\n\n@layout a 5,-5 w210\n";
    let loaded = load(text.as_bytes()).unwrap();
    assert!(!loaded.repaired);
    assert_eq!(loaded.document.shapes[0].text.plain(), "Line");
}

#[test]
fn files_without_the_header_from_a_newer_willow_or_not_utf8_do_not_open() {
    let err = |name| load(&golden(name)).unwrap_err();
    assert_eq!(err("no-header.willow"), LoadError::NotWillow);
    assert_eq!(err("newer.willow"), LoadError::Newer);
    assert_eq!(err("not-utf8.willow"), LoadError::NotUtf8);
    assert_eq!(load(b"").unwrap_err(), LoadError::NotWillow);
    assert_eq!(
        LoadError::Newer.to_string(),
        "It was made by a newer Willow."
    );
}

/// Canonical text for `doc`, with shape heights from the fake measurer.
fn saved(doc: &Document) -> String {
    let s = common::session_of(doc.clone());
    save(s.document(), &s.heights())
}

/// `doc` without drawing order: shapes, zones, members and links sorted, links unordered.
fn unordered(doc: &Document) -> Document {
    let key = |id: &ShapeId| (id.0.len(), id.0.clone());
    let mut doc = doc.clone();
    doc.shapes.sort_by_key(|s| key(&s.id));
    for Link(a, b) in &mut doc.links {
        if key(a) > key(b) {
            std::mem::swap(a, b);
        }
    }
    doc.links.sort_by_key(|Link(a, b)| (key(a), key(b)));
    doc.zones.sort_by_key(|z| key(&z.id));
    for z in &mut doc.zones {
        z.members.sort_by_key(key);
    }
    doc
}

#[test]
fn generated_documents_round_trip_byte_for_byte_and_stack_in_story_order() {
    for seed in 0..40 {
        let doc = willow_core::generate(300, seed);
        let first = saved(&doc);
        let loaded = load(first.as_bytes()).unwrap();
        assert!(!loaded.repaired, "seed {seed}");
        assert_eq!(unordered(&loaded.document), unordered(&doc), "seed {seed}");
        assert_eq!(saved(&loaded.document), first, "seed {seed}");
        // Shapes stack in the order their lines appear.
        let lines: Vec<&str> = (first.lines().map(str::trim_start))
            .filter_map(|l| l.split_once(": \"").map(|(id, _)| id))
            .collect();
        let stacked: Vec<&str> = loaded
            .document
            .shapes
            .iter()
            .map(|s| s.id.0.as_str())
            .collect();
        assert_eq!(stacked, lines, "seed {seed}");
    }
}

fn runs(doc: &Document, i: usize) -> Vec<Vec<(&str, bool, bool)>> {
    (doc.shapes[i].text.paragraphs.iter())
        .map(|p| {
            p.runs
                .iter()
                .map(|r| (r.text.as_str(), r.bold, r.italic))
                .collect()
        })
        .collect()
}

#[test]
fn loads_escapes_styles_line_breaks_and_cjk_and_saves_them_back_byte_for_byte() {
    let bytes = golden("text.willow");
    let loaded = load(&bytes).unwrap();
    assert!(!loaded.repaired);
    let doc = &loaded.document;

    assert_eq!(
        runs(doc, 0),
        [[
            ("Plain ", false, false),
            ("bold", true, false),
            (" ", false, false),
            ("italic", false, true),
            (" ", false, false),
            ("both", true, true),
            (" \"quoted\" 2*3 back\\slash", false, false),
        ]]
    );
    assert_eq!(
        runs(doc, 1),
        [
            vec![("Bold across", true, false)],
            vec![("two lines", true, false), (" then", false, false)],
            vec![],
            vec![("plain", false, false)],
        ]
    );
    assert_eq!(
        runs(doc, 2)[0][..4],
        [
            ("bold ", true, false),
            ("both", true, true),
            (" bold", true, false),
            (" and ", false, false),
        ]
    );
    assert_eq!(runs(doc, 3)[0][1], ("夜", false, true));
    let d = &doc.shapes[3];
    assert_eq!((d.x, d.y, d.width, d.extra_height), (-40, 300, 240, 12));

    assert_eq!(saved(doc).as_bytes(), bytes);
}

fn colored(id: &str, x: i32, y: i32, text: &str, color: Color) -> Shape {
    Shape {
        color,
        ..shape(id, x, y, text)
    }
}

fn link(a: &str, b: &str) -> Link {
    Link(ShapeId(a.into()), ShapeId(b.into()))
}

fn zone(id: &str, title: &str, color: Color, at: (i32, i32, u32, u32), members: &[&str]) -> Zone {
    Zone {
        id: ShapeId(id.into()),
        title: title.into(),
        color,
        x: at.0,
        y: at.1,
        width: at.2,
        height: at.3,
        members: members.iter().map(|m| ShapeId((*m).into())).collect(),
    }
}

/// `doc` saves as golden file `name`, which loads back as `doc` and saves the same.
fn assert_golden(doc: &Document, name: &str) {
    let bytes = golden(name);
    assert_eq!(saved(doc), String::from_utf8(bytes.clone()).unwrap());
    let loaded = load(&bytes).unwrap();
    assert!(!loaded.repaired);
    assert_eq!(unordered(&loaded.document), unordered(doc));
    assert_eq!(saved(&loaded.document).as_bytes(), bytes);
}

#[test]
fn a_trunk_with_branches_walks_below_right_left_above_with_its_color_on_the_first_shape() {
    let sage = |id, x, y, text| colored(id, x, y, text, Color::Sage);
    let mut b = sage("b", 0, 170, "");
    let run = |text: &str, bold| Run {
        text: text.into(),
        bold,
        italic: false,
    };
    b.text.paragraphs = vec![Paragraph {
        runs: vec![run("Food prices ", false), run("spike", true)],
    }];
    let doc = Document {
        shapes: vec![
            sage("f", 290, 340, "Curfew"),
            sage("d", 290, 170, "Riots"),
            sage("e", -290, 170, "Smugglers"),
            sage("g", 290, 0, "Rumours"),
            sage("c", 0, 340, "Grain runs out"),
            b,
            sage("a", 0, 0, "Gates close"),
        ],
        links: vec![
            link("f", "d"),
            link("b", "e"),
            link("g", "d"),
            link("a", "b"),
            link("d", "b"),
            link("b", "c"),
        ],
        ..Document::default()
    };
    assert_golden(&doc, "trunk.willow");
}

#[test]
fn ties_go_by_x_then_id_in_generation_order() {
    let doc = Document {
        shapes: vec![
            shape("d", 700, 400, "Right prong"),
            shape("b", 300, 0, "Bee"),
            shape("aa", 0, 0, "Double a"),
            shape("c", 600, 200, "Fork"),
            shape("z", 0, 0, "Zed"),
            shape("e", 500, 400, "Left prong"),
        ],
        links: vec![link("d", "c"), link("c", "e")],
        ..Document::default()
    };
    assert_golden(&doc, "ties.willow");
}

#[test]
fn a_chain_dipping_into_a_nested_zone_and_back_out_stays_one_unit() {
    let rose = |id, y, text| colored(id, 0, y, text, Color::Rose);
    let doc = Document {
        shapes: vec![
            shape("g", 0, 1000, "After"),
            rose("c", 340, "Back out"),
            rose("b", 170, "Dip"),
            rose("a", 0, "Start"),
        ],
        links: vec![link("c", "b"), link("b", "a")],
        zones: vec![
            zone(
                "p",
                "Act \"One\"",
                Color::Sky,
                (-40, -40, 300, 500),
                &["c", "n", "a"],
            ),
            zone("n", "Inner", Color::Neutral, (-20, 130, 260, 120), &["b"]),
        ],
    };
    assert_golden(&doc, "dip.willow");
}

#[test]
fn a_link_may_point_forward_across_zone_blocks() {
    let doc = Document {
        shapes: vec![
            colored("m", 400, 100, "Parent", Color::Rose),
            colored("k", 0, 300, "Child", Color::Rose),
            shape("j", 0, 0, "Unrelated"),
        ],
        links: vec![link("m", "k")],
        zones: vec![
            zone("r", "Second", Color::Peach, (360, 60, 300, 200), &["m"]),
            zone(
                "q",
                "First",
                Color::Neutral,
                (-40, -40, 300, 400),
                &["k", "j"],
            ),
        ],
    };
    assert_golden(&doc, "forward.willow");
}

#[test]
fn an_empty_zone_is_its_two_lines() {
    let doc = Document {
        shapes: vec![shape("i", 40, 700, "Alone")],
        zones: vec![
            zone(
                "f",
                "Holder",
                Color::Neutral,
                (0, 500, 400, 400),
                &["i", "h"],
            ),
            zone("h", "Nothing yet", Color::Neutral, (20, 540, 200, 100), &[]),
            zone("e", "Empty", Color::Lilac, (0, 0, 400, 200), &[]),
        ],
        ..Document::default()
    };
    assert_golden(&doc, "empty-zones.willow");
}

/// Golden file `name` loads as `doc` (order-free), flagged as repaired.
fn assert_repaired(name: &str, doc: &Document) {
    let loaded = load(&golden(name)).unwrap();
    assert!(loaded.repaired, "{name}");
    assert_eq!(unordered(&loaded.document), unordered(doc), "{name}");
}

#[test]
fn unreadable_lines_are_dropped() {
    let doc = Document {
        shapes: vec![shape("a", 0, 0, "Kept")],
        ..Document::default()
    };
    assert_repaired("repair-unknown.willow", &doc);
}

#[test]
fn a_duplicate_id_drops_the_later_line_be_it_shape_zone_or_layout() {
    let doc = Document {
        shapes: vec![
            shape("a", 0, 0, "First"),
            shape("b", 0, 140, "Bee"),
            shape("c", 0, 280, "Inside"),
        ],
        links: vec![link("a", "b")],
        ..Document::default()
    };
    assert_repaired("repair-duplicate.willow", &doc);
}

#[test]
fn a_link_closing_a_cycle_is_dropped() {
    let doc = Document {
        shapes: vec![
            shape("a", 0, 0, "A"),
            shape("b", 0, 140, "B"),
            shape("c", 0, 280, "C"),
        ],
        links: vec![link("a", "c"), link("a", "b")],
        ..Document::default()
    };
    assert_repaired("repair-cycle.willow", &doc);
}

#[test]
fn a_link_to_a_missing_id_is_dropped_and_the_shape_kept() {
    let doc = Document {
        shapes: vec![colored("b", 0, 0, "B", Color::Sky)],
        ..Document::default()
    };
    assert_repaired("repair-missing.willow", &doc);
}

#[test]
fn a_zone_nested_deeper_than_one_level_is_flattened_into_its_parent() {
    let doc = Document {
        shapes: vec![
            shape("a", 0, 0, "Deep shape"),
            shape("b", 0, 100, "Inner shape"),
        ],
        zones: vec![
            zone("p", "Outer", Color::Neutral, (-80, -80, 500, 400), &["q"]),
            zone(
                "q",
                "Inner",
                Color::Neutral,
                (-40, -40, 300, 300),
                &["a", "b"],
            ),
        ],
        ..Document::default()
    };
    assert_repaired("repair-deep.willow", &doc);
}

#[test]
fn conflicting_colors_in_a_chain_keep_the_first() {
    let doc = Document {
        shapes: vec![
            colored("a", 0, 0, "A", Color::Rose),
            colored("b", 0, 140, "B", Color::Rose),
        ],
        links: vec![link("a", "b")],
        ..Document::default()
    };
    assert_repaired("repair-colors.willow", &doc);
}

#[test]
fn a_stray_brace_is_dropped_and_an_unclosed_zone_closes_at_the_end() {
    let doc = Document {
        shapes: vec![shape("a", 0, 0, "Top"), shape("b", 0, 140, "Inside")],
        zones: vec![zone(
            "z",
            "Open",
            Color::Neutral,
            (-40, 100, 300, 200),
            &["b"],
        )],
        ..Document::default()
    };
    assert_repaired("repair-braces.willow", &doc);
}
