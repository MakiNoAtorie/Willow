//! The context menu widget: draws the core's open `Menu` in the theme's menu style, on both
//! platforms. Choosing an item or clicking outside goes back to the core as input.

use gpui::{AnyElement, Context, MouseButton, div, prelude::*, px, relative};
use willow_core::{Color, Menu, MenuAction, MenuEntry, MenuItem, Session, Shortcut};

use crate::{Input, Willow, color, shadows};

/// The menu with a click-catching backdrop over the canvas, or nothing while closed.
pub fn menu(session: &Session, cx: &mut Context<Willow>) -> Option<AnyElement> {
    let menu = session.menu()?;
    let m = session.theme().menu;
    let (w, h) = estimate(&m, menu);
    let size = session.viewport().size;
    // ponytail: size is estimated from the style, not measured; measure if rows ever wrap.
    let left = menu.at.x.min(size.x - w).max(0.0);
    let top = menu.at.y.min(size.y - h).max(0.0);
    let panel = div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .flex()
        .flex_col()
        .min_w(px(m.min_width))
        .p(px(m.padding))
        .rounded(px(m.radius))
        .bg(color(m.bg))
        .shadow(shadows(&m.shadow, &m.shadow, 0.0, 1.0))
        .text_color(color(m.ink))
        .text_size(px(m.font_size))
        .line_height(relative(m.line_height))
        .font_weight(gpui::FontWeight(m.font_weight))
        .when_some(m.font, |d, f| d.font_family(f))
        // Keep presses off the backdrop beneath.
        .occlude()
        .children(menu.entries.iter().enumerate().map(|(i, e)| {
            match e {
                MenuEntry::Item(item) => row(&m, i, item, cx).into_any_element(),
                MenuEntry::Colors { current } => colors(session, &m, i, *current, cx),
                MenuEntry::Separator => div()
                    .py(px(m.separator_pad_y))
                    .px(px(m.padding))
                    .child(
                        div()
                            .h(px(m.separator_width))
                            .w_full()
                            .bg(color(m.separator)),
                    )
                    .into_any_element(),
                MenuEntry::Heading(label, key) => div()
                    .flex()
                    .justify_between()
                    .gap(px(m.item_gap))
                    .pt(px(m.heading_pad_top))
                    .pb(px(m.heading_pad_bottom))
                    .px(px(m.item_pad_x))
                    .text_size(px(m.heading_size))
                    .opacity(m.heading_opacity)
                    .child(*label)
                    .children(key.map(shortcut))
                    .into_any_element(),
            }
        }));
    // Pinned to the corner: an absolute box with no insets sits below the canvas, clipped.
    let backdrop = div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_down(MouseButton::Left, cx.listener(Willow::mouse_down))
        .on_mouse_down(MouseButton::Middle, cx.listener(Willow::mouse_down))
        .on_mouse_down(MouseButton::Right, cx.listener(Willow::mouse_down))
        .child(panel);
    Some(backdrop.into_any_element())
}

fn row(
    m: &willow_core::MenuStyle,
    i: usize,
    item: &MenuItem,
    cx: &mut Context<Willow>,
) -> gpui::Stateful<gpui::Div> {
    let hover = color(m.hover);
    let action = item.action;
    div()
        .id(("menu-item", i))
        .flex()
        .justify_between()
        .gap(px(m.item_gap))
        .px(px(m.item_pad_x))
        .py(px(m.item_pad_y))
        .rounded(px(m.item_radius))
        .when(item.destructive, |d| d.text_color(color(m.destructive)))
        .when(item.enabled, |d| {
            d.hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |w, _, _, cx| w.choose(action, cx)))
        })
        .when(!item.enabled, |d| d.opacity(m.disabled_opacity))
        .child(item.label)
        .child(
            div()
                .opacity(m.shortcut_opacity)
                .children(item.shortcut.map(shortcut)),
        )
}

