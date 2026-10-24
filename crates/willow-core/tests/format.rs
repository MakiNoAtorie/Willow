mod common;

use willow_core::{Document, LoadError, Shape, ShapeId, load, save};

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
    };
    assert_eq!(save(&doc).as_bytes(), golden("shapes.willow"));
}

#[test]
fn crlf_loads_without_a_repair_and_saves_as_lf() {
    let loaded = load(&golden("crlf.willow")).unwrap();
    assert!(!loaded.repaired);
    let a = &loaded.document.shapes[0];
    assert_eq!((a.text.plain().as_str(), a.x, a.y), ("Line", 5, -5));
    assert_eq!(
        save(&loaded.document),
        "willow 1\n\na: \"Line\"\n\n@layout a 5,-5 w210\n"
    );
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

#[test]
fn generated_documents_round_trip_byte_for_byte() {
    for seed in 0..20 {
        let first = save(&willow_core::generate(300, seed));
        let loaded = load(first.as_bytes()).unwrap();
        assert!(!loaded.repaired, "seed {seed}");
        assert_eq!(save(&loaded.document), first, "seed {seed}");
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

    assert_eq!(save(doc).as_bytes(), bytes);
}
