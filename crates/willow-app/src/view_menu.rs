//! View menu data shared by both platforms: the Theme and Text Alignment submenus.
//! To add an entry: one line in `themes` or `alignments`, plus its action in `actions!`.

use gpui::{Action, App, actions};
use willow_core::{Align, Theme};

use crate::prefs::{self, Prefs};

// Entry actions; the ones with no effect yet are no-ops while their choice is the only one.
actions!(
    willow,
    [ThemePaper, AlignJustified, AlignLeft, AlignCentered]
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
    vec![choice("Paper", current.name == "Paper", ThemePaper)]
}

/// View > Text Alignment; `current` is the alignment in use.
pub(crate) fn alignments(current: Align) -> Vec<Choice> {
    vec![
        choice("Justified", current == Align::Justified, AlignJustified),
        choice("Left", current == Align::Left, AlignLeft),
        choice("Centered", current == Align::Centered, AlignCentered),
    ]
}

/// Makes `align` the alignment of every window: remembered, saved, and checked in the menu.
fn set_alignment(align: Align, cx: &mut App) {
    cx.global_mut::<Prefs>().set_alignment(align);
    prefs::save_later(cx);
    #[cfg(target_os = "macos")]
    crate::set_menus(&willow_core::PAPER, cx);
    cx.refresh_windows();
}

/// The entry actions. Theme has one choice so far, so its action does nothing.
pub(crate) fn register(cx: &mut App) {
    cx.on_action(|_: &ThemePaper, _| {});
    cx.on_action(|_: &AlignJustified, cx| set_alignment(Align::Justified, cx));
    cx.on_action(|_: &AlignLeft, cx| set_alignment(Align::Left, cx));
    cx.on_action(|_: &AlignCentered, cx| set_alignment(Align::Centered, cx));
}

#[cfg(test)]
mod tests {
    use super::*;
    use willow_core::PAPER;

    #[test]
    fn submenus_list_every_choice_with_the_current_one_checked() {
        let shown = |c: &[Choice]| c.iter().map(|c| (c.label, c.checked)).collect::<Vec<_>>();
        assert_eq!(shown(&themes(&PAPER)), [("Paper", true)]);
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
