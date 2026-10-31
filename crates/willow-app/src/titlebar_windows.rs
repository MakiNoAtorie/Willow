//! Windows: custom title bar; the OS hit-tests the control areas (drag, snap layouts on Max).
//! The menu button opens the app menu in the theme's menu style, with Edit and View cascaded (Theme and Text Alignment a level below View).

use crate::{
    CloseWindow, Copy, Cut, NewFile, OpenFile, Paste, Quit, Redo, ResetZoom, Save, SaveAs,
    ToggleFullScreen, TogglePanel, Undo, ZoomIn, ZoomOut, color, shadows, view_menu,
};
use gpui::{
    Action, AnyElement, App, Entity, MouseButton, Window, WindowControlArea, anchored, deferred,
    div, point, prelude::*, px, relative,
};
use willow_core::{MenuStyle, Session};

/// The submenus beside the top menu.
#[derive(Clone, Copy, PartialEq)]
enum Cascade {
    Edit,
    View,
    /// Beside the View menu.
    Theme,
    Alignment,
}

impl Cascade {
    /// Whether the View menu is showing, with or without a submenu beside it.
    fn in_view(self) -> bool {
        matches!(self, Cascade::View | Cascade::Theme | Cascade::Alignment)
    }

    fn label(self) -> &'static str {
        match self {
            Cascade::Edit => "Edit",
            Cascade::View => "View",
            Cascade::Theme => "Theme",
            Cascade::Alignment => "Text Alignment",
        }
    }
}

