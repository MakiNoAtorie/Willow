//! The session's file state: saved or unsaved, the bytes last read or written, and when a
//! recovery file is due.

use std::io;
use std::time::Duration;

use super::Session;
use super::panel_direction::words;
use crate::{Document, Loaded, Measure, Recovery, Theme, format, load};

/// A recovery write is due this long after the last change…
const RECOVERY_QUIET: Duration = Duration::from_secs(5);
/// …or this long after the first unrecovered one, while changes keep coming.
const RECOVERY_MAX: Duration = Duration::from_secs(30);

impl Session {
    /// A session over a loaded file's document, with what had no layout placed; `bytes` are
    /// what was read. A load that needed repairs starts unsaved, unless a line is invalid: then
    /// the panel opens on the text as read, which holds the document.
    pub fn open(
        loaded: Loaded,
        bytes: Vec<u8>,
        theme: &'static Theme,
        measure: Box<dyn Measure>,
    ) -> Self {
        let mut s = Session::new(Document::default(), theme, measure);
        s.take_file(loaded, &bytes, |s, doc| s.doc = doc);
        s.bytes = bytes;
        s
    }

    /// Makes a file's document, read from `bytes`, this session's through `put`: placed,
    /// saved unless it needed repairs (an invalid file counts as saved), and held by the
    /// panel text as read when a line is invalid.
    fn take_file(
        &mut self,
        mut loaded: Loaded,
        bytes: &[u8],
        put: impl FnOnce(&mut Self, Document),
    ) {
        let invalid = loaded.invalid();
        self.place(&mut loaded);
        let saved = (!loaded.repaired || invalid).then(|| loaded.document.clone());
        let words = words(&loaded);
        put(self, loaded.document);
        self.set_saved(saved);
        if invalid {
            self.hold_text(String::from_utf8_lossy(bytes).into(), loaded.lines, words);
        } else {
            self.release_text();
        }
        // Every shape measured and spaced before the first paint.
        self.spaced();
    }

    /// A session over a recovery file's document, read from `bytes`, as `open` makes it but
    /// marked unsaved, with nothing read from the document's file yet.
    pub fn recover(
        loaded: Loaded,
        bytes: &[u8],
        theme: &'static Theme,
        measure: Box<dyn Measure>,
    ) -> Self {
        let mut s = Session::open(loaded, bytes.to_vec(), theme, measure);
        s.set_saved(None);
        s.bytes.clear();
        s
    }

    /// Places what `loaded` had no layout for, measuring with this session.
    pub(super) fn place(&self, loaded: &mut Loaded) {
        let height = |shape: &_| self.shape_height(shape);
        format::place(&mut loaded.document, &loaded.unplaced, self.theme, height);
    }

    /// The document matches what was last read or written (undoing back to it counts), and
    /// no panel text was typed since that isn't regenerated yet.
    pub fn is_saved(&self) -> bool {
        self.same_as_saved() && !self.text_unsaved()
    }

    /// Compares the whole document, a fifth of a frame's draw on 10,000 shapes, so the title
    /// bar's read every frame keeps the answer per revision. Marked text changes the document
    /// outside the history, so composing always compares.
    fn same_as_saved(&self) -> bool {
        let revision = self.history.revision();
        let composing = (self.editing.as_ref()).is_some_and(|e| e.marked.is_some());
        match self.same_as_saved.get() {
            Some((r, same)) if r == revision && !composing => same,
            _ => {
                let same = self.saved.as_ref() == Some(&self.doc);
                if !composing {
                    self.same_as_saved.set(Some((revision, same)));
                }
                same
            }
        }
    }

    /// Replaces what the document is compared with.
    fn set_saved(&mut self, saved: Option<Document>) {
        self.saved = saved;
        self.same_as_saved.set(None);
    }

