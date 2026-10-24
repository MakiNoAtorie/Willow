//! The fake text-measuring function every core test uses: fixed, known cluster widths.
#![allow(dead_code)]

use std::ops::Range;
use willow_core::{Document, Measure, PAPER, Paragraph, RichText, Run, Session, Theme};

/// A session over a 1000 x 800 canvas with `doc`'s first shape centered.
pub fn session_of(doc: Document) -> Session {
    let mut s = Session::new(doc, &PAPER, Box::new(Fake));
    s.resize(1000.0, 800.0);
    s
}

/// A new document whose shape holds `s`.
pub fn doc_with(s: &str) -> Document {
    let mut doc = Document::new();
    doc.shapes[0].text = text(s);
    doc
}

/// Space 4, CJK 15, any other cluster 10 (11 in bold).
pub struct Fake;

impl Measure for Fake {
    fn advances(&self, para: &Paragraph, clusters: &[Range<usize>], _: &Theme) -> Vec<f32> {
        let text: String = para.runs.iter().map(|r| r.text.as_str()).collect();
        let bold_at = |i: usize| {
            let mut end = 0;
            para.runs.iter().any(|r| {
                end += r.text.len();
                i < end && r.bold
            })
        };
        clusters
            .iter()
            .map(|c| match text[c.clone()].chars().next() {
                Some(' ') => 4.0,
                Some(ch) if ch >= '\u{3000}' => 15.0,
                _ if bold_at(c.start) => 11.0,
                _ => 10.0,
            })
            .collect()
    }
}

/// Plain text, one paragraph per `\n`-separated part.
pub fn text(s: &str) -> RichText {
    RichText {
        paragraphs: s
            .split('\n')
            .map(|p| Paragraph {
                runs: vec![Run {
                    text: p.into(),
                    ..Run::default()
                }],
            })
            .collect(),
    }
}
