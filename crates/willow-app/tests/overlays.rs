//! An absolute div with no insets is placed where it would sit in flow, which is below the
//! full-height canvas, so it gets clipped. Phase 4's menus were invisible that way. Every
//! `.absolute()` in the app sets a vertical and a horizontal inset within the next few calls.

use std::fs;

/// The `.absolute()` lines (1-based) missing a vertical or horizontal inset.
fn missing_insets(src: &str) -> Vec<usize> {
    let lines: Vec<&str> = src.lines().collect();
    let has = |w: &[&str], names: &[&str]| {
        w.iter()
            .any(|l| names.iter().any(|n| l.contains(&format!(".{n}"))))
    };
    (lines.iter().enumerate())
        .filter(|(_, l)| l.contains(".absolute()"))
        .filter(|&(i, _)| {
            let w = &lines[i..(i + 6).min(lines.len())];
            !(has(w, &["top", "bottom", "inset"]) && has(w, &["left", "right", "inset"]))
        })
        .map(|(i, _)| i + 1)
        .collect()
}

#[test]
fn every_absolute_div_sets_its_insets() {
    assert_eq!(
        missing_insets("div()\n    .absolute()\n    .size_full()\n"),
        [2]
    );
    assert!(
        missing_insets("div()\n    .absolute()\n    .top_0()\n    .left(px(1.0))\n").is_empty()
    );
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    for e in fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        let missing = missing_insets(&fs::read_to_string(&path).unwrap());
        assert!(
            missing.is_empty(),
            "{}: lines {missing:?} need top/left insets",
            path.display()
        );
    }
}
