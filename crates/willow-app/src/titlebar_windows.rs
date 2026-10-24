//! Windows: 34 px custom title bar; the OS hit-tests the control areas (drag, snap layouts on Max).

use crate::color;
use gpui::{Window, WindowControlArea, div, prelude::*, px};
use willow_core::Theme;

pub fn title_bar(t: &Theme, window: &Window) -> impl IntoElement {
    let hover = color(t.chrome_button_hover);
    let button = move |id: &'static str, glyph: &'static str, area| {
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family("Segoe Fluent Icons")
            .text_size(px(10.))
            .hover(move |s| s.bg(hover))
            .window_control_area(area)
            .child(glyph)
    };
    div()
        .h(px(34.))
        .w_full()
        .flex()
        .items_center()
        .text_color(color(t.ink))
        // Menu button placeholder; inert until its menu lands (#9).
        .child(
            div()
                .id("menu")
                .w(px(44.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .hover(move |s| s.bg(hover))
                .child("\u{2261}"),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color(t.chrome_title))
                .window_control_area(WindowControlArea::Drag)
                .child("Untitled"),
        )
        .child(button("min", "\u{e921}", WindowControlArea::Min))
        .child(button(
            "max",
            if window.is_maximized() {
                "\u{e923}"
            } else {
                "\u{e922}"
            },
            WindowControlArea::Max,
        ))
        .child(button("close", "\u{e8bb}", WindowControlArea::Close))
}
