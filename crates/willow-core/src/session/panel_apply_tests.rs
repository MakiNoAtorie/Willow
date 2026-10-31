//! What each line of regenerated text is, read without parsing, matches the line check.

use super::panel_apply::canonical_lines;
use crate::{check, generate, save};

#[test]
fn canonical_lines_are_what_the_line_check_says() {
    for seed in 0..20 {
        let doc = generate(200, seed);
        let heights = vec![60.0; doc.shapes.len()];
        let text = save(&doc, &heights, &[]);
        assert_eq!(
            canonical_lines(&text, &doc),
            check(&text).lines,
            "seed {seed}"
        );
    }
}
