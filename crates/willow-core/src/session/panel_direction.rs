//! Typed direction words: changing only a link's word (`below a` to `right of a`), with the
//! shape's `@layout` line untouched, re-places the shape one creation gap from its anchor on
//! the new side, carrying the part of its chain beyond it as one block.

use std::collections::{HashMap, HashSet};

use super::Session;
use super::push::beyond;
use crate::{Document, Loaded, Shape, ShapeId, Side, chain, side};

/// Each shape's stated anchor and side, by shape: the direction words of a text.
pub(super) type Words = HashMap<ShapeId, (ShapeId, Side)>;

/// The direction words of the text `loaded` was read from.
pub(super) fn words(loaded: &Loaded) -> Words {
    let shapes = &loaded.document.shapes;
    (loaded.sides.iter().enumerate())
        .filter_map(|(i, s)| {
            let (a, side) = (*s)?;
            Some((shapes[i].id.clone(), (shapes[a].id.clone(), side)))
        })
        .collect()
}

/// Where a shape sits and how big it is: what its `@layout` line says.
fn layout(s: &Shape) -> (i32, i32, u32, u32) {
    (s.x, s.y, s.width, s.extra_height)
}

impl Session {
    /// Re-places shapes of `new` (the document typed text makes, positions as the text or the
    /// document gives them) whose line states another side of the shape it links to than the
    /// text before did (`Panel.words`, else the side the document shows), when their layout
    /// is the document's. `sides` is each shape's stated link and side; `at` finds a shape of
    /// the document by ID.
    pub(super) fn replace_by_word(
        &self,
        new: &mut Document,
        sides: &[Option<(usize, Side)>],
        at: &HashMap<&ShapeId, usize>,
    ) {
        let old = &self.doc;
        let height = |s: &Shape| self.shape_height(s);
        // Shapes whose layout the text leaves as the document has it.
        let kept: Vec<Option<usize>> = (new.shapes.iter())
            .map(|s| {
                at.get(&s.id)
                    .copied()
                    .filter(|&j| layout(&old.shapes[j]) == layout(s))
            })
            .collect();
        let links: HashSet<(&ShapeId, &ShapeId)> = old.links.iter().map(|l| (&l.0, &l.1)).collect();
        let linked = |a, b| links.contains(&(a, b)) || links.contains(&(b, a));
        let was = |s: &Shape, a: &Shape| match self.panel.words.get(&s.id) {
            Some((anchor, side)) if *anchor == a.id => *side,
            _ => side(self.drawn(a).center(), self.drawn(s).center()),
        };
        let index: HashMap<&ShapeId, usize> = (new.shapes.iter().enumerate())
            .map(|(i, s)| (&s.id, i))
            .collect();
        let ends: Vec<(usize, usize)> = (new.links.iter())
            .filter_map(|l| Some((*index.get(&l.0)?, *index.get(&l.1)?)))
            .collect();
        for (i, stated) in sides.iter().enumerate() {
            let (Some((a, to)), Some(j)) = (*stated, kept[i]) else {
                continue;
            };
            let Some(&k) = at.get(&new.shapes[a].id) else {
                continue;
            };
            let (os, oa) = (&old.shapes[j], &old.shapes[k]);
            if !linked(&os.id, &oa.id) || was(os, oa) == to {
                continue;
            }
            let (s, anchor) = (&new.shapes[i], &new.shapes[a]);
            let (x, y) = chain::beside(
                anchor,
                height(anchor),
                s.width,
                height(s),
                to,
                self.theme.creation_gap,
            );
            let (dx, dy) = (x - s.x, y - s.y);
            for m in beyond(new.shapes.len(), &ends, i, a) {
                if kept[m].is_some() {
                    new.shapes[m].x += dx;
                    new.shapes[m].y += dy;
                }
            }
        }
    }
}
