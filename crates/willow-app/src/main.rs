//! Willow's window: turns GPUI events into editor-session input and draws what the session holds.

use std::time::Instant;

use gpui::{
    App, Bounds, BoxShadow, Context, Corners, CursorStyle, DispatchPhase, FocusHandle, Hsla,
    KeyBinding, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollEvent, TitlebarOptions, Window, WindowBounds, WindowOptions,
    actions, canvas, div, point, prelude::*, px, quad, size,
};
use willow_core::{
    Button, Cursor, Document, Input, Key, Modifiers, PAPER, Rgba, Session, Shadow, ShapeId, Theme,
    Viewport,
};
#[cfg(target_os = "windows")]
mod titlebar_windows;
#[cfg(target_os = "windows")]
use titlebar_windows::title_bar;

actions!(willow, [Quit]);

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
        self.input(Input::Wheel { delta, mods }, cx);
    }
}

impl Render for Willow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.session.theme();
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
                paint(
                    bounds,
                    session.document(),
                    session.viewport(),
                    theme,
                    window,
                    session.drag_preview(),
                    &session.selection_rings(now),
                );
            },
        )
        .size_full();
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(color(theme.canvas))
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
            }));
        let body = div()
            .flex_1()
            .overflow_hidden()
            .cursor(match self.cursor {
                Cursor::Arrow => CursorStyle::Arrow,
                Cursor::Grab => CursorStyle::OpenHand,
                Cursor::Grabbing => CursorStyle::ClosedHand,
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_scroll(cx.listener(Self::scroll))
            .child(canvas);
        // Windows: title bar above the canvas. macOS: title floats over the canvas, so paint it last.
        if cfg!(target_os = "windows") {
            root.child(title_bar(theme, window)).child(body)
        } else {
            root.child(body).child(title_bar(theme, window))
        }
    }
}

/// Canvas, dot grid and shapes. Zoom is 1 until zoom lands (#7).
fn paint(
    bounds: Bounds<Pixels>,
    doc: &Document,
    vp: Viewport,
    t: &Theme,
    window: &mut Window,
    preview: Option<(&ShapeId, i32, i32)>,
    rings: &[(&ShapeId, f32)],
) {
    window.paint_quad(gpui::fill(bounds, color(t.canvas)));
    // Screen position of canvas (0, 0), on whole pixels.
    let o = vp.to_screen(willow_core::Point::default());
    let origin = bounds.origin + point(px(o.x.round()), px(o.y.round()));

    // One dot at the center of every grid cell, as the mockups' radial-gradient tile draws it.
    let (s, r) = (t.grid_spacing, t.grid_dot_radius);
    let phase = |o: Pixels, b: Pixels| f32::from(o - b + px(s / 2.0)).rem_euclid(s);
    let (x0, y0) = (
        phase(origin.x, bounds.origin.x),
        phase(origin.y, bounds.origin.y),
    );
    let dot = color(t.grid_dot);
    let mut y = y0;
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

    let radius = px(t.shape.radius);
    let sel = &t.selection;
    for shape in &doc.shapes {
        let (x, y) = match preview {
            Some((id, x, y)) if *id == shape.id => (x, y),
            _ => (shape.x, shape.y),
        };
        let b = Bounds::new(
            origin + point(px(x as f32), px(y as f32)),
            size(px(shape.width as f32), px(shape.height(t))),
        );
        let ring = rings
            .iter()
            .find(|(id, _)| **id == shape.id)
            .map_or(0.0, |r| r.1);
        let shadows = shadows(&t.shape.shadow, &sel.shadow, ring);
        window.paint_drop_shadows(b, Corners::all(radius), &shadows);
        if ring > 0.0 {
            // As the mockups' box-shadow spreads: a canvas gap, then the accent ring, both growing.
            let out = (sel.gap + sel.ring) * ring;
            let ring_bounds = b.dilate(px(out));
            window.paint_quad(quad(
                ring_bounds,
                radius + px(out),
                color(t.canvas),
                px(sel.ring * ring),
                color(sel.color),
                Default::default(),
            ));
        }
        window.paint_quad(quad(
            b,
            radius,
            color(t.pastel(shape.color)),
            px(0.),
            gpui::transparent_black(),
            Default::default(),
        ));
    }
}

/// The resting shadow eased toward the selected one by `k` (0 to 1).
fn shadows(rest: &[Shadow; 2], selected: &[Shadow; 2], k: f32) -> Vec<BoxShadow> {
    let mix = |a: f32, b: f32| a + (b - a) * k;
    rest.iter()
        .zip(selected)
        .map(|(a, b)| {
            let (ca, cb) = (a.color.0.to_be_bytes(), b.color.0.to_be_bytes());
            let c: [u8; 4] = std::array::from_fn(|i| mix(ca[i] as f32, cb[i] as f32).round() as u8);
            BoxShadow {
                color: color(Rgba(u32::from_be_bytes(c))),
                offset: point(px(mix(a.x, b.x)), px(mix(a.y, b.y))),
                // GPUI's blur radius is the Gaussian sigma; CSS blur is 2 sigma.
                blur_radius: px(mix(a.blur, b.blur) / 2.0),
                spread_radius: px(mix(a.spread, b.spread)),
                inset: false,
            }
        })
        .collect()
}

/// macOS: transparent title bar over the canvas; the traffic lights are native.
#[cfg(not(target_os = "windows"))]
fn title_bar(t: &Theme, _: &Window) -> impl IntoElement {
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

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let quit = if cfg!(target_os = "macos") {
            "cmd-q"
        } else {
            "alt-f4"
        };
        cx.bind_keys([KeyBinding::new(quit, Quit, None)]);
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
                        session: Session::new(Document::new(), &PAPER),
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
