mod common;

use common::{Fake, text};
use willow_core::{Align, Layout, PAPER, layout};

/// Content width of a default 210-wide Paper shape; the fake measures CJK at 15.
const W: f32 = 178.0;

fn lay(s: &str) -> Layout {
    layout(&text(s), W, &PAPER, Align::Justified, &Fake)
}

fn lines(s: &str, l: &Layout) -> Vec<String> {
    (l.lines.iter())
        .map(|line| s[line.range.clone()].to_string())
        .collect()
}

#[test]
fn cjk_breaks_between_characters_and_no_line_starts_with_closing_punctuation() {
    // 11 characters of 15 fit in 178.
    let s = "一二三四五六七八九十百千万";
    assert_eq!(lines(s, &lay(s)), ["一二三四五六七八九十百", "千万"]);

    // `。` would start line two, so the character before it moves down with it.
    let s = "一二三四五六七八九十百。千万";
    assert_eq!(lines(s, &lay(s)), ["一二三四五六七八九十", "百。千万"]);
    // Same for a closing bracket, a small kana and the prolonged sound mark.
    for close in ["」", "ょ", "ー"] {
        let s = format!("一二三四五六七八九十百{close}千");
        assert_eq!(
            lines(&s, &lay(&s)),
            ["一二三四五六七八九十", &format!("百{close}千")]
        );
    }
}

#[test]
fn justified_cjk_lines_fill_the_width_over_the_gaps_between_characters() {
    let s = "一二三四五六七八九十百千万";
    let l = lay(s);
    let c = &l.lines[0].clusters;
    // 11 characters, 165 wide: 13 spare units over the 10 gaps between them.
    let last = c.last().unwrap();
    assert!((last.x + last.width - W).abs() < 0.01, "{last:?}");
    assert!((c[1].x - (15.0 + 1.3)).abs() < 0.01, "{:?}", c[1]);
    // The paragraph's last line is not stretched.
    assert_eq!(l.lines[1].clusters[1].x, 15.0);

    // No break before `。`, so no gap there either: it stays against its character.
    let s = "一二三。四五六七八九十百千";
    let c = &lay(s).lines[0].clusters;
    assert_eq!(c.len(), 11);
    assert_eq!(c[3].x, c[2].x + 15.0);
    let last = c.last().unwrap();
    assert!((last.x + last.width - W).abs() < 0.01, "{last:?}");
}
