//! Windows: custom title bar; the OS hit-tests the control areas (drag, snap layouts on Max).
//! The menu button opens the app menu in the theme's menu style, with the Edit menu cascaded.

use crate::{Copy, Cut, NewFile, OpenFile, Paste, Quit, Redo, Save, SaveAs, Undo, color, shadows};
use gpui::{
    Action, AnyElement, App, Entity, MouseButton, Window, WindowControlArea, anchored, deferred,
    div, point, prelude::*, px, relative,
};
use willow_core::{MenuStyle, Session};

#[derive(Default)]
struct MenuState {
    open: bool,
    /// The Edit cascade is showing.
    edit: bool,
}

pub fn title_bar(
    name: &str,
    session: &Session,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let t = session.theme();
    let c = t.chrome;
    let state = window.use_keyed_state("app-menu", cx, |_, _| MenuState::default());
    let (open, edit) = {
        let s = state.read(cx);
        (s.open, s.edit)
    };
    let hover = color(t.chrome.button_hover);
    let button = move |id: &'static str, glyph: &'static str, area| {
        div()
            .id(id)
            .w(px(c.win_button_width))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family("Segoe Fluent Icons")
            .text_size(px(c.win_glyph_size))
            .hover(move |s| s.bg(hover))
            .window_control_area(area)
            .child(glyph)
    };
    let menus = open.then(|| app_menu(&t.menu, &state, edit, session));
    let toggle = state.clone();
    div()
        .h(px(c.win_height))
        .w_full()
        .flex()
        .items_center()
        .text_color(color(t.ink))
        .child(
            div()
                .id("menu")
                .w(px(c.win_menu_button_width))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .hover(move |s| s.bg(hover))
                .when(open, |d| d.bg(hover))
                // When open, the menu's click-outside handler has already closed it.
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if !open {
                        toggle.update(cx, |s, cx| {
                            (s.open, s.edit) = (true, true);
                            cx.notify();
                        });
                    }
                })
                .child("\u{2261}"),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .text_size(px(c.win_title_size))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color(c.title))
                .window_control_area(WindowControlArea::Drag)
                .gap(px(c.win_unsaved_gap))
                .child(name.to_string())
                .when(!session.is_saved(), |d| {
                    d.child(
                        div()
                            .size(px(c.win_unsaved_dot))
                            .rounded_full()
                            .bg(color(c.title)),
                    )
                }),
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
        .children(menus)
}

/// One menu row: label, shortcut (or the submenu arrow), the action it runs.
type Item = (&'static str, &'static str, Box<dyn Action>);

/// The top menu with the Edit cascade beside it, below the menu button.
fn app_menu(m: &MenuStyle, state: &Entity<MenuState>, edit: bool, session: &Session) -> AnyElement {
    let set = |open: bool, edit: bool| {
        let state = state.clone();
        move |cx: &mut App| {
            state.update(cx, |s, cx| {
                (s.open, s.edit) = (open, edit);
                cx.notify();
            })
        }
    };
    let (show_edit, hide_edit, close) = (set(true, true), set(true, false), set(false, false));
    let file_items: [Item; 4] = [
        ("New", "Ctrl+N", Box::new(NewFile)),
        ("Open…", "Ctrl+O", Box::new(OpenFile)),
        ("Save", "Ctrl+S", Box::new(Save)),
        ("Save As…", "Ctrl+Shift+S", Box::new(SaveAs)),
    ];
    let mut top = panel(m);
    for it in file_items {
        let hide_edit = set(true, false);
        top = top.child(
            item(m, it, true, set(false, false)).on_hover(move |&h, _, cx| {
                if h {
                    hide_edit(cx)
                }
            }),
        );
    }
    let top = top
        .child(separator(m))
        .child(
            row(m, "edit", "Edit", "\u{203a}", true)
                .when(edit, |d| d.bg(color(m.hover)))
                .on_hover(move |&h, _, cx| {
                    if h {
                        show_edit(cx)
                    }
                }),
        )
        .child(separator(m))
        .child(
            item(
                m,
                ("Exit", "Alt+F4", Box::new(Quit)),
                true,
                set(false, false),
            )
            .on_hover(move |&h, _, cx| {
                if h {
                    hide_edit(cx)
                }
            }),
        );
    let edit_items: [Item; 5] = [
        ("Undo", "Ctrl+Z", Box::new(Undo)),
        ("Redo", "Ctrl+Y", Box::new(Redo)),
        ("Cut", "Ctrl+X", Box::new(Cut)),
        ("Copy", "Ctrl+C", Box::new(Copy)),
        ("Paste", "Ctrl+V", Box::new(Paste)),
    ];
    let cascade = edit.then(|| {
        let mut p = panel(m);
        for (i, it) in edit_items.into_iter().enumerate() {
            if i == 2 {
                p = p.child(separator(m));
            }
            // As on macOS: Undo and Redo follow the core; Cut, Copy and Paste wait for text selection.
            let enabled = match i {
                0 => session.can_undo(),
                1 => session.can_redo(),
                _ => false,
            };
            p = p.child(item(m, it, enabled, set(false, false)));
        }
        p
    });
    deferred(
        anchored()
            .position(point(
                px(m.left),
                px(session.theme().chrome.win_height + m.gap),
            ))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(m.gap))
                    .on_mouse_down_out(move |_, _, cx| close(cx))
                    .child(top)
                    .children(cascade),
            ),
    )
    .into_any_element()
}

fn panel(m: &MenuStyle) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .min_w(px(m.min_width))
        .p(px(m.padding))
        .rounded(px(m.radius))
        .bg(color(m.bg))
        .shadow(shadows(&m.shadow, &m.shadow, 0.0, 1.0))
        .text_color(color(m.ink))
        .text_size(px(m.font_size))
        .line_height(relative(1.2))
        .font_weight(gpui::FontWeight::MEDIUM)
        // Keep clicks off the canvas and title bar beneath.
        .occlude()
}

fn row(
    m: &MenuStyle,
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = color(m.hover);
    div()
        .id(id)
        .flex()
        .justify_between()
        .gap(px(2.0 * m.item_pad_x))
        .px(px(m.item_pad_x))
        .py(px(m.item_pad_y))
        .rounded(px(m.item_radius))
        .when(enabled, |d| d.hover(move |s| s.bg(hover)))
        .when(!enabled, |d| d.opacity(m.disabled_opacity))
        .child(label)
        .child(div().opacity(m.shortcut_opacity).child(hint))
}

/// A command row: runs its action on the focused view and closes the menu.
fn item(
    m: &MenuStyle,
    (label, keys, action): Item,
    enabled: bool,
    close: impl Fn(&mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    row(m, label, label, keys, enabled).when(enabled, |d| {
        d.on_click(move |_, window, cx| {
            close(cx);
            window.dispatch_action(action.boxed_clone(), cx);
        })
    })
}

fn separator(m: &MenuStyle) -> gpui::Div {
    div()
        .py(px(m.separator_pad_y))
        .px(px(m.padding))
        .child(div().h(px(1.)).w_full().bg(color(m.separator)))
}
