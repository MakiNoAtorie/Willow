//! macOS: transparent title bar over the canvas; the traffic lights are native.

use gpui::{App, Window, div, prelude::*, px};
use willow_core::Session;

use crate::color;

pub fn title_bar(session: &Session, _: &mut Window, _: &mut App) -> impl IntoElement {
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
        .child("Untitled")
}
