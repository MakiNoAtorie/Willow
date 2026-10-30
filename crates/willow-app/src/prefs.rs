//! App preferences: a few `key=value` lines in `Willow/preferences.txt` next to the recovery
//! folder. One set for the whole app, shared by every window, remembered across launches,
//! never part of a document and not undoable.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gpui::Global;

/// `~/Library/Application Support/Willow` or `%APPDATA%\Willow`.
pub(crate) fn app_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("APPDATA")?);
    #[cfg(not(windows))]
    let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
    Some(base.join("Willow"))
}

/// The preferences file.
pub(crate) fn file() -> Option<PathBuf> {
    Some(app_dir()?.join("preferences.txt"))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Prefs(BTreeMap<String, String>);

impl Global for Prefs {}

impl Prefs {
    /// The `key=value` lines of `text`; a line without `=` is skipped.
    pub(crate) fn parse(text: &str) -> Self {
        let pairs = text.lines().filter_map(|l| l.split_once('='));
        Prefs(
            pairs
                .map(|(k, v)| (k.trim().into(), v.trim().into()))
                .collect(),
        )
    }

    /// Every preference as a line, in key order.
    pub(crate) fn text(&self) -> String {
        self.0.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
    }

    /// The preferences in `path`; none when it is missing or unreadable.
    pub(crate) fn load(path: &Path) -> Self {
        Self::parse(&fs::read_to_string(path).unwrap_or_default())
    }

    /// Writes the file, folders and all. A failure only loses the preference.
    pub(crate) fn save(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            _ = fs::create_dir_all(dir);
        }
        _ = fs::write(path, self.text());
    }

    pub(crate) fn get_f32(&self, key: &str) -> Option<f32> {
        self.0.get(key)?.parse().ok()
    }

    pub(crate) fn set_f32(&mut self, key: &str, value: f32) {
        self.0.insert(key.into(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_survive_a_save_and_load() {
        let dir = std::env::temp_dir().join(format!("willow-prefs-{}", std::process::id()));
        let path = dir.join("preferences.txt");
        let mut p = Prefs::default();
        p.set_f32("panel_width", 512.5);
        p.save(&path);
        assert_eq!(Prefs::load(&path).get_f32("panel_width"), Some(512.5));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_missing_file_or_bad_line_is_ignored() {
        assert_eq!(
            Prefs::load(Path::new("/no/such/preferences.txt")),
            Prefs::default()
        );
        let p = Prefs::parse("junk\npanel_width=wide\nother = 3 \n");
        assert_eq!(p.get_f32("panel_width"), None);
        assert_eq!(p.get_f32("other"), Some(3.0));
    }
}