    /// The bytes last read or written; empty for a document never read or saved.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Writes the text the panel holds, else canonical text, through `write`. On success the
    /// session is saved; on failure nothing else changes.
    /// IME marked text is committed and pending panel text applied first.
    pub fn save(&mut self, write: impl FnOnce(&[u8]) -> io::Result<()>) -> io::Result<()> {
        let revision = self.history.revision();
        self.unmark();
        self.flush_text();
        if self.history.revision() != revision {
            self.changed(self.now);
        }
        let bytes = match self.held_text() {
            Some(text) => text.as_bytes().to_vec(),
            None => format::save(&self.doc, &self.heights(), &self.drawn_positions()).into_bytes(),
        };
        write(&bytes)?;
        (self.unrecovered, self.recovery_written) = (None, false);
        self.set_saved(Some(self.doc.clone()));
        self.text_saved();
        self.bytes = bytes;
        Ok(())
    }

    /// The file on disk as read when the window gains focus; `None` when it is missing.
    /// Bytes equal to the last read or write change nothing. Otherwise, with nothing unsaved,
    /// the document reloads as one undoable step (none if it loads unchanged); with unsaved
    /// changes, unreadable bytes or a missing file, the document stays and is marked unsaved,
    /// with a recovery due.
    pub fn disk_changed(&mut self, bytes: Option<&[u8]>) {
        let Some(bytes) = bytes else {
            self.set_saved(None);
            // A file that comes back is compared afresh.
            self.bytes.clear();
            self.changed(self.now);
            return;
        };
        if bytes == self.bytes {
            return;
        }
        self.bytes = bytes.to_vec();
        match load(bytes) {
            Ok(loaded) if self.is_saved() => self.take_file(loaded, bytes, |s, doc| {
                // The same document in other bytes (line endings, indent) is no new step.
                if doc != s.doc {
                    s.stop_editing();
                    s.end_gesture();
                    s.keep_typing();
                    s.history.replace(&mut s.doc, doc);
                    s.drop_missing_selection();
                    s.respace();
                }
            }),
            _ => self.set_saved(None),
        }
        self.changed(self.now);
    }

    /// Notes a change to the document at `now`. One back to the saved document (undo) leaves
    /// nothing to recover, though a recovery file already written is still due for deletion.
    pub(super) fn changed(&mut self, now: Duration) {
        if self.is_saved() && !self.recovery_written {
            self.unrecovered = None;
            return;
        }
        let first = self.unrecovered.map_or(now, |(first, _)| first);
        self.unrecovered = Some((first, now));
    }

    /// How long after `now` a recovery write is due; `None` while there is nothing to recover.
    /// The app wakes then and calls `take_recovery`.
    pub fn recovery_wake(&self, now: Duration) -> Option<Duration> {
        let (first, last) = self.unrecovered?;
        let due = (last + RECOVERY_QUIET).min(first + RECOVERY_MAX);
        Some(due.saturating_sub(now))
    }

    /// What to do with the recovery file, once due at `now`: write the document (the text the
    /// panel holds, if any), or delete the file when the document is back to saved. Taking it
    /// counts as done: the next one is due only after another change.
    pub fn take_recovery(&mut self, now: Duration) -> Option<Recovery> {
        if self.recovery_wake(now)? > Duration::ZERO {
            return None;
        }
        self.unrecovered = None;
        self.recovery_written = !self.is_saved();
        if !self.recovery_written {
            return Some(Recovery::Delete);
        }
        if let Some(text) = self.held_text() {
            return Some(Recovery::Text(text.into()));
        }
        // The IME may still be composing: leave its marked text out rather than commit it.
        let mut document = self.doc.clone();
        let mut heights = self.spaced_heights();
        if let Some(e) = &self.editing
            && let Some(m) = e.marked.clone()
        {
            if e.zone {
                if let Some(z) = document.zones.iter_mut().find(|z| z.id == e.id) {
                    z.title.replace_range(m, "");
                }
            } else if let Some(i) = document.shapes.iter().position(|s| s.id == e.id) {
                document.shapes[i].text.replace(m, "");
                heights[i] = self.shape_height(&document.shapes[i]);
            }
        }
        let drawn = self.drawn_positions();
        Some(Recovery::Write {
            document,
            heights,
            drawn,
        })
    }
}
