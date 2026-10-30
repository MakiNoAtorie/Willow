//! The code panel on the right: the core's rows painted with line numbers and syntax colors,
//! the title-bar toggle, and regeneration off the UI thread.

use gpui::{
    Action, App, Bounds, Context, FontWeight, MouseButton, Pixels, ScrollEvent, ShapedLine,
    TextAlign, TextRun, Window, canvas, div, point, prelude::*, px,
};
use willow_core::{Input, Session};

use crate::{TogglePanel, WHEEL_LINE, Willow, color};

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

/// The open panel beside the canvas, or nothing while closed.
pub(crate) fn panel(session: &Session, cx: &mut Context<Willow>) -> Option<impl IntoElement> {
    let p = session.panel();
    if !p.open {
        return None;
    }
    let c = session.theme().code;
    let entity = cx.entity();
    let rows = canvas(
        move |bounds, window, cx| shape_rows(bounds, &entity.read(cx).session, window),
        move |_, rows, window, cx| {
            for (line, at) in rows {
                // A glyph that fails to paint only loses that glyph.
                let h = px(c.row_height());
                let _ = line.paint(at, h, TextAlign::Left, None, window, cx);
            }
        },
    )
    .size_full();
    Some(
        div()
            .flex_none()
            .w(px(p.width))
            .h_full()
            .overflow_hidden()
            .bg(color(c.bg))
            .border_l(px(c.border_width))
            .border_color(color(c.border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|w, _, _, cx| w.input(Input::FocusPanel, cx)),
            )
            .on_scroll(cx.listener(|w, e: &ScrollEvent, _, cx| {
                let d = e.delta.pixel_delta(px(WHEEL_LINE));
                w.input(Input::ScrollPanel(d.y.into()), cx)
            }))
            .child(rows),
    )
}

/// Every row in view shaped, with where its line box's top-left goes; line numbers are
/// right-aligned in the gutter.
fn shape_rows(
    bounds: Bounds<Pixels>,
    session: &Session,
    window: &mut Window,
) -> Vec<(ShapedLine, gpui::Point<Pixels>)> {
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
    let run = |len: usize, ink, bold| TextRun {
        len,
        font: font(bold),
        color: color(ink),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let ts = window.text_system().clone();
    let mut out = Vec::new();
    for r in session.panel_rows() {
        let top = bounds.origin.y + px(r.y);
        if let Some(n) = r.number {
            let n = n.to_string();
            let runs = [run(n.len(), c.gutter, false)];
            let line = ts.shape_line(n.into(), size, &runs, None);
            let x = bounds.origin.x + px(c.gutter_width - c.gutter_gap) - line.width;
            out.push((line, point(x, top)));
        }
        if r.range.is_empty() {
            continue;
        }
        let runs: Vec<TextRun> = (r.spans.iter())
            .map(|s| run(s.range.len(), s.color, s.bold))
            .collect();
        let line = ts.shape_line(text[r.range].to_string().into(), size, &runs, None);
        let x = bounds.origin.x + px(c.gutter_width);
        out.push((line, point(x, top)));
    }
    out
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
