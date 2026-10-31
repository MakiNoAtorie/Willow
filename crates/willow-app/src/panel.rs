//! The code panel on the right: the core's rows painted with line numbers and syntax colors,
//! the title-bar toggle, and regeneration off the UI thread.

use gpui::{
    Action, App, Bounds, Context, CursorStyle, DispatchPhase, ElementInputHandler, Entity,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels,
    ScrollEvent, ShapedLine, TextAlign, TextRun, UnderlineStyle, Window, canvas, div, fill, point,
    prelude::*, px, size,
};
use willow_core::{Input, Session};

use crate::prefs::{self, Prefs};
use crate::{TogglePanel, WHEEL_LINE, Willow, color, paint_caret};

impl Willow {
    /// Runs the session's due regeneration off the UI thread. A newer one drops this one's
    /// result, which the session would refuse anyway.
    pub(crate) fn regenerate(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.session.take_regeneration() else {
            return;
        };
        self.regeneration = Some(cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            this.update(cx, |w, cx| {
                w.session.regenerated(done);
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Willow {
    /// Waits for the panel text's check to be due; call after every input. Each keystroke
    /// restarts the wait.
    pub(crate) fn arm_text_check(&mut self, cx: &mut Context<Self>) {
        let now = self.start.elapsed();
        self.text_wake = self.session.text_wake(now).map(|wait| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                this.update(cx, |w, cx| w.check_text(cx)).ok();
            })
        });
    }

    /// Checks the panel text off the UI thread, then applies it if no line is invalid. A
    /// newer check drops this one's result, which the session would refuse anyway.
    fn check_text(&mut self, cx: &mut Context<Self>) {
        let Some(job) = self.session.take_text_check(self.start.elapsed()) else {
            // Woken early: wait again.
            self.arm_text_check(cx);
            return;
        };
        self.text_check = Some(cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            this.update(cx, |w, cx| {
                w.session.text_checked(done);
                w.arm_recovery(cx);
                w.regenerate(cx);
                cx.notify();
            })
            .ok();
        }));
    }
}

/// The open panel beside the canvas, or nothing while closed.
pub(crate) fn panel(session: &Session, cx: &mut Context<Willow>) -> Option<impl IntoElement> {
    let p = session.panel();
    if !p.open {
        return None;
    }
    let c = session.theme().code;
    let (entity, keys) = (cx.entity(), cx.entity());
    let rows = canvas(
        move |bounds, window, cx| {
            let session = &entity.read(cx).session;
            (
                overlays(bounds, session),
                shape_rows(bounds, session, window),
            )
        },
        move |bounds, ((under, over), (rows, tints)), window, cx| {
            for b in tints {
                window.paint_quad(fill(b, color(c.invalid_tint)));
            }
            under.into_iter().for_each(|q| window.paint_quad(q));
            for (line, at) in rows {
                // A glyph that fails to paint only loses that glyph.
                let h = px(c.row_height());
                let _ = line.paint(at, h, TextAlign::Left, None, window, cx);
            }
            over.into_iter().for_each(|q| window.paint_quad(q));
            let w = keys.read(cx);
            let caret = w.session.panel_caret(w.start.elapsed());
            if let Some(c) = caret.filter(|c| c.visible) {
                let t = w.session.theme();
                paint_caret(window, bounds.origin, c, t, t.caret.color, 1.0, 1.0);
            }
            // Typing and the IME go to the panel while it has the keys.
            if w.session.panel().focused {
                let focus = w.focus.clone();
                let handler = ElementInputHandler::new(bounds, keys.clone());
                window.handle_input(&focus, handler, cx);
            }
        },
    )
    .size_full()
    .cursor(CursorStyle::IBeam);
    Some(
        div()
            .relative()
            .flex_none()
            .w(px(p.width))
            .h_full()
            .child(
                div()
                    .size_full()
                    .overflow_hidden()
                    .bg(color(c.bg))
                    .border_l(px(c.border_width))
                    .border_color(color(c.border))
                    // Canvas-relative like any press: the core finds the panel past the
                    // canvas's edge.
                    .on_mouse_down(MouseButton::Left, cx.listener(Willow::mouse_down))
                    .on_scroll(cx.listener(|w, e: &ScrollEvent, _, cx| {
                        let d = e.delta.pixel_delta(px(WHEEL_LINE));
                        w.input(Input::ScrollPanel(d.y.into()), cx)
                    }))
                    .child(rows),
            )
            // The hairline's grab area, centered on it, outside the clipped panel so it can
            // reach over the canvas's edge; painted last, it takes the press first.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px((c.border_width - c.grab_width) / 2.0))
                    .h_full()
                    .w(px(c.grab_width))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|w, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            let (wide, x): (f32, f32) =
                                (window.viewport_size().width.into(), e.position.x.into());
                            w.panel_drag = Some(wide - w.session.panel().width - x);
                        }),
                    ),
            ),
    )
}

/// The selection highlight to paint under the text, and the IME underline over it.
fn overlays(bounds: Bounds<Pixels>, session: &Session) -> (Vec<PaintQuad>, Vec<PaintQuad>) {
    let (t, p) = (session.theme(), session.panel());
    let at = |q: willow_core::Point| bounds.origin + point(px(q.x), px(q.y));
    let under = (session.panel_rects(p.selection()).into_iter())
        .map(|(a, b)| fill(Bounds::from_corners(at(a), at(b)), color(t.selection.text)))
        .collect();
    let over = (session.panel_underlines().into_iter())
        .map(|(a, b)| {
            let line = size(px(b.x - a.x), px(t.caret.marked_underline));
            fill(Bounds::new(at(a), line), color(t.ink))
        })
        .collect();
    (under, over)
}

