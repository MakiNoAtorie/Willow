//! The session's file state: saved or unsaved, the bytes last read or written, and when a
//! recovery file is due.

use std::io;
use std::time::Duration;

use super::Session;
use crate::{Document, Loaded, Measure, Recovery, Theme, format, load};

/// A recovery write is due this long after the last change…
const RECOVERY_QUIET: Duration = Duration::from_secs(5);
/// …or this long after the first unrecovered one, while changes keep coming.
const RECOVERY_MAX: Duration = Duration::from_secs(30);

impl Session {
    /// A session over a loaded file's document, with what had no layout placed; `bytes` are
    /// what was read. A load that needed repairs starts unsaved.
    pub fn open(
        mut loaded: Loaded,
        bytes: Vec<u8>,
        theme: &'static Theme,
        measure: Box<dyn Measure>,
    ) -> Self {
        let mut s = Session::new(Document::default(), theme, measure);
        s.place(&mut loaded);
        s.saved = (!loaded.repaired).then(|| loaded.document.clone());
        s.doc = loaded.document;
        s.bytes = bytes;
        s
    }

    /// Places what `loaded` had no layout for, measuring with this session.
    fn place(&self, loaded: &mut Loaded) {
        let height = |shape: &_| self.shape_height(shape);
        format::place(&mut loaded.document, &loaded.unplaced, self.theme, height);
    }

    /// The document matches what was last read or written (undoing back to it counts).
    pub fn is_saved(&self) -> bool {
        // ponytail: compares the whole document per call; a change counter if it shows up in profiles.
        self.saved.as_ref() == Some(&self.doc)
    }

    /// The bytes last read or written; empty for a document never read or saved.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Writes canonical text through `write`. On success the session is saved; on failure
    /// nothing changes.
    /// IME marked text is committed first.
    pub fn save(&mut self, write: impl FnOnce(&[u8]) -> io::Result<()>) -> io::Result<()> {
        let revision = self.history.revision();
        self.unmark();
        if self.history.revision() != revision {
            self.changed(self.now);
        }
        let bytes = format::save(&self.doc, &self.heights()).into_bytes();
        write(&bytes)?;
        (self.unrecovered, self.recovery_written) = (None, false);
        self.saved = Some(self.doc.clone());
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
            self.saved = None;
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
            Ok(mut loaded) if self.is_saved() => {
                self.place(&mut loaded);
                let saved = (!loaded.repaired).then(|| loaded.document.clone());
                // The same document in other bytes (line endings, indent) is no new step.
                if loaded.document != self.doc {
                    self.stop_editing();
                    self.end_gesture();
                    self.history.replace(&mut self.doc, loaded.document);
                    self.drop_missing_selection();
                }
                self.saved = saved;
            }
            _ => self.saved = None,
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

    /// What to do with the recovery file, once due at `now`: write the document, or delete
    /// the file when the document is back to saved. Taking it counts as done: the next one is
    /// due only after another change.
    pub fn take_recovery(&mut self, now: Duration) -> Option<Recovery> {
        if self.recovery_wake(now)? > Duration::ZERO {
            return None;
        }
        self.unrecovered = None;
        self.recovery_written = !self.is_saved();
        if !self.recovery_written {
            return Some(Recovery::Delete);
        }
        // The IME may still be composing: leave its marked text out rather than commit it.
        let mut document = self.doc.clone();
        if let Some(e) = &self.editing
            && let Some(m) = e.marked.clone()
            && let Some(s) = document.shapes.iter_mut().find(|s| s.id == e.id)
        {
            s.text.replace(m, "");
        }
        let heights = document
            .shapes
            .iter()
            .map(|s| self.shape_height(s))
            .collect();
        Some(Recovery::Write { document, heights })
    }
}
