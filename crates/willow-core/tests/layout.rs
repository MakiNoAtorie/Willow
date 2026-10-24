mod common;

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use common::{Fake, text};
use willow_core::{Caret, Layout, Line, Measure, PAPER, Paragraph, Point, Session, Theme, layout};

/// Content width of a default 210-wide Paper shape (16 padding each side).
const W: f32 = 178.0;

fn lay(s: &str) -> Layout {
    layout(&text(s), W, &PAPER, &Fake)
}

/// Each line's text, from the cluster ranges.
fn lines(s: &str, l: &Layout) -> Vec<String> {
    l.lines
        .iter()
        .map(|line| s[line.range.clone()].to_string())
        .collect()
}

/// Right edge of the line's last non-space cluster.
fn ink_right(s: &str, line: &Line) -> f32 {
    line.clusters
        .iter()
        .filter(|c| &s[c.range.clone()] != " ")
        .map(|c| c.x + c.width)
        .fold(0.0, f32::max)
}

#[test]
fn lines_break_greedily_at_spaces_and_non_last_lines_fill_the_width() {
    // Words are 40 wide, spaces 4: four words and three spaces make 172.
    let s = "aaaa bbbb cccc dddd eeee ffff";
    let l = lay(s);

    assert_eq!(lines(s, &l), ["aaaa bbbb cccc dddd ", "eeee ffff"]);
    // Justified: 6 spare units over 3 gaps, so the first line ends exactly at the width.
    assert_eq!(ink_right(s, &l.lines[0]), W);
    assert_eq!(l.lines[0].clusters[4].x, 40.0);
    assert_eq!(l.lines[0].clusters[5].x, 46.0);
    // The last line of a paragraph is not stretched.
    assert_eq!(ink_right(s, &l.lines[1]), 84.0);
    assert_eq!(l.lines[1].clusters[4].x, 40.0);
    // Two lines of 15 px at 1.5.
    assert_eq!(l.height, 45.0);
    assert_eq!((l.lines[0].y, l.lines[1].y), (0.0, 22.5));
}

#[test]
fn a_word_wider_than_the_line_breaks_between_clusters_and_never_overflows() {
    // 20 clusters of 10 = 200 > 178: 17 fit on a line.
    let s = format!("ab {} cd", "x".repeat(20));
    let l = lay(&s);

    assert_eq!(
        lines(&s, &l),
        ["ab ", &"x".repeat(17), &format!("{} cd", "x".repeat(3))]
    );
    for line in &l.lines {
        assert!(ink_right(&s, line) <= W, "{line:?}");
    }
    // Nothing to stretch on the broken word's line: no spaces.
    assert_eq!(ink_right(&s, &l.lines[1]), 170.0);
}

#[test]
fn every_paragraph_starts_a_line_and_empty_text_is_one_line_tall() {
    let s = "aa\n\nbb";
    let l = lay(s);
    assert_eq!(lines(s, &l), ["aa", "", "bb"]);
    assert_eq!(l.lines[2].range, 4..6);
    assert_eq!(l.height, 67.5);

    let empty = layout(&Default::default(), W, &PAPER, &Fake);
    assert_eq!(empty.height, 22.5);
}

fn at(offset: usize) -> Caret {
    Caret {
        offset,
        upstream: false,
    }
}

#[test]
fn a_point_lands_on_the_nearest_caret_stop_of_its_line() {
    let l = lay("aaaa bbbb cccc dddd eeee ffff");
    let p = |x, y| l.caret_at(Point { x, y });

    // The justified space spans 40..46.
    assert_eq!(p(42.0, 5.0), at(4));
    assert_eq!(p(44.0, 5.0), at(5));
    // Past the end of a wrapped line: its end, shown on that line.
    assert_eq!(
        p(500.0, 5.0),
        Caret {
            offset: 20,
            upstream: true
        }
    );
    assert_eq!(p(3.0, 30.0), at(20));
    // Above the text is the first line; below it, the last.
    assert_eq!(p(14.0, -50.0), at(1));
    assert_eq!(p(500.0, 900.0), at(29));
}

#[test]
fn caret_positions_tell_a_wrap_end_from_the_next_line_start() {
    let l = lay("aaaa bbbb cccc dddd eeee ffff");
    assert_eq!(l.caret_point(at(20)), Point { x: 0.0, y: 22.5 });
    let end = Caret {
        offset: 20,
        upstream: true,
    };
    assert_eq!(l.caret_point(end), Point { x: W, y: 0.0 });
    assert_eq!(l.caret_point(at(7)), Point { x: 66.0, y: 0.0 });
}

/// The fake measurer, counting calls.
struct Counting(Rc<Cell<usize>>);

impl Measure for Counting {
    fn advances(&self, p: &Paragraph, c: &[Range<usize>], t: &Theme) -> Vec<f32> {
        self.0.set(self.0.get() + 1);
        Fake.advances(p, c, t)
    }
}

#[test]
fn a_shapes_layout_is_cached_until_its_text_or_width_changes() {
    let calls = Rc::new(Cell::new(0));
    let s = Session::new(
        common::doc_with("ab"),
        &PAPER,
        Box::new(Counting(calls.clone())),
    );
    let mut shape = s.document().shapes[0].clone();

    s.layout(&shape);
    s.shape_height(&shape);
    assert_eq!(calls.get(), 1);

    shape.width = 300;
    s.layout(&shape);
    shape.text = text("abc");
    s.layout(&shape);
    assert_eq!(calls.get(), 3);
}

const DOWN: bool = true;

#[test]
fn up_and_down_go_to_the_stop_nearest_the_goal_column() {
    let s = "aaaa bbbb cccc dddd eeee ffff\nx";
    let l = lay(s);

    assert_eq!(l.vertical(at(2), 66.0, DOWN), at(27));
    assert_eq!(l.vertical(at(27), 66.0, DOWN), at(31));
    assert_eq!(l.vertical(at(31), 66.0, !DOWN), at(27));
    assert_eq!(l.vertical(at(27), 66.0, !DOWN), at(7));
    // Off the top: the start; off the bottom: the end.
    assert_eq!(l.vertical(at(7), 66.0, !DOWN), at(0));
    assert_eq!(l.vertical(at(31), 66.0, DOWN), at(31));
}
