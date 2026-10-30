//! Text edits reach the document: the panel text is checked off the UI thread a pause after
//! the last keystroke, or at once when it must be (`flush_text`), and applied when no line is
//! invalid.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::Session;
use crate::{Document, LineKind, Link, Loaded, ShapeId, Zone, check, format};

/// How long after the last keystroke the panel text is checked.
const PAUSE: Duration = Duration::from_millis(150);

/// Versions of the panel text: `now` counts every change to it; the others are the version
/// last checked, last saved (or loaded or regenerated) and last loaded or regenerated.
#[derive(Clone, Debug, Default)]
pub(super) struct Versions {
    pub(super) now: u64,
    pub(super) checked: u64,
    pub(super) saved: u64,
    pub(super) shown: u64,
}

/// Panel text to check: built on the UI thread by `Session::take_text_check`, `run` anywhere,
/// handed back through `Session::text_checked`.
pub struct TextCheck {
    text: String,
    version: u64,
    revision: u64,
}

impl TextCheck {
    pub fn run(self) -> TextChecked {
        TextChecked {
            loaded: check(&self.text),
            version: self.version,
            revision: self.revision,
        }
    }
}

/// A finished `TextCheck`.
pub struct TextChecked {
    loaded: Loaded,
    version: u64,
    revision: u64,
}

impl Session {
    /// The panel text changed since it was last checked, and no IME text is being composed.
    fn text_pending(&self) -> bool {
        let p = &self.panel;
        p.versions.now != p.versions.checked && p.edit.marked.is_none()
    }

    /// How long after `now` the panel text's check is due: a pause after the last keystroke.
    /// `None` while nothing is pending. The app wakes then and calls `take_text_check`.
    pub fn text_wake(&self, now: Duration) -> Option<Duration> {
        if !self.text_pending() {
            return None;
        }
        let typed = self.panel.edit.typed.unwrap_or_default();
        Some((typed + PAUSE).saturating_sub(now))
    }

    /// The panel text to check, once due at `now`. Taking it counts as checked: the next is
    /// due only after another change. The app runs it off the UI thread and hands the result
    /// to `text_checked`.
    pub fn take_text_check(&mut self, now: Duration) -> Option<TextCheck> {
        if self.text_wake(now)? > Duration::ZERO {
            return None;
        }
        self.panel.versions.checked = self.panel.versions.now;
        Some(TextCheck {
            text: self.panel.text().to_owned(),
            version: self.panel.versions.now,
            revision: self.history.revision(),
        })
    }

    /// Takes a check's result: what each line is, and the text applied to the document when
    /// no line is invalid. Dropped when the text or the document changed since it was taken.
    pub fn text_checked(&mut self, r: TextChecked) {
        if r.version == self.panel.versions.now && r.revision == self.history.revision() {
            self.text_lines_checked(r.loaded);
        }
    }

    /// Checks and applies pending panel text now, on the UI thread: when focus leaves the
    /// panel, on Undo, Redo and Save. IME text being composed is committed first.
    pub(super) fn flush_text(&mut self) {
        self.panel_unmark();
        if self.text_pending() {
            self.panel.versions.checked = self.panel.versions.now;
            self.text_lines_checked(check(self.panel.text()));
        }
    }

    /// The panel text was just loaded or regenerated: nothing in it is pending or unsaved.
    pub(super) fn text_shown(&mut self) {
        let v = &mut self.panel.versions;
        (v.checked, v.saved, v.shown) = (v.now, v.now, v.now);
    }

    /// Makes the panel text the document's canonical text now, on the UI thread, unless it
    /// holds the document as it is: typing then goes into text that has every canvas edit,
    /// even one whose regeneration hasn't landed yet.
    pub(super) fn fresh_text(&mut self) {
        if self.held_text().is_none() {
            self.panel.regenerated = None;
            if let Some(job) = self.take_regeneration() {
                self.regenerated(job.run());
            }
        }
    }

    /// Panel text was typed since it was last saved, loaded or regenerated.
    pub(super) fn text_unsaved(&self) -> bool {
        self.panel.versions.now != self.panel.versions.saved
    }

    /// The panel text as it is now was written.
    pub(super) fn text_saved(&mut self) {
        self.panel.versions.saved = self.panel.versions.now;
    }

    /// Focus leaves the panel: pending text applies, and text typed since the last
    /// regeneration is regenerated unless a line is invalid.
    pub(super) fn leave_panel(&mut self) {
        self.panel.focused = false;
        self.flush_text();
        let v = &self.panel.versions;
        if v.now != v.shown && !self.panel.lines.contains(&LineKind::Invalid) {
            self.panel.regenerated = None;
        }
    }

    /// `loaded` is the panel text as it is now: its lines, and the text applied when valid.
    fn text_lines_checked(&mut self, loaded: Loaded) {
        self.panel.lines = loaded.lines.clone();
        if !loaded.invalid() {
            self.apply_text(loaded);
        }
    }

