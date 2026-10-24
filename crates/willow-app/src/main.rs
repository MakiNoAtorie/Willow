//! Willow's window: turns GPUI events into editor-session input and draws what the session holds.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, BoxShadow, Context, Corners, CursorStyle, DispatchPhase, ElementInputHandler,
    FocusHandle, Hsla, KeyBinding, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, ScrollEvent, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, canvas, div, point, prelude::*, px, quad, size,
};
use willow_core::{
    Button, Cursor, Document, Input, Key, Modifiers, Motion, PAPER, Rgba, Session, Shadow, Theme,
    ZoomPill,
};
mod text;
#[cfg(target_os = "windows")]
mod titlebar_windows;
#[cfg(target_os = "windows")]
use titlebar_windows::title_bar;

actions!(willow, [Quit, ResetZoom]);
// Edit menu. Cut, Copy and Paste have no handler yet, so their menu items stay disabled.
actions!(willow, [Undo, Redo, Cut, Copy, Paste]);
// Text editing; bound in the `editing` key context except F2, Enter and Esc.
actions!(
    willow,
    [
        StartEditing,
        StopEditing,
        Left,
        Right,
        WordLeft,
        WordRight,
        Up,
        Down,
        LineStart,
        LineEnd,
        Backspace,
        Delete,
        WordBackspace,
        WordDelete,
        ToggleBold,
        ToggleItalic,
    ]
);

/// Registers one handler per editing action on `$d`, each sending its `Input`.
macro_rules! editing_actions {
    ($d:expr, $cx:expr, $($action:ident => $input:expr),* $(,)?) => {
        $d $(.on_action($cx.listener(|w, _: &$action, _, cx| w.input($input, cx))))*
    };
}

/// Pixels per line for wheels that report lines (Windows already multiplies by its lines per notch).
const WHEEL_LINE: f32 = 20.0;

pub(crate) fn color(c: Rgba) -> Hsla {
    gpui::rgba(c.0).into()
}

fn mods(m: &gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: m.shift,
        ctrl: m.control,
        alt: m.alt,
        platform: m.platform,
    }
}

fn button(b: MouseButton) -> Option<Button> {
    match b {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Middle => Some(Button::Middle),
        MouseButton::Right => Some(Button::Right),
        _ => None,
    }
}

fn key(k: &gpui::Keystroke) -> Option<Key> {
    (k.key == "space").then_some(Key::Space)
}

struct Willow {
    session: Session,
    focus: FocusHandle,
    cursor: Cursor,
    /// Window position of the canvas's top-left, from the last paint.
    canvas_origin: gpui::Point<Pixels>,
    start: Instant,
}

impl Willow {
    fn input(&mut self, input: Input, cx: &mut Context<Self>) {
        let r = self.session.handle(input, self.start.elapsed());
        self.cursor = r.cursor;
        if r.redraw {
            cx.notify();
        }
    }

    /// Window position to canvas-relative screen position.
    fn local(&self, p: gpui::Point<Pixels>) -> willow_core::Point {
        let p = p - self.canvas_origin;
        willow_core::Point {
            x: p.x.into(),
            y: p.y.into(),
        }
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(button) = button(e.button) {
            let pos = self.local(e.position);
            let mods = mods(&e.modifiers);
            self.input(Input::PointerDown { pos, button, mods }, cx);
        }
    }

    fn scroll(&mut self, e: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let d = e.delta.pixel_delta(px(WHEEL_LINE));
        let delta = willow_core::Point {
            x: d.x.into(),
            y: d.y.into(),
        };
        let mods = mods(&e.modifiers);
        let pos = self.local(e.position);
        self.input(Input::Wheel { pos, delta, mods }, cx);
    }

    fn pinch(&mut self, e: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        let pos = self.local(e.position);
        self.input(
            Input::Pinch {
                pos,
                delta: e.delta,
            },
            cx,
        );
    }
}

