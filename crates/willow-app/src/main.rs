//! Willow's window: turns GPUI events into editor-session input and draws what the session holds.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, BoxShadow, Context, Corners, CursorStyle, DispatchPhase, ElementInputHandler,
    FocusHandle, Hsla, KeyBinding, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, QuitMode, ScrollEvent, TitlebarOptions,
    Window, WindowBounds, WindowOptions, actions, canvas, div, point, prelude::*, px, quad, size,
};
use willow_core::{
    Button, Cursor, Document, Input, Key, Loaded, Modifiers, Motion, PAPER, Rgba, Session, Shadow,
    Theme, ZoomPill,
};
mod bench;
mod file;
mod text;
#[cfg(not(target_os = "windows"))]
mod titlebar_macos;
#[cfg(not(target_os = "windows"))]
use titlebar_macos::title_bar;
#[cfg(target_os = "windows")]
mod titlebar_windows;
#[cfg(target_os = "windows")]
use titlebar_windows::title_bar;

actions!(willow, [Quit, ResetZoom]);
// File menu: New and Open are app-wide; Save and Save As belong to the focused window.
actions!(willow, [NewFile, OpenFile, Save, SaveAs]);
// Edit menu. Each item enables only while its handler is registered.
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
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectUp,
        SelectDown,
        SelectLineStart,
        SelectLineEnd,
        SelectAll,
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
    /// Redraws at the caret's next blink edge; replacing it cancels the old timer.
    wake: Option<gpui::Task<()>>,
    /// Sends `Input::FocusLost` when the window deactivates.
    _activation: gpui::Subscription,
    /// Set by `--bench`.
    bench: Option<bench::Bench>,
    /// The document's file; `None` while untitled.
    path: Option<PathBuf>,
}

/// What a window shows: a document, the bytes it was read from, and its file.
pub(crate) struct Content {
    loaded: Loaded,
    bytes: Vec<u8>,
    path: Option<PathBuf>,
}

impl Content {
    fn new(document: Document) -> Self {
        let loaded = Loaded {
            document,
            repaired: false,
        };
        Content {
            loaded,
            bytes: Vec::new(),
            path: None,
        }
    }

    fn session(self, window: &Window) -> (Session, Option<PathBuf>) {
        let measure = Box::new(text::TextMeasure(window.text_system().clone()));
        let session = Session::open(self.loaded, self.bytes, &PAPER, measure);
        (session, self.path)
    }
}

impl Willow {
    fn name(&self) -> String {
        file::name(self.path.as_deref())
    }

    fn set_path(&mut self, path: Option<PathBuf>, window: &mut Window) {
        self.path = path;
        window.set_window_title(&self.name());
    }

    /// An untitled new document nobody has touched: Open reuses its window.
    fn blank(&self) -> bool {
        let s = &self.session;
        self.path.is_none()
            && s.is_saved()
            && !s.can_undo()
            && !s.can_redo()
            && *s.document() == Document::new()
    }

    /// Replaces this window's document with `content`.
    fn show(&mut self, content: Content, window: &mut Window, cx: &mut Context<Self>) {
        // Paint sizes the new session's viewport.
        let path;
        (self.session, path) = content.session(window);
        self.set_path(path, window);
        cx.notify();
    }

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

