//! View menu data shared by both platforms: the Theme and Text Alignment submenus.
//! To add an entry: one line in `themes` or `alignments`, its action in `actions!`, and its
//! handler in `register`.

use gpui::{Action, App, actions};
use willow_core::{Align, DESK, PAPER, Theme};

use crate::prefs::{self, Prefs};

actions!(
    willow,
    [
        ThemePaper,
        ThemeDesk,
        AlignJustified,
        AlignLeft,
        AlignCentered
    ]
);

/// One checkable submenu entry.
pub(crate) struct Choice {
    pub label: &'static str,
    pub checked: bool,
    pub action: Box<dyn Action>,
}

fn choice(label: &'static str, checked: bool, action: impl Action) -> Choice {
    Choice {
        label,
        checked,
        action: Box::new(action),
    }
}

/// View > Theme; `current` is the theme in use.
pub(crate) fn themes(current: &Theme) -> Vec<Choice> {
    vec![
        choice(PAPER.name, current.name == PAPER.name, ThemePaper),
        choice(DESK.name, current.name == DESK.name, ThemeDesk),
    ]
}

/// View > Text Alignment; `current` is the alignment in use.
pub(crate) fn alignments(current: Align) -> Vec<Choice> {
    vec![
        choice("Justified", current == Align::Justified, AlignJustified),
        choice("Left", current == Align::Left, AlignLeft),
        choice("Centered", current == Align::Centered, AlignCentered),
    ]
}

/// A preference just changed: saved, checked in the menu, and shown in every window.
fn changed(cx: &mut App) {
    prefs::save_later(cx);
    #[cfg(target_os = "macos")]
    crate::set_menus(cx);
    cx.refresh_windows();
}

fn set_theme(theme: &Theme, cx: &mut App) {
    cx.global_mut::<Prefs>().set_theme(theme);
    changed(cx);
}

fn set_alignment(align: Align, cx: &mut App) {
    cx.global_mut::<Prefs>().set_alignment(align);
    changed(cx);
}

/// The entry actions.
pub(crate) fn register(cx: &mut App) {
    cx.on_action(|_: &ThemePaper, cx| set_theme(&PAPER, cx));
    cx.on_action(|_: &ThemeDesk, cx| set_theme(&DESK, cx));
    cx.on_action(|_: &AlignJustified, cx| set_alignment(Align::Justified, cx));
    cx.on_action(|_: &AlignLeft, cx| set_alignment(Align::Left, cx));
    cx.on_action(|_: &AlignCentered, cx| set_alignment(Align::Centered, cx));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submenus_list_every_choice_with_the_current_one_checked() {
        let shown = |c: &[Choice]| c.iter().map(|c| (c.label, c.checked)).collect::<Vec<_>>();
        assert_eq!(shown(&themes(&PAPER)), [("Paper", true), ("Desk", false)]);
        assert_eq!(shown(&themes(&DESK)), [("Paper", false), ("Desk", true)]);
        assert_eq!(
            shown(&alignments(Align::Justified)),
            [("Justified", true), ("Left", false), ("Centered", false)]
        );
        assert_eq!(
            shown(&alignments(Align::Centered)),
            [("Justified", false), ("Left", false), ("Centered", true)]
        );
    }
}