impl Render for Willow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.session.theme();
        let now = self.start.elapsed();
        // Paint requests the next frame while the session is animating.
        self.session.tick(now);
        let pill = self.session.zoom_pill(now).map(|p| zoom_pill(theme, p));
        let entity = cx.entity();
        let canvas = canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let grabbing = entity.update(cx, |w, _| {
                    w.canvas_origin = bounds.origin;
                    let (width, height) = (bounds.size.width.into(), bounds.size.height.into());
                    let size = w.session.viewport().size;
                    if (size.x, size.y) != (width, height) {
                        w.session.resize(width, height);
                    }
                    w.cursor == Cursor::Grabbing
                });
                // Window-wide, so a pan keeps going (and ends) outside the canvas.
                let e = entity.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        e.update(cx, |w, cx| {
                            let (pos, mods) = (w.local(ev.position), mods(&ev.modifiers));
                            w.input(Input::PointerMove { pos, mods }, cx)
                        });
                    }
                });
                let e = entity.clone();
                window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble
                        && let Some(button) = button(ev.button)
                    {
                        e.update(cx, |w, cx| {
                            let (pos, mods) = (w.local(ev.position), mods(&ev.modifiers));
                            w.input(Input::PointerUp { pos, button, mods }, cx)
                        });
                    }
                });
                if grabbing {
                    window.set_window_cursor_style(CursorStyle::ClosedHand);
                }
                let w = entity.read(cx);
                let (session, now) = (&w.session, w.start.elapsed());
                if session.animating(now) {
                    window.request_animation_frame();
                }
                paint(bounds, session, now, window);
                // Text input reaches the selected shape too: typing on it starts editing.
                if session.selection().is_some() {
                    let handler = ElementInputHandler::new(bounds, entity.clone());
                    window.handle_input(&w.focus, handler, cx);
                }
            },
        )
        .size_full();
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(color(theme.canvas))
            .key_context(if self.session.editing().is_some() {
                "Willow editing"
            } else {
                "Willow"
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|w, e: &KeyDownEvent, _, cx| {
                if let Some(k) = key(&e.keystroke) {
                    w.input(Input::KeyDown(k), cx);
                }
            }))
            .on_key_up(cx.listener(|w, e: &KeyUpEvent, _, cx| {
                if let Some(k) = key(&e.keystroke) {
                    w.input(Input::KeyUp(k), cx);
                }
            }))
            .on_action(cx.listener(|w, _: &ResetZoom, _, cx| w.input(Input::ResetZoom, cx)))
            // A menu item is enabled only while its action has a handler, so register these only
            // while the core has something to undo or redo.
            .when(self.session.can_undo(), |d| {
                d.on_action(cx.listener(|w, _: &Undo, _, cx| w.input(Input::Undo, cx)))
            })
            .when(self.session.can_redo(), |d| {
                d.on_action(cx.listener(|w, _: &Redo, _, cx| w.input(Input::Redo, cx)))
            });
        let root = editing_actions!(root, cx,
            StartEditing => Input::StartEditing,
            StopEditing => Input::StopEditing,
            Left => Input::Move(Motion::Left),
            Right => Input::Move(Motion::Right),
            WordLeft => Input::Move(Motion::WordLeft),
            WordRight => Input::Move(Motion::WordRight),
            Up => Input::Move(Motion::Up),
            Down => Input::Move(Motion::Down),
            LineStart => Input::Move(Motion::LineStart),
            LineEnd => Input::Move(Motion::LineEnd),
            Backspace => Input::Delete(Motion::Left),
            Delete => Input::Delete(Motion::Right),
            WordBackspace => Input::Delete(Motion::WordLeft),
            WordDelete => Input::Delete(Motion::WordRight),
            ToggleBold => Input::ToggleBold,
            ToggleItalic => Input::ToggleItalic,
        );
        let body = div()
            .flex_1()
            .overflow_hidden()
            .cursor(match self.cursor {
                Cursor::Arrow => CursorStyle::Arrow,
                Cursor::Grab => CursorStyle::OpenHand,
                Cursor::Grabbing => CursorStyle::ClosedHand,
                Cursor::Text => CursorStyle::IBeam,
                Cursor::ResizeEw => CursorStyle::ResizeLeftRight,
                Cursor::ResizeNs => CursorStyle::ResizeUpDown,
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_scroll(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .relative()
            .child(canvas)
            .children(pill);
        // Windows: title bar above the canvas. macOS: title floats over the canvas, so paint it last.
        if cfg!(target_os = "windows") {
            root.child(title_bar(&self.session, window, cx)).child(body)
        } else {
            root.child(body).child(title_bar(&self.session, window, cx))
        }
    }
}

/// The zoom level pill, bottom center of the canvas.
fn zoom_pill(t: &Theme, pill: ZoomPill) -> impl IntoElement {
    let p = t.zoom_pill;
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(p.bottom))
        .flex()
        .justify_center()
        .opacity(pill.opacity)
        .child(
            div()
                .px(px(p.pad_x))
                .py(px(p.pad_y))
                .rounded_full()
                .bg(color(p.bg))
                .text_color(color(p.ink))
                .text_size(px(p.font_size))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(format!("{}%", pill.percent)),
        )
}

