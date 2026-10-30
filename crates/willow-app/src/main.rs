//! Willow's window: turns GPUI events into editor-session input and draws what the session holds.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use gpui::{
    App, Bounds, BoxShadow, Context, Corners, CursorStyle, DispatchPhase, ElementInputHandler,
    FocusHandle, Hsla, KeyBinding, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, QuitMode,
    ScrollEvent, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, canvas, div, point,
    prelude::*, px, quad, size,
};
use willow_core::{
    Button, Cursor, Document, Input, Key, Loaded, Modifiers, Motion, PAPER, Rgba, Session, Shadow,
    Side, Theme, ZoomPill,
};
mod bench;
mod file;
mod menu;
mod panel;
mod prefs;
mod recovery;
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
// File menu: New and Open are app-wide; Close, Save and Save As belong to the focused window.
actions!(willow, [NewFile, OpenFile, CloseWindow, Save, SaveAs]);
// Edit menu. Each item enables only while its handler is registered.
actions!(willow, [Undo, Redo, Cut, Copy, Paste]);
// Delete or Backspace on the canvas, with nothing being edited.
actions!(willow, [DeleteShapes]);
// Cmd/Ctrl+D on the canvas: duplicates the selected shapes.
actions!(willow, [Duplicate]);
// Keyboard color picker; the last four are bound in the `picker` key context.
actions!(
    willow,
    [
        OpenColorPicker,
        PickerLeft,
        PickerRight,
        PickerApply,
        PickerCancel
    ]
);
// Cmd/Ctrl+Arrows move the selection; with Shift they resize the selected shape.
actions!(willow, [NudgeLeft, NudgeRight, NudgeUp, NudgeDown]);
actions!(willow, [Narrower, Wider, Shorter, Taller]);
// Tab, also while editing: the directional picker; `tab_picker_keys` takes keys while it is open.
actions!(willow, [ShowTabPicker]);
// Bare arrows on the canvas, with nothing being edited: select the next shape that way.
actions!(willow, [NavLeft, NavRight, NavUp, NavDown]);
// Cmd/Ctrl+Shift+N: a new shape beside the selected chain.
actions!(willow, [NewBeside]);
// Cmd/Ctrl+L: select the chains of the selection.
actions!(willow, [SelectChain]);
// Cmd/Ctrl+E, the title-bar toggle and View > Code Panel: open (and focus) or close the code panel.
actions!(willow, [TogglePanel]);
// Text editing; bound in the `editing` key context except F2, Enter and Esc.
actions!(
    willow,
    [
        StartEditing,
        Enter,
        Escape,
        NewLine,
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

/// Registers one handler per action on `$d`, each sending its `Input`.
macro_rules! input_actions {
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

/// Raw key-down and key-up. An arrow's key-down comes as a `Nav*` action instead; its key-up,
/// which no action reports, ends a keyboard pan.
fn key(k: &gpui::Keystroke) -> Option<Key> {
    match k.key.as_str() {
        "space" => Some(Key::Space),
        key => arrow_side(key).map(Key::Arrow),
    }
}

/// The side an arrow key's GPUI name points to.
fn arrow_side(key: &str) -> Option<Side> {
    match key {
        "left" => Some(Side::Left),
        "right" => Some(Side::Right),
        "up" => Some(Side::Top),
        "down" => Some(Side::Bottom),
        _ => None,
    }
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
    /// Lays the document out ahead of the first save; see `measure_ahead`.
    ahead: Option<gpui::Task<()>>,
    /// Sends `Input::FocusLost` when the window deactivates.
    _activation: gpui::Subscription,
    /// Sends keys to the Tab picker while it is open (`tab_picker_keys`).
    _tab_picker_keys: gpui::Subscription,
    /// Set by `--bench`.
    bench: Option<bench::Bench>,
    /// The document's file; `None` while untitled.
    path: Option<PathBuf>,
    /// The file's modified time at the last focus check.
    mtime: Option<SystemTime>,
    /// Crash recovery; off in `--bench`.
    recovery: Option<recovery::Autosave>,
    /// A close is settling this window (`file::close`).
    settling: bool,
    /// The code panel's regeneration running off the UI thread; replacing it drops the old one.
    regeneration: Option<gpui::Task<()>>,
    /// Dragging the panel's hairline: how far left of the pointer the hairline was grabbed.
    panel_drag: Option<f32>,
}

/// What a window shows: a document, the bytes it was read from, and its file.
pub(crate) struct Content {
    loaded: Loaded,
    bytes: Vec<u8>,
    path: Option<PathBuf>,
    /// The recovery file it was reopened from, at launch.
    recovery: Option<String>,
}

impl Content {
    /// `loaded` as read from `bytes` at `path`, or from recovery file `recovery`'s `bytes`.
    fn new(
        loaded: Loaded,
        bytes: Vec<u8>,
        path: Option<PathBuf>,
        recovery: Option<String>,
    ) -> Self {
        Content {
            loaded,
            bytes,
            path,
            recovery,
        }
    }

    /// A new, untitled `document`.
    fn untitled(document: Document) -> Self {
        let loaded = Loaded {
            document,
            repaired: false,
            unplaced: Default::default(),
            lines: Vec::new(),
        };
        Content::new(loaded, Vec::new(), None, None)
    }

    fn session(self, window: &Window) -> (Session, Option<PathBuf>) {
        let measure = Box::new(text::TextMeasure::new(window.text_system().clone()));
        let session = match self.recovery {
            Some(_) => Session::recover(self.loaded, &self.bytes, &PAPER, measure),
            None => Session::open(self.loaded, self.bytes, &PAPER, measure),
        };
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
        let panel = self.session.panel().open;
        let path;
        (self.session, path) = content.session(window);
        // The panel belongs to the window: still open, it shows the new document.
        if panel && !self.session.panel().open {
            self.session
                .handle(Input::TogglePanel, self.start.elapsed());
            self.regenerate(cx);
        }
        self.set_path(path, window);
        self.measure_ahead(cx);
        cx.notify();
    }

    /// Lays a just-opened document out a few hundred shapes at a time while idle, without
    /// redrawing, so the first save (which needs every shape's height) doesn't stall on it.
    fn measure_ahead(&mut self, cx: &mut Context<Self>) {
        self.ahead = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(4))
                    .await;
                let done = this.update(cx, |w, _| w.session.measure_ahead(200));
                if done.unwrap_or(true) {
                    break;
                }
            }
        }));
    }

    fn input(&mut self, input: Input, cx: &mut Context<Self>) {
        let r = self.session.handle(input, self.start.elapsed());
        self.arm_recovery(cx);
        self.regenerate(cx);
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

    /// Puts the selected text, or else the selected shapes as Willow text, on the system
    /// clipboard as plain text.
    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.session.clipboard_text() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    /// A click on an app menu item that skips the core (Copy, Save) closes the Tab picker, as
    /// every click does; the core closes it for the items it handles.
    fn close_tab_picker(&mut self, cx: &mut Context<Self>) {
        if self.session.tab_picker_open() {
            self.input(Input::CloseMenu, cx);
        }
    }

    /// Runs a context-menu item, moving clipboard text for Cut, Copy and Paste first.
    fn choose(&mut self, action: willow_core::MenuAction, cx: &mut Context<Self>) {
        use willow_core::MenuAction::{Copy, Cut, Paste};
        match action {
            Cut | Copy => self.copy(cx),
            Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    return self.input(Input::Paste(text), cx);
                }
            }
            _ => {}
        }
        self.input(Input::MenuChoose(action), cx)
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(button) = button(e.button) {
            let pos = self.local(e.position);
            let mods = mods(&e.modifiers);
            self.input(Input::PointerDown { pos, button, mods }, cx);
            if button == Button::Left && e.click_count == 2 {
                self.input(Input::DoubleClick { pos }, cx);
            }
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
        panel::apply_width(&mut self.session, window, cx);
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
                panel::drag_panel(&entity, window, cx);
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
                window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        e.update(cx, |w, cx| w.input(Input::PointerLeft, cx));
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
                // Text input reaches the selected shape too: typing on it starts editing. A
                // zone title is edited with nothing selected. None while the panel has the keys.
                let canvas_keys = !session.panel().focused;
                if canvas_keys && (!session.selection().is_empty() || session.editing().is_some()) {
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
            // The panel takes the editing keys: the core keeps them off the canvas.
            .key_context(if self.session.panel().focused {
                "Willow editing panel"
            } else if self.session.editing().is_some() {
                "Willow editing"
            } else if self.session.color_picker_open() {
                "Willow picker"
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
            .on_modifiers_changed(cx.listener(|w, _: &ModifiersChangedEvent, _, cx| {
                w.input(Input::ModifiersChanged, cx)
            }))
            .on_action(cx.listener(|w, _: &ResetZoom, _, cx| w.input(Input::ResetZoom, cx)))
            .on_action(cx.listener(|w, _: &DeleteShapes, _, cx| w.input(Input::DeleteShapes, cx)))
            .on_action(cx.listener(|w, _: &Duplicate, _, cx| w.input(Input::Duplicate, cx)))
            .on_action(cx.listener(|w, _: &ShowTabPicker, _, cx| w.input(Input::Tab, cx)))
            .on_action(cx.listener(|w, _: &SelectChain, _, cx| w.input(Input::SelectChain, cx)))
            .on_action(cx.listener(|w, _: &TogglePanel, _, cx| w.input(Input::TogglePanel, cx)))
            .on_action(
                cx.listener(|w, _: &OpenColorPicker, _, cx| w.input(Input::OpenColorPicker, cx)),
            )
            .on_action(cx.listener(|w, _: &PickerLeft, _, cx| w.input(Input::PickerStep(-1), cx)))
            .on_action(cx.listener(|w, _: &PickerRight, _, cx| w.input(Input::PickerStep(1), cx)))
            .on_action(cx.listener(|w, _: &PickerApply, _, cx| w.input(Input::PickerApply, cx)))
            .on_action(cx.listener(|w, _: &PickerCancel, _, cx| w.input(Input::CloseMenu, cx)))
            .on_action(cx.listener(|_, _: &CloseWindow, window, cx| {
                if let Some(w) = window.window_handle().downcast::<Willow>() {
                    file::close(w, cx);
                }
            }))
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
                d.on_action(cx.listener(|w, _: &Copy, _, cx| {
                    w.close_tab_picker(cx);
                    w.copy(cx)
                }))
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
        let root = input_actions!(root, cx,
            StartEditing => Input::StartEditing,
            Enter => Input::Enter,
            Escape => Input::Escape,
            NewLine => Input::NewLine,
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
            NewBeside => Input::NewBeside,
            NavLeft => Input::Navigate(Side::Left),
            NavRight => Input::Navigate(Side::Right),
            NavUp => Input::Navigate(Side::Top),
            NavDown => Input::Navigate(Side::Bottom),
        );
        let root = input_actions!(root, cx,
            NudgeLeft => Input::Nudge(Side::Left),
            NudgeRight => Input::Nudge(Side::Right),
            NudgeUp => Input::Nudge(Side::Top),
            NudgeDown => Input::Nudge(Side::Bottom),
            Narrower => Input::NudgeSize(Side::Left),
            Wider => Input::NudgeSize(Side::Right),
            Shorter => Input::NudgeSize(Side::Top),
            Taller => Input::NudgeSize(Side::Bottom),
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
                Cursor::ResizeNwse => CursorStyle::ResizeUpLeftDownRight,
                Cursor::ResizeNesw => CursorStyle::ResizeUpRightDownLeft,
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_scroll(cx.listener(Self::scroll))
            .on_pinch(cx.listener(Self::pinch))
            .relative()
            .child(canvas)
            .children(pill)
            .children(menu::menu(&self.session, cx));
        // The canvas, narrowed by the code panel on its right.
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(body)
            .children(panel::panel(&self.session, cx));
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

/// The caret `c` (canvas units) at zoom `z` and opacity `k`; its width is screen pixels at
/// any zoom.
fn paint_caret(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    c: willow_core::CaretView,
    t: &Theme,
    z: f32,
    k: f32,
) {
    let p = origin + point(px(c.pos.x * z - t.caret.width / 2.0), px(c.pos.y * z));
    let b = Bounds::new(p, size(px(t.caret.width), px(c.height * z)));
    window.paint_quad(gpui::fill(b, color(t.caret.color).opacity(k)));
}

/// Canvas, dot grid, zones, connectors, shapes with their text, and the caret.
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

    // Zones, then their titles (so a nested zone never covers its parent's), then
    // connectors, under the shapes.
    let at = |p: willow_core::Point| origin + point(px(p.x * z), px(p.y * z));
    let zt = &t.zone;
    let zone_bounds = |zone: &willow_core::Zone| {
        Bounds::new(
            at(willow_core::Point {
                x: zone.x as f32,
                y: zone.y as f32,
            }),
            size(px(zone.width as f32 * z), px(zone.height as f32 * z)),
        )
    };
    for v in &frame.zones {
        window.paint_quad(quad(
            zone_bounds(&v.zone),
            px(zt.radius * z),
            color(v.fill),
            px(zt.edge_width * z),
            color(v.edge),
            Default::default(),
        ));
    }
    // The hover ring over the edge, after every fill so an overlapping zone never hides it.
    let ringed = (frame.zones.iter()).find(|v| Some(&v.zone.id) == frame.zone_ring);
    if let Some(v) = ringed {
        window.paint_quad(quad(
            zone_bounds(&v.zone),
            px(zt.radius * z),
            gpui::transparent_black(),
            px(frame.zone_ring_width * z),
            color(t.accent),
            Default::default(),
        ));
    }
    let sel = &t.selection;
    for v in frame.zones.iter().filter(|v| v.show_title) {
        for (a, c) in &v.selection {
            window.paint_quad(gpui::fill(
                Bounds::from_corners(at(*a), at(*c)),
                color(sel.text),
            ));
        }
        text::paint_line(
            window,
            at(v.title_at),
            &v.zone.title,
            t,
            zt.title_bold(),
            v.title_size * z,
        );
        if let Some((a, c)) = v.marked {
            let b = Bounds::new(
                at(a),
                size(px((c.x - a.x) * z), px(t.caret.marked_underline)),
            );
            window.paint_quad(gpui::fill(b, color(t.ink)));
        }
        if let Some(c) = v.caret {
            paint_caret(window, origin, c, t, z, 1.0);
        }
    }
    let width = frame.connector_width * z;
    let ink = color(t.connector.color);
    let cs = &t.connector;
    let (hover_width, hover) = ((cs.hover_width * z).max(cs.min_screen), color(cs.hover));
    for c in &frame.connectors {
        let (p, q) = (at(c.from), at(c.to));
        if c.hovered {
            line(window, p, q, hover_width, hover);
        } else {
            line(window, p, q, width, ink);
        }
    }

    for v in &frame.shapes {
        // A pushed shape paints off its place while it slides there.
        let origin = origin + point(px(v.slide.x * z), px(v.slide.y * z));
        // A shape being created paints faded (`k`), offset and scaled around its center.
        let (origin, z, k) = match v.appear {
            Some(a) => {
                let c = (v.x as f32 + v.shape.width as f32 / 2.0) * (1.0 - a.scale);
                let d = (v.y as f32 + v.height / 2.0) * (1.0 - a.scale) + a.dy;
                (origin + point(px(c * z), px(d * z)), z * a.scale, a.opacity)
            }
            None => (origin, z, 1.0),
        };
        let radius = px(t.shape.radius * z);
        let shape = &*v.shape;
        let b = Bounds::new(
            origin + point(px(v.x as f32 * z), px(v.y as f32 * z)),
            size(px(shape.width as f32 * z), px(v.height * z)),
        );
        let ring = v.ring;
        if frame.shadows {
            let mut shadows = shadows(&t.shape.shadow, &sel.shadow, ring, z);
            shadows
                .iter_mut()
                .for_each(|s| s.color = s.color.opacity(k));
            window.paint_drop_shadows(b, Corners::all(radius), &shadows);
        }
        if ring > 0.0 {
            // As the mockups' box-shadow spreads: a canvas gap, then the accent ring, both growing.
            let out = (sel.gap + sel.ring) * ring * z;
            let ring_bounds = b.dilate(px(out));
            window.paint_quad(quad(
                ring_bounds,
                radius + px(out),
                color(t.canvas).opacity(k),
                px(sel.ring * ring * z),
                color(sel.color).opacity(k),
                Default::default(),
            ));
        }
        window.paint_quad(quad(
            b,
            radius,
            color(v.fill).opacity(k),
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
            text::paint_text(
                window,
                o,
                &shape.text,
                &v.layout,
                t,
                z,
                frame.text_opacity * k,
            );
            if let Some(m) = v.marked.clone() {
                text::paint_marked(window, o, &v.layout, m, t, z);
            }
        }
        if let Some(c) = v.caret {
            paint_caret(window, origin, c, t, z, k);
        }
    }

    // A link drag's band over every shape, in the hovered connector's style.
    if let Some(l) = frame.link_drag {
        line(window, at(l.from), at(l.to), hover_width, hover);
    }

    // Midpoints over every shape, constant on screen: the stroke sits outside `size`.
    let m = &t.midpoint;
    let (d, sh) = (m.size + 2.0 * m.stroke_width, m.shadow);
    let shadow = [BoxShadow {
        color: color(sh.color),
        offset: point(px(sh.x), px(sh.y)),
        blur_radius: px(sh.blur / 2.0),
        spread_radius: px(sh.spread),
        inset: false,
    }];
    for &p in &frame.midpoints {
        let b = Bounds::new(at(p) - point(px(d / 2.0), px(d / 2.0)), size(px(d), px(d)));
        window.paint_drop_shadows(b, Corners::all(px(d / 2.0)), &shadow);
        let (fill, stroke) = (color(m.fill), color(m.stroke));
        let r = px(d / 2.0);
        window.paint_quad(quad(
            b,
            r,
            fill,
            px(m.stroke_width),
            stroke,
            Default::default(),
        ));
    }
    // The marquee, on top; its border and radius are screen pixels at any zoom.
    if let Some((a, c)) = frame.marquee {
        let snap = |p: willow_core::Point| {
            let p = at(p);
            point(px(f32::from(p.x).round()), px(f32::from(p.y).round()))
        };
        window.paint_quad(quad(
            Bounds::from_corners(snap(a), snap(c)),
            px(sel.marquee_radius),
            color(sel.marquee_fill),
            px(sel.marquee_width),
            color(sel.marquee),
            Default::default(),
        ));
    }
    let picker: Vec<_> = (frame.tab_picker.iter())
        .map(|&(s, p)| (s, at(p)))
        .collect();
    paint_tab_picker(window, &t.tab_picker, &picker);
}

/// While the Tab picker is open in `window`, every key goes to it before any binding or text
/// input: a bare arrow picks that side, anything else closes it (as Esc).
fn tab_picker_keys(cx: &mut Context<Willow>, window: &Window) -> gpui::Subscription {
    let (this, handle) = (cx.entity().downgrade(), window.window_handle());
    cx.intercept_keystrokes(move |e, window, cx| {
        let Some(this) = this.upgrade().filter(|_| window.window_handle() == handle) else {
            return;
        };
        if !this.read(cx).session.tab_picker_open() {
            return;
        }
        let k = &e.keystroke;
        let side = arrow_side(&k.key).filter(|_| !k.modifiers.modified());
        let input = side.map_or(Input::Escape, Input::PickSide);
        this.update(cx, |w, cx| w.input(input, cx));
        cx.stop_propagation();
    })
}

/// The Tab picker over every shape: a dashed stub out of each edge midpoint and a keycap with
/// that side's arrow, constant on screen.
fn paint_tab_picker(
    window: &mut Window,
    t: &willow_core::TabPickerStyle,
    picker: &[(Side, gpui::Point<Pixels>)],
) {
    let shadow = t.shadow.map(|s| BoxShadow {
        color: color(s.color),
        offset: point(px(s.x), px(s.y)),
        blur_radius: px(s.blur / 2.0),
        spread_radius: px(s.spread),
        inset: false,
    });
    for &(side, m) in picker {
        let (dx, dy) = side.unit();
        let (dx, dy) = (dx as f32, dy as f32);
        let along = |d: f32| m + point(px(dx * d), px(dy * d));
        let mut d = 0.0;
        while d < t.stub_length {
            let end = (d + t.dash).min(t.stub_length);
            line(window, along(d), along(end), t.stub_width, color(t.stub));
            d += 2.0 * t.dash;
        }
        let c = along(t.offset);
        let half = point(px(t.size / 2.0), px(t.size / 2.0));
        let b = Bounds::new(c - half, size(px(t.size), px(t.size)));
        window.paint_drop_shadows(b, Corners::all(px(t.radius)), &shadow);
        window.paint_quad(gpui::fill(b, color(t.fill)).corner_radii(px(t.radius)));
        // The arrow: a shaft and two wings at its tip.
        let (g, ink) = (t.glyph_size / 2.0, color(t.glyph));
        let at = |a: f32, b: f32| c + point(px(dx * a - dy * b), px(dy * a + dx * b));
        line(window, at(-g, 0.0), at(g, 0.0), t.glyph_width, ink);
        for w in [-1.0, 1.0] {
            line(
                window,
                at(g * 0.2, w * g * 0.8),
                at(g, 0.0),
                t.glyph_width,
                ink,
            );
        }
    }
}

/// A straight line from `p` to `q`, `width` screen pixels wide.
fn line(
    window: &mut Window,
    p: gpui::Point<Pixels>,
    q: gpui::Point<Pixels>,
    width: f32,
    ink: Hsla,
) {
    let (dx, dy) = (f32::from(q.x - p.x).abs(), f32::from(q.y - p.y).abs());
    if dx.max(dy) < 1.0 {
        // Under a pixel long: nothing to see.
        return;
    }
    if dx < 1.0 || dy < 1.0 {
        // Within a pixel of straight across or down: a quad is far cheaper than a path.
        let half = point(px(width / 2.0), px(width / 2.0));
        let (lo, hi) = (p.min(&q) - half, p.max(&q) + half);
        window.paint_quad(gpui::fill(Bounds::from_corners(lo, hi), ink));
        return;
    }
    let mut path = gpui::PathBuilder::stroke(px(width));
    path.move_to(p);
    path.line_to(q);
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
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
            MenuItem::action("Close", CloseWindow),
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
        Menu::new("View").items([MenuItem::action("Code Panel", TogglePanel)]),
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
    let recover = bench.is_none() && arg("--generate").is_none();
    let doc = match arg("--generate") {
        _ if bench.is_some() => willow_core::generate(10_000, GENERATE_SEED),
        Some(n) => {
            let n = n.and_then(|n| n.parse().ok());
            willow_core::generate(n.expect("--generate takes a shape count"), GENERATE_SEED)
        }
        None => Document::new(),
    };
    gpui_platform::application().run(move |cx: &mut App| {
        let reset_zoom = if cfg!(target_os = "macos") {
            "cmd-0"
        } else {
            "ctrl-0"
        };
        cx.bind_keys([KeyBinding::new(reset_zoom, ResetZoom, None)]);
        let toggle_panel = if cfg!(target_os = "macos") {
            "cmd-e"
        } else {
            "ctrl-e"
        };
        cx.bind_keys([KeyBinding::new(toggle_panel, TogglePanel, None)]);
        // Windows has no Quit key: Alt+F4 reaches the OS, which closes the window through
        // `on_window_should_close`.
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-w", CloseWindow, None),
                KeyBinding::new("cmd-n", NewFile, None),
                KeyBinding::new("cmd-o", OpenFile, None),
                KeyBinding::new("cmd-s", Save, None),
                KeyBinding::new("cmd-shift-s", SaveAs, None),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-w", CloseWindow, None),
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
        // Canvas commands: outside editing, where these keys edit text or move the caret.
        let canvas = Some("!editing");
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("cmd-left", NudgeLeft, canvas),
                KeyBinding::new("cmd-right", NudgeRight, canvas),
                KeyBinding::new("cmd-up", NudgeUp, canvas),
                KeyBinding::new("cmd-down", NudgeDown, canvas),
                KeyBinding::new("cmd-shift-left", Narrower, canvas),
                KeyBinding::new("cmd-shift-right", Wider, canvas),
                KeyBinding::new("cmd-shift-up", Shorter, canvas),
                KeyBinding::new("cmd-shift-down", Taller, canvas),
                KeyBinding::new("cmd-shift-n", NewBeside, canvas),
            ]);
        } else {
            cx.bind_keys([
                KeyBinding::new("ctrl-left", NudgeLeft, canvas),
                KeyBinding::new("ctrl-right", NudgeRight, canvas),
                KeyBinding::new("ctrl-up", NudgeUp, canvas),
                KeyBinding::new("ctrl-down", NudgeDown, canvas),
                KeyBinding::new("ctrl-shift-left", Narrower, canvas),
                KeyBinding::new("ctrl-shift-right", Wider, canvas),
                KeyBinding::new("ctrl-shift-up", Shorter, canvas),
                KeyBinding::new("ctrl-shift-down", Taller, canvas),
                KeyBinding::new("ctrl-shift-n", NewBeside, canvas),
            ]);
        }
        let ed = Some("editing");
        // Bare arrows navigate unless the text or the color picker takes them.
        let nav = Some("!editing && !picker");
        let picker = Some("picker");
        // Enter and Esc everywhere, so they never reach a selected shape as typed text.
        cx.bind_keys([
            KeyBinding::new("f2", StartEditing, None),
            KeyBinding::new("tab", ShowTabPicker, None),
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("shift-enter", NewLine, ed),
            KeyBinding::new("escape", Escape, None),
            KeyBinding::new("left", Left, ed),
            KeyBinding::new("right", Right, ed),
            KeyBinding::new("up", Up, ed),
            KeyBinding::new("down", Down, ed),
            KeyBinding::new("home", LineStart, ed),
            KeyBinding::new("end", LineEnd, ed),
            KeyBinding::new("backspace", Backspace, ed),
            KeyBinding::new("delete", Delete, ed),
            KeyBinding::new("backspace", DeleteShapes, canvas),
            KeyBinding::new("delete", DeleteShapes, canvas),
            KeyBinding::new("cmd-d", Duplicate, canvas),
            KeyBinding::new("ctrl-d", Duplicate, canvas),
            KeyBinding::new("cmd-l", SelectChain, canvas),
            KeyBinding::new("ctrl-l", SelectChain, canvas),
            KeyBinding::new("cmd-shift-c", OpenColorPicker, canvas),
            KeyBinding::new("ctrl-shift-c", OpenColorPicker, canvas),
            KeyBinding::new("left", PickerLeft, picker),
            KeyBinding::new("right", PickerRight, picker),
            KeyBinding::new("enter", PickerApply, picker),
            KeyBinding::new("escape", PickerCancel, picker),
            KeyBinding::new("left", NavLeft, nav),
            KeyBinding::new("right", NavRight, nav),
            KeyBinding::new("up", NavUp, nav),
            KeyBinding::new("down", NavDown, nav),
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
                Cow::Borrowed(include_bytes!("../assets/JetBrainsMono-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/JetBrainsMono-SemiBold.ttf").as_slice()),
            ])
            .expect("bundled fonts");
        #[cfg(target_os = "macos")]
        set_menus(cx);
        cx.on_action(|_: &Quit, cx| file::quit(cx));
        cx.on_action(|_: &NewFile, cx| file::new_file(cx));
        cx.on_action(|_: &OpenFile, cx| file::open_file(cx));
        // Preferences are the same in every window; a bench run keeps the defaults.
        let prefs = prefs::file().filter(|_| bench.is_none());
        cx.set_global(prefs.map_or_else(Default::default, |f| prefs::Prefs::load(&f)));
        cx.set_quit_mode(QuitMode::LastWindowClosed);
        // A plain launch applies the recovery files instead of opening a new document.
        if recover {
            recovery::launch(cx);
        } else {
            open_window(Content::untitled(doc), bench, cx);
        }
        cx.activate(true);
    });
}

/// Opens a window showing `content`; `bench` runs the scripted bench in it.
fn open_window(content: Content, bench: Option<String>, cx: &mut App) {
    let c = &PAPER.chrome;
    let bounds = Bounds::centered(None, size(px(c.window_width), px(c.window_height)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(file::name(content.path.as_deref()).into()),
                appears_transparent: true,
                traffic_light_position: Some(point(
                    px(c.mac_traffic_light),
                    px(c.mac_traffic_light),
                )),
            }),
            ..Default::default()
        },
        |window, cx| {
            if let Some(w) = window.window_handle().downcast::<Willow>() {
                // Close only once the user has settled unsaved changes and the recovery
                // file is gone.
                window.on_window_should_close(cx, move |_, cx| {
                    file::close(w, cx);
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
                    } else {
                        w.check_disk(cx);
                    }
                });
                let _tab_picker_keys = tab_picker_keys(cx, window);
                let recovery = content.recovery.clone();
                let (session, path) = content.session(window);
                let mut w = Willow {
                    session,
                    focus,
                    cursor: Cursor::Arrow,
                    canvas_origin: Default::default(),
                    start: Instant::now(),
                    wake: None,
                    ahead: None,
                    _activation,
                    _tab_picker_keys,
                    recovery: bench.is_none().then(|| recovery::Autosave::new(recovery)),
                    bench: bench.map(bench::Bench::new),
                    path,
                    mtime: None,
                    settling: false,
                    regeneration: None,
                    panel_drag: None,
                };
                w.measure_ahead(cx);
                w
            })
        },
    )
    .expect("open window");
}
