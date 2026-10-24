//! Willow's window: draws the core's document with the core's theme.

use gpui::{
    App, Bounds, BoxShadow, Context, Corners, Hsla, KeyBinding, Pixels, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, canvas, div, point, prelude::*, px, quad, size,
};
#[cfg(target_os = "windows")]
mod titlebar_windows;
#[cfg(target_os = "windows")]
use titlebar_windows::title_bar;

use willow_core::{Document, PAPER, Rgba, Theme};

actions!(willow, [Quit]);

pub(crate) fn color(c: Rgba) -> Hsla {
    gpui::rgba(c.0).into()
}

struct Willow {
    doc: Document,
    theme: &'static Theme,
}

impl Render for Willow {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (doc, theme) = (self.doc.clone(), self.theme);
        let canvas = canvas(|_, _, _| (), move |b, (), w, _| paint(b, &doc, theme, w)).size_full();
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(color(theme.canvas));
        let body = div().flex_1().overflow_hidden().child(canvas);
        // Windows: title bar above the canvas. macOS: title floats over the canvas, so paint it last.
        if cfg!(target_os = "windows") {
            root.child(title_bar(theme, window)).child(body)
        } else {
            root.child(body).child(title_bar(theme, window))
        }
    }
}

/// Canvas, dot grid and shapes. Zoom is 1 and the first shape is centered until the viewport lands.
fn paint(bounds: Bounds<Pixels>, doc: &Document, t: &Theme, window: &mut Window) {
    window.paint_quad(gpui::fill(bounds, color(t.canvas)));
    let Some(first) = doc.shapes.first() else {
        return;
    };
    // Screen position of canvas (0, 0), on whole pixels.
    let origin = point(
        (bounds.center().x - px(first.width as f32 / 2.0)).round(),
        (bounds.center().y - px(first.height(t) / 2.0)).round(),
    );

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

    let shadows: Vec<BoxShadow> = t
        .shape
        .shadow
        .iter()
        .map(|s| BoxShadow {
            color: color(s.color),
            offset: point(px(s.x), px(s.y)),
            // GPUI's blur radius is the Gaussian sigma; CSS blur is 2 sigma.
            blur_radius: px(s.blur / 2.0),
            spread_radius: px(s.spread),
            inset: false,
        })
        .collect();
    let radius = px(t.shape.radius);
    for shape in &doc.shapes {
        let b = Bounds::new(
            origin + point(px(shape.x as f32), px(shape.y as f32)),
            size(px(shape.width as f32), px(shape.height(t))),
        );
        window.paint_drop_shadows(b, Corners::all(radius), &shadows);
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
            |_, cx| {
                cx.new(|_| Willow {
                    doc: Document::new(),
                    theme: &PAPER,
                })
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