/// Canvas, dot grid, shapes with their text, and the caret.
fn paint(bounds: Bounds<Pixels>, session: &Session, now: Duration, window: &mut Window) {
    let (vp, t) = (session.viewport(), session.theme());
    let rings = session.selection_rings(now);
    let (w, h) = (bounds.size.width.into(), bounds.size.height.into());
    let frame = session.draw(
        willow_core::Point::default(),
        willow_core::Point { x: w, y: h },
    );
    window.paint_quad(gpui::fill(bounds, color(t.canvas)));
    // Screen position of canvas (0, 0), on whole pixels.
    let o = vp.to_screen(willow_core::Point::default());
    let origin = bounds.origin + point(px(o.x.round()), px(o.y.round()));

    let z = vp.zoom;
    // One dot at the center of every grid cell, as the mockups' radial-gradient tile draws it.
    let (s, r) = (t.grid_spacing * z, t.grid_dot_radius * z);
    let phase = |o: Pixels, b: Pixels| f32::from(o - b + px(s / 2.0)).rem_euclid(s);
    let (x0, y0) = (
        phase(origin.x, bounds.origin.x),
        phase(origin.y, bounds.origin.y),
    );
    let dot = color(t.grid_dot);
    let mut y = if frame.grid { y0 } else { f32::INFINITY };
    while y < f32::from(bounds.size.height) {
        let mut x = x0;
        while x < f32::from(bounds.size.width) {
            let c = bounds.origin + point(px(x), px(y));
            let b = Bounds::new(c - point(px(r), px(r)), size(px(2. * r), px(2. * r)));
            window.paint_quad(quad(
                b,
                px(r),
                dot,
                px(0.),
                gpui::transparent_black(),
                Default::default(),
            ));
            x += s;
        }
        y += s;
    }

    let radius = px(t.shape.radius * z);
    let sel = &t.selection;
    for v in &frame.shapes {
        let shape = &*v.shape;
        let b = Bounds::new(
            origin + point(px(v.x as f32 * z), px(v.y as f32 * z)),
            size(px(shape.width as f32 * z), px(v.height * z)),
        );
        let ring = rings
            .iter()
            .find(|(id, _)| **id == shape.id)
            .map_or(0.0, |r| r.1);
        if frame.shadows {
            let shadows = shadows(&t.shape.shadow, &sel.shadow, ring, z);
            window.paint_drop_shadows(b, Corners::all(radius), &shadows);
        }
        if ring > 0.0 {
            // As the mockups' box-shadow spreads: a canvas gap, then the accent ring, both growing.
            let out = (sel.gap + sel.ring) * ring * z;
            let ring_bounds = b.dilate(px(out));
            window.paint_quad(quad(
                ring_bounds,
                radius + px(out),
                color(t.canvas),
                px(sel.ring * ring * z),
                color(sel.color),
                Default::default(),
            ));
        }
        window.paint_quad(quad(
            b,
            radius,
            color(v.fill),
            px(0.),
            gpui::transparent_black(),
            Default::default(),
        ));
        if frame.text_opacity > 0.0 {
            let o = b.origin + point(px(t.shape.pad_x * z), px(t.shape.pad_y * z));
            text::paint_text(window, o, &shape.text, &v.layout, t, z, frame.text_opacity);
        }
    }
    if let Some(c) = session.caret(now)
        && c.visible
    {
        // The caret's width is screen pixels at any zoom.
        let p = origin + point(px(c.pos.x * z - t.caret.width / 2.0), px(c.pos.y * z));
        let b = Bounds::new(p, size(px(t.caret.width), px(c.height * z)));
        window.paint_quad(gpui::fill(b, color(t.caret.color)));
    }
}

/// The resting shadow eased toward the selected one by `k` (0 to 1), scaled by zoom `z`.
pub(crate) fn shadows(
    rest: &[Shadow; 2],
    selected: &[Shadow; 2],
    k: f32,
    z: f32,
) -> Vec<BoxShadow> {
    let mix = |a: f32, b: f32| a + (b - a) * k;
    let zmix = |a: f32, b: f32| mix(a, b) * z;
    rest.iter()
        .zip(selected)
        .map(|(a, b)| {
            let (ca, cb) = (a.color.0.to_be_bytes(), b.color.0.to_be_bytes());
            let c: [u8; 4] = std::array::from_fn(|i| mix(ca[i] as f32, cb[i] as f32).round() as u8);
            BoxShadow {
                color: color(Rgba(u32::from_be_bytes(c))),
                offset: point(px(zmix(a.x, b.x)), px(zmix(a.y, b.y))),
                // GPUI's blur radius is the Gaussian sigma; CSS blur is 2 sigma.
                blur_radius: px(zmix(a.blur, b.blur) / 2.0),
                spread_radius: px(zmix(a.spread, b.spread)),
                inset: false,
            }
        })
        .collect()
}

