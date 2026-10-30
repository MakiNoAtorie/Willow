//! View menu data shared by both platforms: the Theme and Text Alignment submenus.
//! To add an entry: one line in `themes` or `alignments`, plus its action in `actions!`.

use gpui::{Action, App, actions};
use willow_core::Theme;

// Entry actions; the ones with no effect yet are no-ops while their choice is the only one.
actions!(willow, [ThemePaper, AlignJustified]);

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

/// View > Text Alignment. Justified is the only alignment so far.
pub(crate) fn alignments() -> Vec<Choice> {
    vec![choice("Justified", true, AlignJustified)]
}

/// The entry actions that do nothing yet, so their items stay enabled on macOS.
pub(crate) fn register(cx: &mut App) {
    cx.on_action(|_: &ThemePaper, _| {});
    cx.on_action(|_: &AlignJustified, _| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use willow_core::PAPER;

    #[test]
    fn submenus_list_paper_and_justified_checked() {
        let (t, a) = (themes(&PAPER), alignments());
        let shown = |c: &[Choice]| c.iter().map(|c| (c.label, c.checked)).collect::<Vec<_>>();
        assert_eq!(shown(&t), [("Paper", true)]);
        assert_eq!(shown(&a), [("Justified", true)]);
    }
}
