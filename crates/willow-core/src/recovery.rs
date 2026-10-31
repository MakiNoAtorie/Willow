//! The launch plan: what to do with the recovery files found at launch.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::Document;

/// What to do with a window's recovery file, once due (`Session::take_recovery`).
#[derive(Debug, Clone, PartialEq)]
pub enum Recovery {
    /// Write this document to it, with its shapes' heights and drawn positions for `save`.
    Write {
        document: Document,
        heights: Vec<f32>,
        drawn: Vec<(i32, i32)>,
    },
    /// Write this text to it: the code panel's, which holds the document.
    Text(String),
    /// Delete it: the document went back to its saved state.
    Delete,
}

/// A recovery file found at launch.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryFile {
    /// The random name shared by `<id>.willow` and `<id>.path`.
    pub id: String,
    /// The document's file; `None` for an untitled document.
    pub path: Option<PathBuf>,
    /// When the recovery file was written.
    pub recovered_at: SystemTime,
    /// When the document's file was last modified; `None` when it is missing.
    pub document_modified: Option<SystemTime>,
}

/// A recovery to open, marked unsaved, keeping its recovery file.
#[derive(Debug, Clone, PartialEq)]
pub struct Reopen {
    pub id: String,
    /// Where it saves; `None` opens it untitled.
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPlan {
    pub open: Vec<Reopen>,
    /// Recovery files older than their document.
    pub delete: Vec<String>,
    /// Nothing to recover: open one new document.
    pub new_document: bool,
}

/// Recoveries newer than their document open titled; those older are deleted; those whose
/// document is missing, or that never had one, open untitled.
pub fn launch_plan(files: Vec<RecoveryFile>) -> LaunchPlan {
    let (mut open, mut delete) = (Vec::new(), Vec::new());
    for f in files {
        match (f.path, f.document_modified) {
            (Some(_), Some(t)) if f.recovered_at < t => delete.push(f.id),
            (Some(path), Some(_)) => open.push(Reopen {
                id: f.id,
                path: Some(path),
            }),
            _ => open.push(Reopen {
                id: f.id,
                path: None,
            }),
        }
    }
    LaunchPlan {
        new_document: open.is_empty(),
        open,
        delete,
    }
}
