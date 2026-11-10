//! Helpers every core test shares, and the fake text-measuring function they all use: fixed,
//! known cluster widths.
#![allow(dead_code)]

use std::ops::Range;
use std::time::Duration;
use willow_core::{
    Color, Document, Input, Link, Measure, PAPER, Paragraph, Point, RichText, Run, Session, Shape,
    ShapeId, Theme, load,
};

pub fn id(s: &str) -> ShapeId {
    ShapeId(s.into())
}

pub fn pt(x: f32, y: f32) -> Point {
    Point { x, y }
}

pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// What the app does after every input: runs a due regeneration (off the UI thread there)
/// and hands the result back.
pub fn settle(s: &mut Session) {
    if let Some(job) = s.take_regeneration() {
        s.regenerated(job.run());
    }
}

/// A session over the file `bytes`, as the app opens one.
pub fn open(bytes: &[u8]) -> Session {
    let loaded = load(bytes).unwrap();
    Session::open(loaded, bytes.to_vec(), &PAPER, Box::new(Fake))
}

/// The bytes a save writes.
pub fn saved(s: &mut Session) -> Vec<u8> {
    let mut written = Vec::new();
    s.save(|b| {
        written = b.to_vec();
        Ok(())
    })
    .unwrap();
    written
}

pub fn saved_text(s: &mut Session) -> String {
    String::from_utf8(saved(s)).unwrap()
}

/// The first shape's text, unstyled.
pub fn plain(s: &Session) -> String {
    s.document().shapes[0].text.plain()
}

/// Types `text` at the caret.
pub fn typ(s: &mut Session, text: &str) {
    s.handle(
        Input::Text {
            text: text.into(),
            replace: None,
        },
        Duration::ZERO,
    );
}

/// Empty rose shapes `(id, x, y)` (210 x 48.5 each), linked as `links`.
pub fn doc(shapes: &[(&str, i32, i32)], links: &[(&str, &str)]) -> Document {
    Document {
        shapes: (shapes.iter())
            .map(|&(s, x, y)| Shape {
                color: Color::Rose,
                ..Shape::new(id(s), x, y)
            })
            .collect(),
        links: links.iter().map(|&(a, b)| Link(id(a), id(b))).collect(),
        ..Document::default()
    }
}

/// The bytes of `tests/golden/<name>`.
pub fn golden(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

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