/// macOS: transparent title bar over the canvas; the traffic lights are native.
#[cfg(not(target_os = "windows"))]
fn title_bar(session: &Session, _: &mut Window, _: &mut App) -> impl IntoElement {
    let t = session.theme();
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(color(t.chrome_title))
        .child("Untitled")
}

/// The native menu bar: app menu and Edit. Items enable while their action has a handler.
#[cfg(target_os = "macos")]
fn set_menus(cx: &mut App) {
    use gpui::{Menu, MenuItem, OsAction};
    cx.set_menus([
        Menu::new("Willow").items([MenuItem::action("Quit Willow", Quit)]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
        ]),
    ]);
}

/// Seed for `--generate N`: the same document on every run and machine.
const GENERATE_SEED: u64 = 1;

fn main() {
    // `--generate N`: open a generated document of N shapes instead of a new one.
    let args: Vec<String> = std::env::args().collect();
    let doc = match args.iter().position(|a| a == "--generate") {
        Some(i) => {
            let n = args.get(i + 1).and_then(|n| n.parse().ok());
            willow_core::generate(n.expect("--generate takes a shape count"), GENERATE_SEED)
        }
        None => Document::new(),
    };
    gpui_platform::application().run(move |cx: &mut App| {
        let quit = if cfg!(target_os = "macos") {
            "cmd-q"
        } else {
            "alt-f4"
        };
        let reset_zoom = if cfg!(target_os = "macos") {
            "cmd-0"
        } else {
            "ctrl-0"
        };
        cx.bind_keys([
            KeyBinding::new(quit, Quit, None),
            KeyBinding::new(reset_zoom, ResetZoom, None),
        ]);
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("cmd-z", Undo, None),
                KeyBinding::new("cmd-shift-z", Redo, None),
                KeyBinding::new("cmd-x", Cut, None),
                KeyBinding::new("cmd-c", Copy, None),
                KeyBinding::new("cmd-v", Paste, None),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-z", Undo, None),
                KeyBinding::new("ctrl-y", Redo, None),
                KeyBinding::new("ctrl-shift-z", Redo, None),
                KeyBinding::new("ctrl-x", Cut, None),
                KeyBinding::new("ctrl-c", Copy, None),
                KeyBinding::new("ctrl-v", Paste, None),
            ]);
        }
        let ed = Some("editing");
        // Enter and Esc everywhere, so they never reach a selected shape as typed text.
        cx.bind_keys([
            KeyBinding::new("f2", StartEditing, None),
            KeyBinding::new("enter", StopEditing, None),
            KeyBinding::new("escape", StopEditing, None),
            KeyBinding::new("left", Left, ed),
            KeyBinding::new("right", Right, ed),
            KeyBinding::new("up", Up, ed),
            KeyBinding::new("down", Down, ed),
            KeyBinding::new("home", LineStart, ed),
            KeyBinding::new("end", LineEnd, ed),
            KeyBinding::new("backspace", Backspace, ed),
            KeyBinding::new("delete", Delete, ed),
        ]);
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("alt-left", WordLeft, ed),
                KeyBinding::new("alt-right", WordRight, ed),
                KeyBinding::new("cmd-left", LineStart, ed),
                KeyBinding::new("cmd-right", LineEnd, ed),
                KeyBinding::new("alt-backspace", WordBackspace, ed),
                KeyBinding::new("alt-delete", WordDelete, ed),
                KeyBinding::new("cmd-b", ToggleBold, ed),
                KeyBinding::new("cmd-i", ToggleItalic, ed),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-left", WordLeft, ed),
                KeyBinding::new("ctrl-right", WordRight, ed),
                KeyBinding::new("ctrl-backspace", WordBackspace, ed),
                KeyBinding::new("ctrl-delete", WordDelete, ed),
                KeyBinding::new("ctrl-b", ToggleBold, ed),
                KeyBinding::new("ctrl-i", ToggleItalic, ed),
            ]);
        }
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/Literata-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/Literata-Bold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/Literata-Italic.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/Literata-BoldItalic.ttf").as_slice()),
            ])
            .expect("bundled fonts");
        #[cfg(target_os = "macos")]
        set_menus(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let bounds = Bounds::centered(None, size(px(1180.), px(740.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Untitled".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(12.))),
                }),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus, cx);
                    Willow {
                        session: Session::new(
                            doc,
                            &PAPER,
                            Box::new(text::TextMeasure(window.text_system().clone())),
                        ),
                        focus,
                        cursor: Cursor::Arrow,
                        canvas_origin: Default::default(),
                        start: Instant::now(),
                    }
                })
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