#[derive(Default)]
struct MenuState {
    open: bool,
    /// The cascade showing.
    cascade: Option<Cascade>,
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
    let (open, cascade) = {
        let s = state.read(cx);
        (s.open, s.cascade)
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
    let menus = open.then(|| app_menu(&t.menu, &state, cascade, session));
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
                            (s.open, s.cascade) = (true, Some(Cascade::Edit));
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
        .child(crate::panel::toggle(
            div()
                .w(px(c.win_button_width))
                .h_full()
                .hover(move |s| s.bg(hover)),
            session,
        ))
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

/// The top menu with the `cascade` menu beside it, below the menu button.
fn app_menu(
    m: &MenuStyle,
    state: &Entity<MenuState>,
    cascade: Option<Cascade>,
    session: &Session,
) -> AnyElement {
    let set = |open: bool, cascade: Option<Cascade>| {
        let state = state.clone();
        move |cx: &mut App| {
            state.update(cx, |s, cx| {
                (s.open, s.cascade) = (open, cascade);
                cx.notify();
            })
        }
    };
    let close = set(false, None);
    let file_items: [Item; 4] = [
        ("New", "Ctrl+N", Box::new(NewFile)),
        ("Open…", "Ctrl+O", Box::new(OpenFile)),
        ("Save", "Ctrl+S", Box::new(Save)),
        ("Save As…", "Ctrl+Shift+S", Box::new(SaveAs)),
    ];
    // Alt+F4 closes the window (the OS sends it through `on_window_should_close`), so Exit has no key.
    let window_items: [Item; 2] = [
        ("Close", "Ctrl+W", Box::new(CloseWindow)),
        ("Exit", "", Box::new(Quit)),
    ];
    let command = |it: Item| {
        let hide = set(true, None);
        item(m, it, true, set(false, None)).on_hover(move |&h, _, cx| {
            if h {
                hide(cx)
            }
        })
    };
    // A row that shows its cascade while hovered.
    let opens = |shows: Cascade, lit: bool| {
        let show = set(true, Some(shows));
        let name = shows.label();
        row(m, name, name, "\u{203a}", true)
            .when(lit, |d| d.bg(color(m.hover)))
            .on_hover(move |&h, _, cx| {
                if h {
                    show(cx)
                }
            })
    };
    let top = (panel(m).children(file_items.map(command)))
        .child(separator(m))
        .child(opens(Cascade::Edit, cascade == Some(Cascade::Edit)))
        .child(opens(Cascade::View, cascade.is_some_and(Cascade::in_view)))
        .child(separator(m))
        .children(window_items.map(command));
    let edit_items: [Item; 5] = [
        ("Undo", "Ctrl+Z", Box::new(Undo)),
        ("Redo", "Ctrl+Y", Box::new(Redo)),
        ("Cut", "Ctrl+X", Box::new(Cut)),
        ("Copy", "Ctrl+C", Box::new(Copy)),
        ("Paste", "Ctrl+V", Box::new(Paste)),
    ];
    let choices = cascade.and_then(|shown| {
        let entries = match shown {
            Cascade::Theme => view_menu::themes(session.theme()),
            Cascade::Alignment => view_menu::alignments(session.align()),
            _ => return None,
        };
        let close = &set;
        Some(
            panel(m).children(
                entries
                    .into_iter()
                    .map(|c| choice_row(m, c, close(false, None))),
            ),
        )
    });
    let cascade = cascade.map(|shown| {
        let mut p = panel(m);
        if shown.in_view() {
            return view_panel(m, shown, &set, &opens);
        }
        for (i, it) in edit_items.into_iter().enumerate() {
            if i == 2 {
                p = p.child(separator(m));
            }
            // As on macOS, each item follows the core.
            let enabled = [
                session.can_undo(),
                session.can_redo(),
                session.can_cut(),
                session.can_copy(),
                session.can_paste(),
            ][i];
            p = p.child(item(m, it, enabled, set(false, None)));
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
                    .children(cascade)
                    .children(choices),
            ),
    )
    .into_any_element()
}

/// The View menu, whose Theme and Text Alignment rows open a submenu beside it.
fn view_panel<F: Fn(&mut App) + 'static>(
    m: &MenuStyle,
    shown: Cascade,
    set: &impl Fn(bool, Option<Cascade>) -> F,
    opens: &impl Fn(Cascade, bool) -> gpui::Stateful<gpui::Div>,
) -> gpui::Div {
    // A command shows only the View menu while hovered, closing any submenu.
    let command = |label, keys, action: Box<dyn Action>| {
        let stay = set(true, Some(Cascade::View));
        item(m, (label, keys, action), true, set(false, None)).on_hover(move |&h, _, cx| {
            if h {
                stay(cx)
            }
        })
    };
    panel(m)
        .child(command("Show Code Panel", "Ctrl+E", Box::new(TogglePanel)))
        .child(separator(m))
        .child(command("Zoom In", "Ctrl+=", Box::new(ZoomIn)))
        .child(command("Zoom Out", "Ctrl+-", Box::new(ZoomOut)))
        .child(command("Actual Size", "Ctrl+0", Box::new(ResetZoom)))
        .child(separator(m))
        .child(opens(Cascade::Theme, shown == Cascade::Theme))
        .child(opens(Cascade::Alignment, shown == Cascade::Alignment))
        .child(separator(m))
        .child(command(
            "Enter Full Screen",
            "F11",
            Box::new(ToggleFullScreen),
        ))
}

/// A checkable submenu row: a check column, the label, and the action it runs.
fn choice_row(
    m: &MenuStyle,
    c: view_menu::Choice,
    close: impl Fn(&mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let (label, action) = (c.label, c.action);
    let mark = div().child("\u{2713}").when(!c.checked, |d| d.invisible());
    let text = div().flex().gap(px(m.item_pad_x)).child(mark).child(label);
    row(m, label, text, "", true).on_click(move |_, window, cx| {
        close(cx);
        window.dispatch_action(action.boxed_clone(), cx);
    })
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
        .line_height(relative(m.line_height))
        .font_weight(gpui::FontWeight(m.font_weight))
        .when_some(m.font, |d, f| d.font_family(f))
        // Keep clicks off the canvas and title bar beneath.
        .occlude()
}

fn row(
    m: &MenuStyle,
    id: &'static str,
    label: impl IntoElement,
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
