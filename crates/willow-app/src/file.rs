//! File actions: New, Open, Save and Save As, with native panels and alerts.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use gpui::{AnyWindowHandle, App, AsyncApp, Context, PromptLevel, Task, Window, WindowHandle};
use willow_core::{Document, load};

use crate::{Content, Willow, open_window};

fn panel() -> rfd::AsyncFileDialog {
    rfd::AsyncFileDialog::new().add_filter("Willow", &["willow"])
}

/// The document name a title shows: the file name without `.willow`, or "Untitled".
pub(crate) fn name(path: Option<&Path>) -> String {
    (path.and_then(Path::file_stem)).map_or("Untitled".into(), |s| s.to_string_lossy().into())
}

pub(crate) fn new_file(cx: &mut App) {
    open_window(Content::untitled(Document::new()), None, cx);
}

pub(crate) fn open_file(cx: &mut App) {
    let pick = panel().pick_file();
    cx.spawn(async move |cx| {
        if let Some(file) = pick.await {
            cx.update(|cx| open_path(file.path(), cx));
        }
    })
    .detach();
}

/// The window other than `except` showing the canonical `path`.
fn showing(path: &Path, except: Option<AnyWindowHandle>, cx: &App) -> Option<WindowHandle<Willow>> {
    (cx.windows().into_iter())
        .filter(|w| Some(*w) != except)
        .filter_map(|w| w.downcast::<Willow>())
        .find(|w| w.read(cx).is_ok_and(|w| w.path.as_deref() == Some(path)))
}

/// Focuses the window already showing `path`; else reads it into the active window if that
/// is blank and untouched, or into a new window. Alerts when the file can't be opened.
fn open_path(path: &Path, cx: &mut App) {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.into());
    if let Some(open) = showing(&path, None, cx) {
        open.update(cx, |_, window, _| window.activate_window())
            .ok();
        return;
    }
    let active = cx.active_window().and_then(|w| w.downcast::<Willow>());
    let bytes = fs::read(&path).map_err(|e| e.to_string());
    let loaded = bytes.and_then(|b| load(&b).map(|l| (l, b)).map_err(|e| e.to_string()));
    let (loaded, bytes) = match loaded {
        Ok(l) => l,
        Err(reason) => {
            let message = format!("“{}” can't be opened.", name(Some(&path)));
            let window: Option<AnyWindowHandle> =
                active.map(Into::into).or(cx.windows().first().copied());
            if let Some(w) = window {
                _ = w.update(cx, |_, window, cx| alert(window, &message, &reason, cx));
            }
            return;
        }
    };
    let content = Content::new(loaded, bytes, Some(path), None);
    match active.filter(|w| w.read(cx).is_ok_and(Willow::blank)) {
        Some(w) => _ = w.update(cx, |w, window, cx| w.show(content, window, cx)),
        None => open_window(content, None, cx),
    }
}

/// A native alert with one OK button.
fn alert(window: &mut Window, message: &str, detail: &str, cx: &mut App) {
    // Nothing waits on the answer.
    drop(window.prompt(PromptLevel::Critical, message, Some(detail), &["OK"], cx));
}

impl Willow {
    /// Saves to the document's file, or asks for one first (always, with `choose`).
    pub(crate) fn save(&mut self, choose: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_tab_picker(cx);
        self.save_task(choose, window, cx).detach();
    }

    /// Like `save`; resolves to whether the document is saved afterwards (false when the
    /// panel was cancelled or the write failed).
    fn save_task(
        &mut self,
        choose: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        if let Some(path) = self.path.clone().filter(|_| !choose) {
            self.write(path, window, cx);
            return Task::ready(self.session.is_saved());
        }
        let mut panel = panel().set_file_name(format!("{}.willow", self.name()));
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            panel = panel.set_directory(dir);
        }
        let pick = panel.save_file();
        cx.spawn_in(window, async move |this, cx| {
            let Some(file) = pick.await else { return false };
            let picked = file.path();
            this.update_in(cx, |w, window, cx| {
                let Some(path) = save_as_path(picked) else {
                    let path = picked.with_extension("willow");
                    let message = format!("“{}” could not be saved.", name(Some(&path)));
                    let detail = format!(
                        "A file named “{}” already exists. Choose another name.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    alert(window, &message, &detail, cx);
                    return false;
                };
                // Two windows on one file would overwrite each other.
                let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if showing(&canon, Some(window.window_handle()), cx).is_some() {
                    let message = format!("“{}” could not be saved.", name(Some(&path)));
                    alert(window, &message, "It is open in another window.", cx);
                    return false;
                }
                w.write(path, window, cx);
                w.session.is_saved()
            })
            .unwrap_or(false)
        })
    }

    fn write(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match self.session.save(|b| write_beside(&path, b)) {
            Ok(()) => {
                self.discard_recovery(cx).detach();
                let path = fs::canonicalize(&path).unwrap_or(path);
                self.set_path(Some(path), window);
                cx.notify();
            }
            Err(e) => {
                let message = format!("“{}” could not be saved.", name(Some(&path)));
                alert(window, &message, &e.to_string(), cx);
                // Committing IME text may have made a recovery due.
                self.arm_recovery(cx);
            }
        }
    }
}