/// The color row: a swatch per picker color, the current one ringed.
fn colors(
    session: &Session,
    m: &willow_core::MenuStyle,
    row: usize,
    current: Option<Color>,
    cx: &mut Context<Willow>,
) -> AnyElement {
    let ring = color(m.ink);
    div()
        .flex()
        .gap(px(m.swatch_gap))
        .px(px(m.item_pad_x))
        .py(px(m.item_pad_y))
        .children(Color::ALL.into_iter().map(|c| {
            // The ring is a shadow outside the circle, so every circle takes the same room.
            div()
                .id(("menu-color", row * Color::ALL.len() + c as usize))
                .size(px(m.swatch))
                .flex_none()
                .rounded(px(m.swatch_radius))
                .bg(color(session.theme().picker[c as usize]))
                .border(px(m.swatch_edge))
                .border_color(ring)
                .when(current == Some(c), |d| {
                    d.shadow(vec![
                        gpui::BoxShadow {
                            color: ring,
                            offset: gpui::point(px(0.0), px(0.0)),
                            blur_radius: px(0.0),
                            inset: false,
                            spread_radius: px(m.swatch_ring_gap + m.swatch_ring),
                        },
                        gpui::BoxShadow {
                            color: color(m.bg),
                            offset: gpui::point(px(0.0), px(0.0)),
                            blur_radius: px(0.0),
                            inset: false,
                            spread_radius: px(m.swatch_ring_gap),
                        },
                    ])
                })
                .cursor_pointer()
                .on_click(cx.listener(move |w, _, _, cx| {
                    w.input(Input::MenuChoose(MenuAction::Recolor(c)), cx)
                }))
        }))
        .into_any_element()
}

fn shortcut(s: Shortcut) -> String {
    if s == Shortcut::DELETE {
        return if cfg!(target_os = "macos") {
            s.key
        } else {
            "Del"
        }
        .into();
    }
    match (cfg!(target_os = "macos"), s.shift) {
        (true, true) => format!("\u{21e7}\u{2318}{}", s.key),
        (true, false) => format!("\u{2318}{}", s.key),
        (false, true) => format!("Ctrl+Shift+{}", s.key),
        (false, false) => format!("Ctrl+{}", s.key),
    }
}

/// The panel's width.
fn width(m: &willow_core::MenuStyle, entries: &[MenuEntry]) -> f32 {
    let n = Color::ALL.len() as f32;
    let row = n * m.swatch + (n - 1.0) * m.swatch_gap + 2.0 * (m.item_pad_x + m.padding);
    match entries
        .iter()
        .any(|e| matches!(e, MenuEntry::Colors { .. }))
    {
        true => row.max(m.min_width),
        false => m.min_width,
    }
}

/// Width and height the panel will take, for keeping it inside the canvas.
fn estimate(m: &willow_core::MenuStyle, menu: &Menu) -> (f32, f32) {
    let row = m.font_size * m.line_height + 2.0 * m.item_pad_y;
    let h = (menu.entries.iter())
        .map(|e| match e {
            MenuEntry::Item(_) => row,
            MenuEntry::Separator => 2.0 * m.separator_pad_y + m.separator_width,
            MenuEntry::Heading(..) => {
                m.heading_size * m.line_height + m.heading_pad_top + m.heading_pad_bottom
            }
            MenuEntry::Colors { .. } => m.swatch + 2.0 * m.item_pad_y,
        })
        .sum::<f32>();
    (width(m, &menu.entries), h + 2.0 * m.padding)
}

#[cfg(test)]
mod tests {
    use willow_core::{PAPER, POSTER};

    use super::*;

    #[test]
    fn a_color_row_wider_than_the_minimum_widens_the_menu() {
        let colors = [MenuEntry::Colors { current: None }];
        // Seven 22 swatches, six 7 gaps, 10 and 6 padding either side.
        assert_eq!(width(&POSTER.menu, &colors), 228.0);
        assert_eq!(width(&POSTER.menu, &[MenuEntry::Separator]), 200.0);
        assert_eq!(width(&PAPER.menu, &colors), 200.0);
    }
}
