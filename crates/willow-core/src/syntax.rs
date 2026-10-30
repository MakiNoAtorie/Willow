//! Syntax coloring for the code panel: one line of the text format at a time.

use std::ops::Range;

/// What a token is, for its color. Text in no token is plain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    /// `willow`, `zone`, `color`.
    Keyword,
    /// A shape's ID before its `:`, a zone's after `zone`.
    Id,
    /// A quoted string; one left open runs to the end of the line.
    String,
    /// `below`, `above`, `right of`, `left of`.
    Direction,
    /// `@layout`.
    Layout,
}

/// The tokens of `line` (no line break), in order, as byte ranges into it.
pub(crate) fn tokens(line: &str) -> Vec<(Range<usize>, Token)> {
    let mut out = Vec::new();
    let (b, mut i, mut first) = (line.as_bytes(), 0, true);
    // The word after `zone` is its ID; `of` after `right` or `left` joins the direction.
    let mut after_zone = false;
    while i < b.len() {
        if b[i] == b' ' {
            i += 1;
            continue;
        }
        let start = i;
        if b[i] == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            out.push((start..i, Token::String));
            (first, after_zone) = (false, false);
            continue;
        }
        while i < b.len() && b[i] != b' ' && b[i] != b'"' {
            i += 1;
        }
        let word = &line[start..i];
        let token = match word {
            _ if after_zone => Some(Token::Id),
            "willow" | "zone" if first => Some(Token::Keyword),
            "@layout" if first => Some(Token::Layout),
            "color" => Some(Token::Keyword),
            "below" | "above" => Some(Token::Direction),
            "right" | "left" if line[i..].starts_with(" of") => {
                i += " of".len();
                Some(Token::Direction)
            }
            _ if first && word.ends_with(':') && word.len() > 1 => {
                out.push((start..i - 1, Token::Id));
                None
            }
            _ => None,
        };
        if let Some(t) = token {
            out.push((start..i, t));
        }
        after_zone = first && word == "zone";
        first = false;
    }
    out
}
