//! App preferences: a few `key=value` lines in `Willow/preferences.txt` next to the recovery
//! folder. One set for the whole app, shared by every window, remembered across launches,
//! never part of a document and not undoable.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gpui::{App, Global};
use willow_core::{Align, PAPER, THEMES, Theme};

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

const ALIGNMENT: &str = "text_alignment";
const THEME: &str = "theme";

/// Writes the preferences off the UI thread.
pub(crate) fn save_later(cx: &App) {
    if let Some(file) = file() {
        let prefs = cx.global::<Prefs>().clone();
        cx.background_executor()
            .spawn(async move { prefs.save(&file) })
            .detach();
    }
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

    /// The text alignment; Justified unless the file says otherwise.
    pub(crate) fn alignment(&self) -> Align {
        match self.0.get(ALIGNMENT).map(String::as_str) {
            Some("left") => Align::Left,
            Some("centered") => Align::Centered,
            _ => Align::Justified,
        }
    }

    pub(crate) fn set_alignment(&mut self, align: Align) {
        let name = match align {
            Align::Justified => "justified",
            Align::Left => "left",
            Align::Centered => "centered",
        };
        self.0.insert(ALIGNMENT.into(), name.into());
    }

    /// The theme; Paper unless the file names another.
    pub(crate) fn theme(&self) -> &'static Theme {
        let name = self.0.get(THEME).map(String::as_str);
        let named = THEMES.iter().find(|t| Some(t.name) == name);
        named.copied().unwrap_or(&PAPER)
    }

    pub(crate) fn set_theme(&mut self, theme: &Theme) {
        self.0.insert(THEME.into(), theme.name.into());
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
    fn the_alignment_survives_a_reload_and_defaults_to_justified() {
        assert_eq!(Prefs::default().alignment(), Align::Justified);
        assert_eq!(
            Prefs::parse("text_alignment=bogus").alignment(),
            Align::Justified
        );
        for a in [Align::Justified, Align::Left, Align::Centered] {
            let mut p = Prefs::default();
            p.set_alignment(a);
            assert_eq!(Prefs::parse(&p.text()).alignment(), a);
        }
    }

    #[test]
    fn the_theme_survives_a_reload_and_defaults_to_paper() {
        assert_eq!(Prefs::default().theme().name, "Paper");
        assert_eq!(Prefs::parse("theme=bogus").theme().name, "Paper");
        for t in THEMES {
            let mut p = Prefs::default();
            p.set_theme(t);
            assert_eq!(Prefs::parse(&p.text()).theme().name, t.name);
        }
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
