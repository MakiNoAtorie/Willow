//! The session's file state: saved or unsaved, the bytes last read or written, and when a
//! recovery file is due.

use std::io;
use std::time::Duration;

use super::Session;
use crate::{Document, Loaded, Measure, Theme, format, load};

/// A recovery write is due this long after the last change…
const RECOVERY_QUIET: Duration = Duration::from_secs(5);
/// …or this long after the first unrecovered one, while changes keep coming.
const RECOVERY_MAX: Duration = Duration::from_secs(30);

impl Session {
    /// A session over a loaded file's document; `bytes` are what was read. A load that
    /// needed repairs starts unsaved.
    pub fn open(
        loaded: Loaded,
        bytes: Vec<u8>,
        theme: &'static Theme,
        measure: Box<dyn Measure>,
    ) -> Self {
        let mut s = Session::new(loaded.document, theme, measure);
        if loaded.repaired {
            s.saved = None;
        }
        s.bytes = bytes;
        s
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
    pub fn save(&mut self, write: impl FnOnce(&[u8]) -> io::Result<()>) -> io::Result<()> {
        let bytes = format::save(&self.doc, &self.heights()).into_bytes();
        write(&bytes)?;
        self.unrecovered = None;
        self.saved = Some(self.doc.clone());
        self.bytes = bytes;
        Ok(())
    }

    /// The file on disk as read when the window gains focus; `None` when it is missing.
    /// Bytes equal to the last read or write change nothing. Otherwise, with nothing unsaved,
    /// the document reloads as one undoable step; with unsaved changes, unreadable bytes or
    /// a missing file, the document stays and is marked unsaved.
    pub fn disk_changed(&mut self, bytes: Option<&[u8]>) {
        let Some(bytes) = bytes else {
            self.saved = None;
            // A file that comes back is compared afresh.
            self.bytes.clear();
            return;
        };
        if bytes == self.bytes {
            return;
        }
        self.bytes = bytes.to_vec();
        match load(bytes) {
            Ok(loaded) if self.is_saved() => {
                self.stop_editing();
                self.saved = (!loaded.repaired).then(|| loaded.document.clone());
                self.history.replace(&mut self.doc, loaded.document);
                self.drop_missing_selection();
            }
            _ => self.saved = None,
        }
    }

    /// Notes a change to the document at `now`. One back to the saved document (undo) leaves
    /// nothing to recover.
    pub(super) fn changed(&mut self, now: Duration) {
        if self.is_saved() {
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

    /// The document to write to the recovery file, once due at `now`. Taking it counts as
    /// written: the next one is due only after another change.
    pub fn take_recovery(&mut self, now: Duration) -> Option<Document> {
        if self.recovery_wake(now)? > Duration::ZERO {
            return None;
        }
        self.unrecovered = None;
        Some(self.doc.clone())
    }
}