    /// Puts the selected text on the system clipboard as plain text.
    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.session.selected_text() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
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
        bench::step(self, window);
        let now = self.start.elapsed();
        // Paint requests the next frame while the session is animating.
        self.session.tick(now);
        let pill = self.session.zoom_pill(now).map(|p| zoom_pill(theme, p));
        let entity = cx.entity();
        let canvas = canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let grabbing = entity.update(cx, |w, cx| {
                    w.canvas_origin = bounds.origin;
                    let (width, height) = (bounds.size.width.into(), bounds.size.height.into());
                    let size = w.session.viewport().size;
                    if (size.x, size.y) != (width, height) {
                        w.session.resize(width, height);
                    }
                    // Frames while animating; else a timer for the caret's next blink edge.
                    let now = w.start.elapsed();
                    let animating = w.session.animating(now);
                    if animating {
                        window.request_animation_frame();
                    }
                    let wake = (!animating).then(|| w.session.next_wake(now)).flatten();
                    w.wake = wake.map(|d| {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(d).await;
                            this.update(cx, |_, cx| cx.notify()).ok();
                        })
                    });
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
            .on_action(cx.listener(|w, _: &Save, window, cx| w.save(false, window, cx)))
            .on_action(cx.listener(|w, _: &SaveAs, window, cx| w.save(true, window, cx)))
            // A menu item is enabled only while its action has a handler, so register these only
            // while the core has something to undo or redo.
            .when(self.session.can_undo(), |d| {
                d.on_action(cx.listener(|w, _: &Undo, _, cx| w.input(Input::Undo, cx)))
            })
            .when(self.session.can_redo(), |d| {
                d.on_action(cx.listener(|w, _: &Redo, _, cx| w.input(Input::Redo, cx)))
            })
            .when(self.session.can_copy(), |d| {
                d.on_action(cx.listener(|w, _: &Copy, _, cx| w.copy(cx)))
            })
            .when(self.session.can_cut(), |d| {
                d.on_action(cx.listener(|w, _: &Cut, _, cx| {
                    w.copy(cx);
                    w.input(Input::Cut, cx)
                }))
            })
            .when(self.session.can_paste(), |d| {
                d.on_action(cx.listener(|w, _: &Paste, _, cx| {
                    let text = cx.read_from_clipboard().and_then(|c| c.text());
                    if let Some(text) = text {
                        w.input(Input::Paste(text), cx);
                    }
                }))
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
            SelectLeft => Input::Select(Motion::Left),
            SelectRight => Input::Select(Motion::Right),
            SelectWordLeft => Input::Select(Motion::WordLeft),
            SelectWordRight => Input::Select(Motion::WordRight),
            SelectUp => Input::Select(Motion::Up),
            SelectDown => Input::Select(Motion::Down),
            SelectLineStart => Input::Select(Motion::LineStart),
            SelectLineEnd => Input::Select(Motion::LineEnd),
            SelectAll => Input::SelectAll,
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
        let name = self.name();
        if cfg!(target_os = "windows") {
            root.child(title_bar(&name, &self.session, window, cx))
                .child(body)
        } else {
            root.child(body)
                .child(title_bar(&name, &self.session, window, cx))
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
    let (w, h) = (bounds.size.width.into(), bounds.size.height.into());
    let frame = session.draw(
        willow_core::Point::default(),
        willow_core::Point { x: w, y: h },
        now,
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
        let ring = v.ring;
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
        // The text selection, under the edited shape's text.
        for (a, c) in &v.selection {
            let tl = origin + point(px(a.x * z), px(a.y * z));
            let br = origin + point(px(c.x * z), px(c.y * z));
            window.paint_quad(gpui::fill(Bounds::from_corners(tl, br), color(sel.text)));
        }
        if frame.text_opacity > 0.0 {
            let o = b.origin + point(px(t.shape.pad_x * z), px(t.shape.pad_y * z));
            text::paint_text(window, o, &shape.text, &v.layout, t, z, frame.text_opacity);
            if let Some(m) = v.marked.clone() {
                text::paint_marked(window, o, &v.layout, m, t, z);
            }
        }
        if let Some(c) = v.caret {
            // The caret's width is screen pixels at any zoom.
            let p = origin + point(px(c.pos.x * z - t.caret.width / 2.0), px(c.pos.y * z));
            let b = Bounds::new(p, size(px(t.caret.width), px(c.height * z)));
            window.paint_quad(gpui::fill(b, color(t.caret.color)));
        }
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

/// The native menu bar: app menu and Edit. Items enable while their action has a handler.
#[cfg(target_os = "macos")]
fn set_menus(cx: &mut App) {
    use gpui::{Menu, MenuItem, OsAction};
    cx.set_menus([
        Menu::new("Willow").items([MenuItem::action("Quit Willow", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New", NewFile),
            MenuItem::action("Open…", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Save As…", SaveAs),
        ]),
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
    // `--bench --machine NAME`: run the scripted bench on 10,000 generated shapes, then quit.
    let args: Vec<String> = std::env::args().collect();
    let arg = |flag: &str| args.iter().position(|a| a == flag).map(|i| args.get(i + 1));
    let bench = args.iter().any(|a| a == "--bench").then(|| {
        let machine = arg("--machine").flatten();
        machine.expect("--bench needs --machine NAME").clone()
    });
    let doc = match arg("--generate") {
        _ if bench.is_some() => willow_core::generate(10_000, GENERATE_SEED),
        Some(n) => {
            let n = n.and_then(|n| n.parse().ok());
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
                KeyBinding::new("cmd-n", NewFile, None),
                KeyBinding::new("cmd-o", OpenFile, None),
                KeyBinding::new("cmd-s", Save, None),
                KeyBinding::new("cmd-shift-s", SaveAs, None),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-n", NewFile, None),
                KeyBinding::new("ctrl-o", OpenFile, None),
                KeyBinding::new("ctrl-s", Save, None),
                KeyBinding::new("ctrl-shift-s", SaveAs, None),
            ]);
        }
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
            KeyBinding::new("shift-left", SelectLeft, ed),
            KeyBinding::new("shift-right", SelectRight, ed),
            KeyBinding::new("shift-up", SelectUp, ed),
            KeyBinding::new("shift-down", SelectDown, ed),
            KeyBinding::new("shift-home", SelectLineStart, ed),
            KeyBinding::new("shift-end", SelectLineEnd, ed),
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
                KeyBinding::new("alt-shift-left", SelectWordLeft, ed),
                KeyBinding::new("alt-shift-right", SelectWordRight, ed),
                KeyBinding::new("cmd-shift-left", SelectLineStart, ed),
                KeyBinding::new("cmd-shift-right", SelectLineEnd, ed),
                KeyBinding::new("cmd-a", SelectAll, ed),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-left", WordLeft, ed),
                KeyBinding::new("ctrl-right", WordRight, ed),
                KeyBinding::new("ctrl-backspace", WordBackspace, ed),
                KeyBinding::new("ctrl-delete", WordDelete, ed),
                KeyBinding::new("ctrl-b", ToggleBold, ed),
                KeyBinding::new("ctrl-i", ToggleItalic, ed),
                KeyBinding::new("ctrl-shift-left", SelectWordLeft, ed),
                KeyBinding::new("ctrl-shift-right", SelectWordRight, ed),
                KeyBinding::new("ctrl-a", SelectAll, ed),
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
        cx.on_action(|_: &Quit, cx| file::quit(cx));
        cx.on_action(|_: &NewFile, cx| file::new_file(cx));
        cx.on_action(|_: &OpenFile, cx| file::open_file(cx));
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        open_window(Content::new(doc), bench, cx);
        cx.activate(true);
    });
}

/// Opens a window showing `content`; `bench` runs the scripted bench in it.
fn open_window(content: Content, bench: Option<String>, cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1180.), px(740.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(file::name(content.path.as_deref()).into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.), px(12.))),
            }),
            ..Default::default()
        },
        |window, cx| {
            if let Some(w) = window.window_handle().downcast::<Willow>() {
                window.on_window_should_close(cx, move |_, cx| {
                    if w.read(cx).is_ok_and(|w| w.session.is_saved()) {
                        return true;
                    }
                    // Close only once the user has settled unsaved changes.
                    cx.spawn(async move |cx| {
                        if file::settle(w, cx).await {
                            w.update(cx, |_, window, _| window.remove_window()).ok();
                        }
                    })
                    .detach();
                    false
                });
            }
            cx.new(|cx| {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                if bench.is_some() {
                    cx.spawn_in(window, async move |this, cx| bench::run(this, cx).await)
                        .detach();
                }
                let _activation = cx.observe_window_activation(window, |w, window, cx| {
                    if !window.is_window_active() {
                        w.input(Input::FocusLost, cx);
                    }
                });
                let (session, path) = content.session(window);
                Willow {
                    session,
                    focus,
                    cursor: Cursor::Arrow,
                    canvas_origin: Default::default(),
                    start: Instant::now(),
                    wake: None,
                    _activation,
                    bench: bench.map(bench::Bench::new),
                    path,
                }
            })
        },
    )
    .expect("open window");
}
