//! macOS: transparent title bar over the canvas; the traffic lights are native.

use gpui::{App, Window, div, prelude::*, px};
use objc2_foundation::NSUserDefaults;
use willow_core::Session;

use crate::{color, panel};

/// Keeps AppKit from adding its own Enter Full Screen item (fn-F) under the View menu's. Call
/// before the menu is built.
pub fn one_full_screen_item() {
    let key = objc2_foundation::ns_string!("NSFullScreenMenuItemEverywhere");
    NSUserDefaults::standardUserDefaults().setBool_forKey(false, key);
}

pub fn title_bar(name: &str, session: &Session, _: &mut Window, _: &mut App) -> impl IntoElement {
    let c = session.theme().chrome;
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(c.mac_height))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(c.mac_title_size))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color(c.title))
        .child(if session.is_saved() {
            name.to_string()
        } else {
            format!("{name} — Edited")
        })
        .child(panel::toggle(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(session.theme().code.toggle.mac_right)),
            session,
        ))
}
