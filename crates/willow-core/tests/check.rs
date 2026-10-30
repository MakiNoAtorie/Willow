use willow_core::{LineKind, ShapeId, check};

fn golden(name: &str) -> String {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The invalid and ignored lines of golden file `name`, numbered from 1.
fn marked(name: &str) -> Vec<(usize, LineKind)> {
    let checked = check(&golden(name));
    (checked.lines.into_iter().enumerate())
        .filter(|(_, l)| matches!(l, LineKind::Invalid | LineKind::Ignored))
        .map(|(n, l)| (n + 1, l))
        .collect()
}

use LineKind::{Ignored, Invalid};

#[test]
fn an_unknown_line_is_ignored_and_a_malformed_shape_or_layout_line_is_invalid() {
    assert_eq!(
        marked("repair-unknown.willow"),
        [(4, Ignored), (5, Invalid), (8, Invalid)]
    );
}

#[test]
fn a_duplicate_id_marks_the_later_line_be_it_shape_zone_or_layout() {
    // The later `@layout a` has nothing left to place: ignored.
    assert_eq!(
        marked("repair-duplicate.willow"),
        [(5, Invalid), (6, Invalid), (13, Ignored)]
    );
}

#[test]
fn a_link_closing_a_cycle_marks_the_shape_stating_it() {
    assert_eq!(marked("repair-cycle.willow"), [(5, Invalid)]);
}

#[test]
fn a_link_to_a_missing_id_is_invalid() {
    assert_eq!(marked("repair-missing.willow"), [(3, Invalid)]);
}

#[test]
fn a_zone_two_levels_deep_is_invalid_and_its_layout_ignored() {
    assert_eq!(marked("repair-deep.willow"), [(5, Invalid), (16, Ignored)]);
}

#[test]
fn a_stray_brace_and_an_unclosed_zone_line_are_invalid() {
    assert_eq!(marked("repair-braces.willow"), [(4, Invalid), (5, Invalid)]);
}

#[test]
fn a_malformed_zone_line_is_invalid() {
    assert_eq!(marked("repair-bad-zone.willow"), [(4, Invalid)]);
}

#[test]
fn a_bad_color_on_a_shape_line_is_invalid() {
    assert_eq!(marked("repair-bad-color-link.willow"), [(4, Invalid)]);
}

#[test]
fn conflicting_chain_colors_stay_quiet() {
    for name in ["repair-colors.willow", "repair-split-colors.willow"] {
        let checked = check(&golden(name));
        assert!(checked.repaired && !checked.invalid(), "{name}");
        assert_eq!(marked(name), [], "{name}");
    }
}

#[test]
fn a_quote_left_open_runs_to_the_end_of_its_line() {
    let checked = check(&golden("repair-unclosed-quote.willow"));
    let texts: Vec<String> = (checked.document.shapes.iter())
        .map(|s| s.text.plain())
        .collect();
    assert_eq!(
        texts,
        [
            "Runs to the end",
            "Escaped \" then open below a",
            "Trailing \\"
        ]
    );
    assert!(checked.document.links.is_empty() && checked.document.zones.is_empty());
    assert!(checked.repaired);
    // The zone's title swallows its `{`; the `@layout` for it has no zone to place.
    assert_eq!(
        marked("repair-unclosed-quote.willow"),
        [(6, Invalid), (12, Ignored)]
    );
}

#[test]
fn shape_and_layout_lines_name_their_shape() {
    let shape = |id: &str| LineKind::Shape(ShapeId(id.into()));
    let fine = || LineKind::Fine;
    let checked = check(&golden("forward.willow"));
    let expected = [
        fine(),
        fine(),
        fine(),
        shape("j"),
        shape("k"),
        fine(),
        fine(),
        fine(),
        shape("m"),
        fine(),
        fine(),
        shape("j"),
        shape("k"),
        shape("m"),
        fine(),
        fine(),
    ];
    assert_eq!(checked.lines, expected);
    assert!(!checked.repaired);
}

#[test]
fn a_layout_of_the_wrong_kind_is_ignored_and_its_shape_or_zone_placed() {
    let checked =
        check("willow 1\na: \"A\"\nzone z \"Z\" {\n}\n@layout a 0,0 10x10\n@layout z 0,0 w210\n");
    assert_eq!(checked.lines[4..], [Ignored, Ignored]);
    assert_eq!(checked.lines[1], LineKind::Shape(ShapeId("a".into())));
    assert!(checked.repaired && !checked.invalid());
}

#[test]
fn text_without_the_header_has_line_one_invalid() {
    for text in ["", "a: \"A\"\n", "willow 2\n", "willow\n"] {
        assert_eq!(check(text).lines[0], Invalid, "{text:?}");
    }
    assert_eq!(check("\u{feff}willow 1\r\n").lines, [LineKind::Fine]);
}
