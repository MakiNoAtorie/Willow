//! Crash recovery: recovery files written off the UI thread, deleted on a manual save or
//! Don't save, and applied at launch.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use gpui::{App, Context, Task};
use willow_core::{Document, RecoveryFile, launch_plan, load};

use crate::{Content, Willow, file, open_window};

/// `~/Library/Application Support/Willow/Recovery` or `%APPDATA%\Willow\Recovery`.
fn dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("APPDATA")?);
    #[cfg(not(windows))]
    let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
    Some(base.join("Willow").join("Recovery"))
}

fn file_of(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// A fresh random-enough name: clock, process and a counter.
fn new_id() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos =
        (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)).map_or(0, |d| d.as_nanos());
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{n:x}", std::process::id())
}

/// A window's recovery file and the timer for its next write.
pub(crate) struct Autosave {
    id: String,
    /// Bumped by every write and discard; a write lands only if nothing was asked after it.
    /// Held while writing, so a discard never races a write.
    epoch: Arc<Mutex<u64>>,
    /// Session time the armed timer fires; replacing `timer` cancels the old one.
    at: Option<Duration>,
    timer: Option<Task<()>>,
}

impl Autosave {
    /// `id` names an existing recovery file (one reopened at launch).
    pub(crate) fn new(id: Option<String>) -> Self {
        Autosave {
            id: id.unwrap_or_else(new_id),
            epoch: Arc::default(),
            at: None,
            timer: None,
        }
    }

    fn bump(&self) -> u64 {
        let mut e = self.epoch.lock().unwrap_or_else(PoisonError::into_inner);
        *e += 1;
        *e
    }

    /// Deletes the recovery file; a write still in flight is dropped.
    fn discard(&self) {
        let Some(dir) = dir() else { return };
        let mut e = self.epoch.lock().unwrap_or_else(PoisonError::into_inner);
        *e += 1;
        for ext in ["willow", "path"] {
            _ = fs::remove_file(file_of(&dir, &self.id, ext));
        }
    }
}

impl Willow {
    /// Arms a timer for the session's next recovery write; call after every input.
    pub(crate) fn arm_recovery(&mut self, cx: &mut Context<Self>) {
        let now = self.start.elapsed();
        let Some(a) = &mut self.recovery else { return };
        let at = self.session.recovery_wake(now).map(|d| now + d);
        if at == a.at {
            return;
        }
        a.at = at;
        a.timer = at.map(|at| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(at - now).await;
                this.update(cx, |w, cx| w.autosave(cx)).ok();
            })
        });
    }

    /// Writes the recovery file if it is due, serializing and writing off the UI thread.
    fn autosave(&mut self, cx: &mut Context<Self>) {
        let now = self.start.elapsed();
        if let Some(a) = &mut self.recovery {
            a.at = None;
            // ponytail: clones the document on the UI thread; share it (Arc) if that shows up
            // in frame times on very large documents.
            if let (Some(doc), Some(dir)) = (self.session.take_recovery(now), dir()) {
                // Heights need the measurer, so they're taken here; `save` runs off-thread.
                let heights = self.session.heights();
                let (epoch, mine) = (a.epoch.clone(), a.bump());
                let (id, path) = (a.id.clone(), self.path.clone());
                cx.background_executor()
                    .spawn(async move { write(&dir, &id, path, (doc, heights), &epoch, mine) })
                    .detach();
            }
        }
        self.arm_recovery(cx);
    }

    /// Deletes this window's recovery file: after a manual save, Don't save, or closing saved.
    pub(crate) fn discard_recovery(&self) {
        if let Some(a) = &self.recovery {
            a.discard();
        }
    }
}

/// Writes `<id>.path` then `<id>.willow`, unless a newer write or a discard came after `mine`.
fn write(
    dir: &Path,
    id: &str,
    path: Option<PathBuf>,
    (doc, heights): (Document, Vec<f32>),
    epoch: &Mutex<u64>,
    mine: u64,
) {
    let text = willow_core::save(&doc, &heights);
    let e = epoch.lock().unwrap_or_else(PoisonError::into_inner);
    if *e != mine {
        return;
    }
    let path = path.map_or(String::new(), |p| p.to_string_lossy().into());
    let written = fs::create_dir_all(dir)
        .and_then(|()| fs::write(file_of(dir, id, "path"), path))
        .and_then(|()| file::write_beside(&file_of(dir, id, "willow"), text.as_bytes()));
    drop(e);
    if let Err(e) = written {
        eprintln!("willow: recovery file not written: {e}");
    }
}

/// The recovery files in `dir`, with their document's modified time.
fn scan(dir: &Path) -> Vec<RecoveryFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    entries
        .filter_map(|e| {
            let file = e.ok()?.path();
            if file.extension()? != "willow" {
                return None;
            }
            let id = file.file_stem()?.to_string_lossy().into_owned();
            let path = fs::read_to_string(file_of(dir, &id, "path")).ok();
            let path = path.filter(|p| !p.is_empty()).map(PathBuf::from);
            Some(RecoveryFile {
                document: path.as_deref().and_then(modified),
                recovered: modified(&file)?,
                path,
                id,
            })
        })
        .collect()
}

/// Opens each recovery newer than its document, marked unsaved, and deletes older ones; with
/// nothing recovered, opens one new document.
pub(crate) fn launch(cx: &mut App) {
    let dir = dir();
    let plan = launch_plan(dir.as_deref().map(scan).unwrap_or_default());
    let mut opened = false;
    if let Some(dir) = &dir {
        for id in plan.delete {
            for ext in ["willow", "path"] {
                _ = fs::remove_file(file_of(dir, &id, ext));
            }
        }
        for r in plan.open {
            // An unreadable recovery file stays where it is.
            let bytes = fs::read(file_of(dir, &r.id, "willow"));
            let Ok(mut loaded) = bytes.map_err(|_| ()).and_then(|b| load(&b).map_err(|_| ()))
            else {
                continue;
            };
            // Starts unsaved, like a repaired load.
            loaded.repaired = true;
            let content = Content {
                loaded,
                bytes: Vec::new(),
                path: r.path,
                recovery: Some(r.id),
            };
            open_window(content, None, cx);
            opened = true;
        }
    }
    if !opened {
        open_window(Content::new(Document::new()), None, cx);
    }
}