/// The file Save As writes for the panel's `picked` path: `.willow` is added when it has no
/// extension. None when that names an existing file, which the panel never asked to replace.
fn save_as_path(picked: &Path) -> Option<PathBuf> {
    if picked.extension().is_some() {
        return Some(picked.into());
    }
    let path = picked.with_extension("willow");
    (!path.exists()).then_some(path)
}

/// Writes a temporary file beside `path` and renames it over `path`, so a crash mid-save
/// never leaves a half-written file.
pub(crate) fn write_beside(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let tmp = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
    let written = fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

impl Willow {
    /// On focus: if the file's modified time changed since the last check, passes its bytes
    /// (or its absence) to the session, which decides whether to reload.
    pub(crate) fn check_disk(&mut self, cx: &mut Context<Self>) {
        let Some(path) = &self.path else { return };
        let bytes = match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(t) if Some(t) == self.mtime => return,
            Ok(t) => match fs::read(path) {
                Ok(bytes) => {
                    self.mtime = Some(t);
                    Some(bytes)
                }
                Err(_) => return,
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.mtime = None;
                None
            }
            Err(_) => return,
        };
        self.session.disk_changed(bytes.as_deref());
        self.arm_recovery(cx);
        cx.notify();
    }
}

/// Asks Save / Don't save / Cancel when the window has unsaved changes. Resolves to true when
/// the caller may discard the window (saved, or Don't save), false when the user cancelled
/// (including cancelling the save panel). The recovery file stays: the caller discards it
/// once the close or quit goes ahead, so a later Cancel keeps it.
pub(crate) async fn settle(w: WindowHandle<Willow>, cx: &mut AsyncApp) -> bool {
    let ask = w.update(cx, |this, window, cx| {
        if this.session.is_saved() {
            return None;
        }
        window.activate_window();
        let message = format!("Do you want to save changes to “{}”?", this.name());
        let detail = "Your changes will be lost if you don't save them.";
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            Some(detail),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        Some(answer)
    });
    let Ok(Some(answer)) = ask else { return true };
    match answer.await {
        Ok(0) => {
            let saving = w.update(cx, |this, window, cx| this.save_task(false, window, cx));
            match saving {
                Ok(task) => task.await,
                Err(_) => true,
            }
        }
        Ok(1) => true,
        _ => false,
    }
}

/// Set while `quit` settles windows; `close` and a second `quit` are ignored meanwhile.
static QUITTING: AtomicBool = AtomicBool::new(false);

/// Settles `w` and, unless cancelled, deletes its recovery file and closes it. Ignored while
/// `w` is already closing or a quit is running, so no window is asked twice. The checks run
/// in the task: callers may be inside an update of `w`, where `w.update` fails.
pub(crate) fn close(w: WindowHandle<Willow>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let start = w.update(cx, |w, _, _| {
            !QUITTING.load(Relaxed) && !std::mem::replace(&mut w.settling, true)
        });
        if !start.unwrap_or(false) {
            return;
        }
        if !settle(w, cx).await {
            w.update(cx, |w, _, _| w.settling = false).ok();
            return;
        }
        // Closing the last window quits, so the deletion finishes first.
        if let Ok(gone) = w.update(cx, |w, _, cx| w.discard_recovery(cx)) {
            gone.await;
        }
        w.update(cx, |_, window, _| window.remove_window()).ok();
    })
    .detach();
}

/// Quits after settling each window in turn; Cancel anywhere leaves every window open with
/// its recovery file. Recovery files are deleted only once the quit goes ahead. Ignored while
/// a quit is running or a window is closing.
pub(crate) fn quit(cx: &mut App) {
    cx.spawn(async move |cx| {
        let (windows, closing) = cx.update(|cx| {
            let windows: Vec<_> = (cx.windows().into_iter())
                .filter_map(|w| w.downcast::<Willow>())
                .collect();
            let closing = (windows.iter()).any(|w| w.read(cx).is_ok_and(|w| w.settling));
            (windows, closing)
        });
        if closing || QUITTING.swap(true, Relaxed) {
            return;
        }
        for &w in &windows {
            if !settle(w, cx).await {
                QUITTING.store(false, Relaxed);
                return;
            }
        }
        for w in windows {
            if let Ok(gone) = w.update(cx, |w, _, cx| w.discard_recovery(cx)) {
                gone.await;
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_as_adds_the_extension_only_onto_a_free_name() {
        let dir = std::env::temp_dir().join(format!("willow-save-as-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("taken.willow"), b"").unwrap();
        let picked = |n: &str| save_as_path(&dir.join(n));
        assert_eq!(picked("story"), Some(dir.join("story.willow")));
        assert_eq!(
            picked("taken"),
            None,
            "the panel never asked to replace taken.willow"
        );
        assert_eq!(picked("taken.willow"), Some(dir.join("taken.willow")));
        assert_eq!(picked("notes.txt"), Some(dir.join("notes.txt")));
        fs::remove_dir_all(&dir).unwrap();
    }
}
