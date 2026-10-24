//! File actions: New, Open, Save and Save As, with native panels and alerts.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use gpui::{AnyWindowHandle, App, Context, PromptLevel, Window, WindowHandle};
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
    open_window(Content::new(Document::new()), None, cx);
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

/// Focuses the window already showing `path`; else reads it into the active window if that
/// is blank and untouched, or into a new window. Alerts when the file can't be opened.
fn open_path(path: &Path, cx: &mut App) {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.into());
    let windows = cx
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<Willow>());
    let showing = |w: &WindowHandle<Willow>, cx: &App| {
        w.read(cx).is_ok_and(|w| w.path.as_ref() == Some(&path))
    };
    if let Some(open) = windows.clone().find(|w| showing(w, cx)) {
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
    let content = Content {
        loaded,
        bytes,
        path: Some(path),
    };
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
        if let Some(path) = self.path.clone().filter(|_| !choose) {
            return self.write(path, window, cx);
        }
        let mut panel = panel().set_file_name(format!("{}.willow", self.name()));
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            panel = panel.set_directory(dir);
        }
        let pick = panel.save_file();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(file) = pick.await {
                let mut path = file.path().to_path_buf();
                if path.extension().is_none() {
                    path.set_extension("willow");
                }
                this.update_in(cx, |w, window, cx| w.write(path, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    fn write(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match self.session.save(|b| write_beside(&path, b)) {
            Ok(()) => {
                let path = fs::canonicalize(&path).unwrap_or(path);
                self.set_path(Some(path), window);
                cx.notify();
            }
            Err(e) => {
                let message = format!("“{}” could not be saved.", name(Some(&path)));
                alert(window, &message, &e.to_string(), cx);
            }
        }
    }
}

/// Writes a temporary file beside `path` and renames it over `path`, so a crash mid-save
/// never leaves a half-written file.
fn write_beside(path: &Path, bytes: &[u8]) -> io::Result<()> {
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
