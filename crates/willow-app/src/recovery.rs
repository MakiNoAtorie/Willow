//! Crash recovery: recovery files written and deleted off the UI thread, and applied at
//! launch.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use gpui::{App, Context, Task};
use willow_core::{Document, Recovery, RecoveryFile, launch_plan, load};

use crate::{Content, Willow, file, open_window};

/// `~/Library/Application Support/Willow/Recovery` or `%APPDATA%\Willow\Recovery`.
fn dir() -> Option<PathBuf> {
    Some(crate::prefs::app_dir()?.join("Recovery"))
}

fn file_of(dir: &Path, id: &str, ext: &str) -> PathBuf {
    dir.join(format!("{id}.{ext}"))
}

/// Deletes `<id>.willow` and `<id>.path`.
fn remove(dir: &Path, id: &str) {
    for ext in ["willow", "path"] {
        _ = fs::remove_file(file_of(dir, id, ext));
    }
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
    /// Bumped on the UI thread by every write and delete asked for; each runs only if it is
    /// still the latest when its turn comes.
    epoch: Arc<AtomicU64>,
    /// Held only by the background thread doing the file I/O, never by the UI thread; it
    /// keeps a delete from racing a write.
    io: Arc<Mutex<()>>,
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
            io: Arc::default(),
            at: None,
            timer: None,
        }
    }

    /// Runs `job` on the recovery directory and ID off the UI thread, unless something else
    /// is asked for first; the task ends once it is done.
    fn run(&self, job: impl FnOnce(&Path, &str) + Send + 'static, cx: &App) -> Task<()> {
        let mine = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let (epoch, io, id) = (self.epoch.clone(), self.io.clone(), self.id.clone());
        cx.background_executor().spawn(async move {
            let Some(dir) = dir() else { return };
            let _io = io.lock().unwrap_or_else(PoisonError::into_inner);
            if epoch.load(Ordering::SeqCst) == mine {
                job(&dir, &id);
            }
        })
    }
}

/// A recovery write waiting for its turn off the UI thread.
struct Pending {
    /// The document's file, written to `<id>.path`.
    path: Option<PathBuf>,
    /// `Recovery::Write` or `Text`.
    write: Recovery,
}

impl Pending {
    /// Writes `<id>.path` then `<id>.willow`.
    fn write(self, dir: &Path, id: &str) {
        let text = match self.write {
            Recovery::Write { document, heights } => willow_core::save(&document, &heights),
            Recovery::Text(text) => text,
            Recovery::Delete => return,
        };
        let path = self
            .path
            .map_or(String::new(), |p| p.to_string_lossy().into());
        let written = fs::create_dir_all(dir)
            .and_then(|()| fs::write(file_of(dir, id, "path"), path))
            .and_then(|()| file::write_beside(&file_of(dir, id, "willow"), text.as_bytes()));
        if let Err(e) = written {
            eprintln!("willow: recovery file not written: {e}");
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

    /// Writes the recovery file if due, or deletes it once the document is back to saved;
    /// serializing and I/O run off the UI thread.
    fn autosave(&mut self, cx: &mut Context<Self>) {
        let now = self.start.elapsed();
        if let Some(a) = &mut self.recovery {
            a.at = None;
            // ponytail: clones the document on the UI thread; share it (Arc) if that shows up
            // in frame times on very large documents.
            match self.session.take_recovery(now) {
                Some(Recovery::Delete) => a.run(remove, cx).detach(),
                Some(write) => {
                    let path = self.path.clone();
                    let pending = Pending { path, write };
                    a.run(|dir, id| pending.write(dir, id), cx).detach();
                }
                None => {}
            }
        }
        self.arm_recovery(cx);
    }

    /// Deletes this window's recovery file off the UI thread, dropping any write not yet
    /// done: after a manual save, or once a close or quit goes ahead. Await the task wherever
    /// the app may quit next.
    pub(crate) fn discard_recovery(&mut self, cx: &App) -> Task<()> {
        let Some(a) = &mut self.recovery else {
            return Task::ready(());
        };
        (a.at, a.timer) = (None, None);
        a.run(remove, cx)
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
                document_modified: path.as_deref().and_then(modified),
                recovered_at: modified(&file)?,
                path,
                id,
            })
        })
        .collect()
}

/// Opens each recovery newer than its document, marked unsaved, and deletes older ones; with
/// nothing recovered, opens one new document. Its file I/O runs on the main thread, before
/// any window opens.
pub(crate) fn launch(cx: &mut App) {
    let dir = dir();
    let plan = launch_plan(dir.as_deref().map(scan).unwrap_or_default());
    let mut opened = false;
    if let Some(dir) = &dir {
        for id in &plan.delete {
            remove(dir, id);
        }
        for r in plan.open {
            // An unreadable recovery file stays where it is.
            let bytes = fs::read(file_of(dir, &r.id, "willow")).ok();
            let Some((loaded, bytes)) = bytes.and_then(|b| Some((load(&b).ok()?, b))) else {
                continue;
            };
            open_window(Content::new(loaded, bytes, r.path, Some(r.id)), None, cx);
            opened = true;
        }
    }
    // When every recovery fails to read, a new document still opens.
    if plan.new_document || !opened {
        open_window(Content::untitled(Document::new()), None, cx);
    }
}
