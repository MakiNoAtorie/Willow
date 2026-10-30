//! Panel rows after edits: whatever is typed, the rows and caret points are the ones the same
//! text gets when shown fresh.

mod common;

use std::time::Duration;
use willow_core::{Document, Input, Motion, Session};

const T: Duration = Duration::ZERO;

/// xorshift64: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// An open, focused panel showing `text`, tall enough that nothing scrolls.
fn panel(text: &str) -> Session {
    let mut s = common::session_of(Document::new());
    s.resize(1000.0, 6000.0);
    s.handle(Input::TogglePanel, T);
    s.set_panel_text(text.into());
    s
}

const PIECES: [&str; 9] = ["a", "bb ", " ", "\n", "字", "é", "👍🏽", "word ", "x y\nz"];
const MOTIONS: [Motion; 7] = [
    Motion::Left,
    Motion::Right,
    Motion::Up,
    Motion::Down,
    Motion::LineStart,
    Motion::LineEnd,
    Motion::WordLeft,
];

fn piece(r: &mut Rng) -> String {
    (0..1 + r.below(12))
        .map(|_| PIECES[r.below(PIECES.len() as u64) as usize])
        .collect()
}

/// Each row's line number, top and text. (Colors follow the last check, which a paste or a
/// line change runs at once and a fresh panel hasn't.)
fn rows(s: &Session) -> Vec<(Option<usize>, f32, std::ops::Range<usize>)> {
    (s.panel_rows().into_iter())
        .map(|r| (r.number, r.y, r.range))
        .collect()
}

#[test]
fn rows_after_any_edit_are_the_rows_of_the_text_shown_fresh() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    for case in 0..60 {
        let start = piece(&mut r).repeat(1 + r.below(3) as usize);
        let mut s = panel(&start);
        for step in 0..80 {
            let m = MOTIONS[r.below(MOTIONS.len() as u64) as usize];
            let input = match r.below(9) {
                0 | 1 => Input::Text {
                    text: piece(&mut r),
                    replace: None,
                },
                2 => {
                    let n = s.panel().text().len() as u64 + 1;
                    let (a, b) = (r.below(n) as usize, r.below(n) as usize);
                    Input::Text {
                        text: piece(&mut r),
                        replace: Some(a.min(b)..a.max(b)),
                    }
                }
                3 => Input::Paste(piece(&mut r)),
                4 => Input::Delete(m),
                5 => Input::Move(m),
                6 => Input::Select(m),
                7 => Input::Enter,
                _ => Input::Mark {
                    text: piece(&mut r),
                    replace: None,
                    selected: None,
                },
            };
            s.handle(input, T);
            if s.panel().text().len() > 160 {
                s.handle(Input::SelectAll, T);
                s.handle(Input::Delete(Motion::Left), T);
            }
            let text = s.panel().text().to_owned();
            let fresh = panel(&text);
            assert_eq!(rows(&s), rows(&fresh), "case {case} step {step}: {text:?}");
            for (i, _) in text.char_indices().chain([(text.len(), ' ')]) {
                assert_eq!(
                    s.panel_point(i),
                    fresh.panel_point(i),
                    "case {case} step {step} offset {i}: {text:?}"
                );
            }
        }
    }
}