/// Every row in view shaped, with where its line box's top-left goes, and the invalid rows'
/// tints; line numbers are right-aligned in the gutter.
#[allow(clippy::type_complexity)]
fn shape_rows(
    bounds: Bounds<Pixels>,
    session: &Session,
    window: &mut Window,
) -> (Vec<(ShapedLine, gpui::Point<Pixels>)>, Vec<Bounds<Pixels>>) {
    let (t, text) = (session.theme(), session.panel().text());
    let (c, size) = (&t.code, px(t.code.font_size));
    let font = |bold: bool| {
        let mut f = gpui::font(c.font);
        f.weight = if bold {
            FontWeight(c.id_weight)
        } else {
            FontWeight::NORMAL
        };
        f
    };
    let run = |len: usize, ink, bold, invalid: bool| TextRun {
        len,
        font: font(bold),
        color: color(ink),
        background_color: None,
        underline: invalid.then(|| UnderlineStyle {
            thickness: px(c.invalid_width),
            color: Some(color(c.invalid)),
            wavy: c.invalid_wavy,
        }),
        strikethrough: None,
    };
    let ts = window.text_system().clone();
    let (mut out, mut tints) = (Vec::new(), Vec::new());
    for r in session.panel_rows() {
        let top = bounds.origin.y + px(r.y);
        if r.invalid {
            let at = point(bounds.origin.x, top);
            tints.push(Bounds::new(
                at,
                gpui::size(bounds.size.width, px(c.row_height())),
            ));
        }
        if let Some(n) = r.number {
            let n = n.to_string();
            let runs = [run(n.len(), c.gutter, false, false)];
            let line = ts.shape_line(n.into(), size, &runs, None);
            let x = bounds.origin.x + px(c.gutter_width - c.gutter_gap) - line.width;
            out.push((line, point(x, top)));
        }
        if r.range.is_empty() {
            continue;
        }
        let runs: Vec<TextRun> = (r.spans.iter())
            .map(|s| run(s.range.len(), s.color, s.bold, r.invalid))
            .collect();
        let line = ts.shape_line(text[r.range].to_string().into(), size, &runs, None);
        let x = bounds.origin.x + px(c.gutter_width);
        out.push((line, point(x, top)));
    }
    (out, tints)
}

/// `button` (the toggle's click area in the title bar) with the panel icon centered in it; a
/// left press toggles the panel and goes no further.
pub(crate) fn toggle(button: gpui::Div, session: &Session) -> impl IntoElement {
    button
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            window.dispatch_action(TogglePanel.boxed_clone(), cx);
        })
        .child(icon(session))
}

/// The panel icon, filled while the panel is open.
fn icon(session: &Session) -> impl IntoElement {
    let t = session.theme();
    let (g, open) = (t.code.toggle, session.panel().open);
    let ink = color(if open { t.ink } else { t.chrome.title });
    div()
        .relative()
        .flex_none()
        .w(px(g.width))
        .h(px(g.height))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .border(px(g.stroke))
                .border_color(ink)
                .rounded(px(g.radius)),
        )
        .child(
            div()
                .absolute()
                .left(px(g.divider - g.stroke / 2.0))
                .top_0()
                .w(px(g.stroke))
                .h_full()
                .bg(ink),
        )
        .when(open, |d| {
            let i = g.fill_inset;
            d.child(
                div()
                    .absolute()
                    .left(px(g.divider))
                    .top(px(i))
                    .w(px(g.width - g.divider - i))
                    .h(px(g.height - 2.0 * i))
                    .rounded(px(g.fill_radius))
                    .bg(ink.opacity(g.fill_opacity)),
            )
        })
}

/// The preference holding the panel's width.
const WIDTH: &str = "panel_width";

/// Gives `session` the shared panel width, kept within this window's limits.
pub(crate) fn apply_width(session: &mut Session, window: &Window, cx: &App) {
    let want = (cx.global::<Prefs>().get_f32(WIDTH)).unwrap_or(session.theme().code.width);
    session.set_panel_width(want, window.viewport_size().width.into());
}

/// While the hairline is grabbed, window-wide so the drag goes on outside the panel: the
/// hairline follows the pointer, every window takes the new width, and letting go saves it.
pub(crate) fn drag_panel(entity: &Entity<Willow>, window: &mut Window, cx: &mut App) {
    if entity.read(cx).panel_drag.is_some() {
        window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
    }
    let e = entity.clone();
    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
        let Some(grab) = e.read(cx).panel_drag else {
            return;
        };
        if phase == DispatchPhase::Bubble {
            let wide: f32 = window.viewport_size().width.into();
            let x: f32 = ev.position.x.into();
            let width = e.update(cx, |w, _| {
                w.session.set_panel_width(wide - x - grab, wide);
                w.session.panel().width
            });
            cx.global_mut::<Prefs>().set_f32(WIDTH, width);
            cx.refresh_windows();
        }
    });
    let e = entity.clone();
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble && e.update(cx, |w, _| w.panel_drag.take()).is_some() {
            prefs::save_later(cx);
        }
    });
}
