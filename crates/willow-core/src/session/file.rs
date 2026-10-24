//! The session's file state: saved or unsaved, and the bytes last read or written.

use std::io;

use super::Session;
use crate::{Loaded, Measure, Theme, format};

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
        let bytes = format::save(&self.doc).into_bytes();
        write(&bytes)?;
        self.saved = Some(self.doc.clone());
        self.bytes = bytes;
        Ok(())
    }
}