    /// Makes the document what valid panel text `loaded` says, as one undo step (none when
    /// nothing changes). IDs stay: shapes and zones the document has keep their place in draw
    /// order, new ones go on top in text order. Shapes and zones the text gives no layout keep
    /// theirs, or are placed as a loaded file's are. Nothing is pushed and no zone grows.
    pub(super) fn apply_text(&mut self, mut loaded: Loaded) {
        let new = &mut loaded.document;
        let at: HashMap<_, _> = (self.doc.shapes.iter().enumerate())
            .map(|(i, s)| (&s.id, i))
            .collect();
        (loaded.unplaced.shapes).retain(|&(i, _)| match at.get(&new.shapes[i].id) {
            Some(&j) => {
                let (s, old) = (&mut new.shapes[i], &self.doc.shapes[j]);
                (s.x, s.y, s.width, s.extra_height) = (old.x, old.y, old.width, old.extra_height);
                false
            }
            None => true,
        });
        let zat: HashMap<_, _> = (self.doc.zones.iter().enumerate())
            .map(|(i, z)| (&z.id, i))
            .collect();
        (loaded.unplaced.zones).retain(|&i| match zat.get(&new.zones[i].id) {
            Some(&j) => {
                let (z, old) = (&mut new.zones[i], &self.doc.zones[j]);
                (z.x, z.y, z.width, z.height) = (old.x, old.y, old.width, old.height);
                false
            }
            None => true,
        });
        self.place(&mut loaded);
        let mut new = loaded.document;
        let old = &self.doc;
        // What says the same as the document stays as the document has it.
        new.shapes
            .sort_by_cached_key(|s| at.get(&s.id).copied().unwrap_or(usize::MAX));
        for s in &mut new.shapes {
            if let Some(&j) = at.get(&s.id)
                && s.text != old.shapes[j].text
                && format::text(&s.text) == format::text(&old.shapes[j].text)
            {
                s.text = old.shapes[j].text.clone();
            }
        }
        let lat: HashMap<_, _> = (old.links.iter().enumerate())
            .map(|(i, l)| (pair(l), i))
            .collect();
        let mut links: Vec<_> = (new.links.into_iter())
            .map(|l| match lat.get(&pair(&l)) {
                Some(&i) => (i, old.links[i].clone()),
                None => (usize::MAX, l),
            })
            .collect();
        links.sort_by_key(|l| l.0);
        new.links = links.into_iter().map(|l| l.1).collect();
        for z in &mut new.zones {
            if let Some(&j) = zat.get(&z.id) {
                let mat: HashMap<_, _> = (old.zones[j].members.iter().enumerate())
                    .map(|(i, m)| (m, i))
                    .collect();
                z.members
                    .sort_by_cached_key(|m| mat.get(m).copied().unwrap_or(usize::MAX));
            }
        }
        new.zones
            .sort_by_cached_key(|z| zat.get(&z.id).copied().unwrap_or(usize::MAX));
        new.zones = parents_first(&new);
        self.apply_document(new);
    }

    /// Replaces the document with `new` as one undo step, if it differs.
    fn apply_document(&mut self, new: Document) {
        if new == self.doc {
            return;
        }
        self.stop_editing();
        self.end_gesture();
        self.history.replace(&mut self.doc, new);
        self.drop_missing_selection();
        let revision = Some(self.history.revision());
        self.panel.held = revision;
        if self.panel.focused {
            self.panel.regenerated = revision;
        }
        self.changed(self.now);
    }
}

/// The IDs `l` links, either way round.
fn pair(Link(a, b): &Link) -> (&str, &str) {
    let (a, b) = (a.0.as_str(), b.0.as_str());
    (a.min(b), a.max(b))
}

/// `doc`'s zones in their order, but each after the zone holding it.
fn parents_first(doc: &Document) -> Vec<Zone> {
    let parent: HashMap<_, _> = (doc.zones.iter().enumerate())
        .flat_map(|(p, z)| z.members.iter().map(move |m| (m, p)))
        .collect();
    let (mut out, mut done) = (Vec::new(), vec![false; doc.zones.len()]);
    for i in 0..doc.zones.len() {
        // Zones nest one deep: a parent has no parent.
        for j in [parent.get(&doc.zones[i].id).copied(), Some(i)]
            .into_iter()
            .flatten()
        {
            if !std::mem::replace(&mut done[j], true) {
                out.push(doc.zones[j].clone());
            }
        }
    }
    out
}

/// What each line of `text`, the canonical text `save` wrote for `doc`, is, as `check` would
/// say, read without parsing: a regeneration's lines.
pub(super) fn canonical_lines(text: &str, doc: &Document) -> Vec<LineKind> {
    let zones: HashSet<&str> = doc.zones.iter().map(|z| z.id.0.as_str()).collect();
    (text.lines().map(str::trim_start))
        .map(|line| {
            let id = match line.strip_prefix("@layout ") {
                Some(rest) => rest.split(' ').next(),
                None if line.starts_with("zone ") => None,
                None => line.split_once(": \"").map(|(id, _)| id),
            };
            match id {
                Some(id) if !zones.contains(id) => LineKind::Shape(ShapeId(id.into())),
                _ => LineKind::Fine,
            }
        })
        .collect()
}
