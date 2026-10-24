//! Willow Phase 0 throwaway spike: can GPUI carry the app?
//!
//!   cargo run --release -p spike-gpui                 small demo document
//!   cargo run --release -p spike-gpui -- --large      generated 10,000-shape document
//!   cargo run --release -p spike-gpui -- --bench --machine mac-m4-air
//!                                                     scripted measurements, appended to results.csv
//!   cargo run --release -p spike-gpui -- --selftest   scripted editing checks + screenshots
//!   --text-cutoff PX                                  hide text below PX on-screen font size (default 4)
//!
//! Keys: click a shape to edit; Esc stops editing; Cmd/Ctrl+B/I bold/italic;
//! Tab shows the directional picker, then an arrow adds a linked shape;
//! right-click for the context menu; Cmd/Ctrl+\ toggles the side panel;
//! L loads the large document. Scroll pans, Cmd/Ctrl+scroll or pinch zooms.

mod bench;
mod text;

use std::{borrow::Cow, ops::Range, rc::Rc, time::Instant};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler,
    EntityInputHandler, FocusHandle, Focusable, Hsla, KeyBinding, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, PinchEvent, Pixels, Point,
    ScrollDelta, ScrollEvent, TitlebarOptions, UTF16Selection, Window, WindowBounds, WindowOptions,
    actions, anchored, canvas, deferred, div, point, prelude::*, px, quad, rgb, size,
};
use text::{LINE_HEIGHT, Layout, Style};

actions!(
    spike,
    [
        Backspace, Delete, Left, Right, Up, Down, SelectLeft, SelectRight, SelectUp, SelectDown,
        SelectAll, Home, End, Enter, Escape, Bold, Italic, Copy, Cut, Paste, Picker, TogglePanel,
        LoadLarge, Quit,
    ]
);

const PAD: f32 = 12.0;
const MIN_H: f32 = 44.0;
const RADIUS: f32 = 10.0;
/// Six pastels plus the neutral. Placeholder values; Phase 1 picks the real ones.
const PASTELS: [u32; 7] = [0xeeeeee, 0xfde2e4, 0xfff1c1, 0xd8f3dc, 0xd7e9fb, 0xe7dcf7, 0xffe5d0];

pub struct Shape {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub color: usize,
    pub text: String,
    pub styles: Vec<Style>, // one per byte of `text`
    layout: Option<Rc<Layout>>,
}

impl Shape {
    pub fn new(x: f32, y: f32, w: f32, color: usize, text: String, styles: Vec<Style>) -> Self {
        Shape { x, y, w, color, text, styles, layout: None }
    }
    fn h(&self) -> f32 {
        let text_h = self.layout.as_ref().map_or(LINE_HEIGHT, |l| l.height);
        (text_h + 2.0 * PAD).max(MIN_H)
    }
    fn center(&self) -> Point<f32> {
        point(self.x + self.w / 2.0, self.y + self.h() / 2.0)
    }
    fn contains(&self, p: Point<f32>) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h()
    }
    /// Midpoint of the side facing `other`, derived at draw time (never stored).
    fn side_toward(&self, other: Point<f32>) -> Point<f32> {
        let c = self.center();
        let (dx, dy) = (other.x - c.x, other.y - c.y);
        if dy.abs() * self.w >= dx.abs() * self.h() {
            point(c.x, if dy > 0.0 { self.y + self.h() } else { self.y })
        } else {
            point(if dx > 0.0 { self.x + self.w } else { self.x }, c.y)
        }
    }
}

#[derive(Default)]
enum Drag {
    #[default]
    None,
    Pan(Point<Pixels>),
    Select,
}

pub struct Spike {
    focus: FocusHandle,
    pub shapes: Vec<Shape>,
    pub links: Vec<(usize, usize)>,
    pub offset: Point<f32>, // canvas point at the canvas element's top-left
    pub zoom: f32,
    editing: Option<usize>,
    sel: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    typing_style: Style,
    hover: Option<usize>,
    drag: Drag,
    menu: Option<(Point<Pixels>, usize)>,
    picker: bool,
    panel: bool,
    zoom_badge: Option<Instant>,
    canvas_origin: Point<Pixels>,
    /// Smallest on-screen font size (px) at which text is drawn at all.
    pub text_cutoff: f32,
    pub renders: u64,
    pub bench: Option<bench::Bench>,
}

impl Spike {
    fn new(cx: &mut Context<Self>, doc: (Vec<Shape>, Vec<(usize, usize)>)) -> Self {
        Spike {
            focus: cx.focus_handle(),
            shapes: doc.0,
            links: doc.1,
            offset: point(-40.0, -40.0),
            zoom: 1.0,
            editing: None,
            sel: 0..0,
            reversed: false,
            marked: None,
            typing_style: Style::default(),
            hover: None,
            drag: Drag::None,
            menu: None,
            picker: false,
            panel: false,
            zoom_badge: None,
            canvas_origin: Point::default(),
            text_cutoff: 4.0,
            renders: 0,
            bench: None,
        }
    }

    fn to_canvas(&self, p: Point<Pixels>) -> Point<f32> {
        let local = p - self.canvas_origin;
        point(
            f32::from(local.x) / self.zoom + self.offset.x,
            f32::from(local.y) / self.zoom + self.offset.y,
        )
    }

    fn to_screen(&self, p: Point<f32>) -> Point<Pixels> {
        point(px((p.x - self.offset.x) * self.zoom), px((p.y - self.offset.y) * self.zoom))
    }

    fn hit(&self, p: Point<f32>) -> Option<usize> {
        // ponytail: linear scan over all shapes; a grid index if 10k hit-tests get slow.
        self.shapes.iter().rposition(|s| s.contains(p))
    }

    pub fn zoom_at(&mut self, factor: f32, at: Point<Pixels>) {
        let before = self.to_canvas(at);
        self.zoom = (self.zoom * factor).clamp(0.02, 4.0);
        let after = self.to_canvas(at);
        self.offset = self.offset + (before - after);
        self.zoom_badge = Some(Instant::now());
    }

    // ---- editing ----

    fn edited(&self) -> Option<&Shape> {
        self.editing.map(|i| &self.shapes[i])
    }

    fn cursor(&self) -> usize {
        if self.reversed { self.sel.start } else { self.sel.end }
    }

    fn move_to(&mut self, i: usize, cx: &mut Context<Self>) {
        self.sel = i..i;
        self.reversed = false;
        self.typing_style = self.style_before(i);
        cx.notify();
    }

    fn select_to(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.reversed { self.sel.start = i } else { self.sel.end = i }
        if self.sel.end < self.sel.start {
            self.reversed = !self.reversed;
            self.sel = self.sel.end..self.sel.start;
        }
        cx.notify();
    }

    fn style_before(&self, i: usize) -> Style {
        self.edited()
            .and_then(|s| s.styles.get(i.checked_sub(1)?).copied())
            .unwrap_or_default()
    }

    fn prev_boundary(&self, i: usize) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        let Some(s) = self.edited() else { return 0 };
        s.text.grapheme_indices(true).rev().find_map(|(j, _)| (j < i).then_some(j)).unwrap_or(0)
    }

    fn next_boundary(&self, i: usize) -> usize {
        use unicode_segmentation::UnicodeSegmentation;
        let Some(s) = self.edited() else { return 0 };
        s.text
            .grapheme_indices(true)
            .find_map(|(j, _)| (j > i).then_some(j))
            .unwrap_or(s.text.len())
    }

    fn vertical(&self, i: usize, dir: isize) -> usize {
        let Some(layout) = self.edited().and_then(|s| s.layout.clone()) else { return i };
        let stop = layout.stop_for(i);
        let line = stop.line as isize + dir;
        if line < 0 {
            return 0;
        }
        if line as usize >= layout.lines.len() {
            return self.edited().map_or(0, |s| s.text.len());
        }
        layout.closest_on_line(line as usize, stop.x)
    }

    fn replace(&mut self, range: Range<usize>, new: &str, cx: &mut Context<Self>) {
        let Some(idx) = self.editing else { return };
        let style = self.typing_style;
        let s = &mut self.shapes[idx];
        s.text.replace_range(range.clone(), new);
        s.styles.splice(range.clone(), std::iter::repeat_n(style, new.len()));
        s.layout = None; // relaid out (and the shape regrown) on the next render
        self.marked = None;
        let end = range.start + new.len();
        self.sel = end..end;
        self.reversed = false;
        cx.notify();
    }

    fn toggle_style(&mut self, f: impl Fn(&mut Style) -> &mut bool, cx: &mut Context<Self>) {
        let Some(idx) = self.editing else { return };
        if self.sel.is_empty() {
            let v = f(&mut self.typing_style);
            *v = !*v;
            return;
        }
        let range = self.sel.clone();
        let s = &mut self.shapes[idx];
        let all_on = s.styles[range.clone()].iter().all(|st| *f(&mut st.clone()));
        for st in &mut s.styles[range] {
            *f(st) = !all_on;
        }
        s.layout = None;
        cx.notify();
    }

    fn stop_editing(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        self.marked = None;
        self.picker = false;
        cx.notify();
    }

    fn start_editing(&mut self, idx: usize, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some(idx);
        self.picker = false;
        window.focus(&self.focus, cx);
        self.move_to(at, cx);
    }

    fn index_at(&self, p: Point<f32>) -> usize {
        let Some(s) = self.edited() else { return 0 };
        let Some(layout) = &s.layout else { return 0 };
        layout.closest(point(p.x - s.x - PAD, p.y - s.y - PAD))
    }

    fn add_linked(&mut self, dir: Point<f32>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.editing else { return };
        let f = &self.shapes[from];
        let gap = 48.0;
        let (x, y) = if dir.x != 0.0 {
            (f.x + dir.x * (f.w + gap), f.y)
        } else {
            (f.x, if dir.y > 0.0 { f.y + f.h() + gap } else { f.y - MIN_H - gap })
        };
        let s = Shape::new(x.round(), y.round(), f.w, f.color, String::new(), Vec::new());
        self.shapes.push(s);
        let to = self.shapes.len() - 1;
        self.links.push((from, to));
        self.start_editing(to, 0, window, cx);
    }

    // ---- input ----

    fn on_mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        let p = self.to_canvas(e.position);
        match self.hit(p) {
            Some(i) if Some(i) == self.editing => {
                let at = self.index_at(p);
                if e.modifiers.shift { self.select_to(at, cx) } else { self.move_to(at, cx) }
                self.drag = Drag::Select;
            }
            Some(i) => {
                self.start_editing(i, 0, window, cx);
                let at = self.index_at(p); // layout may be missing until next render
                self.move_to(at, cx);
                self.drag = Drag::Select;
            }
            None => {
                self.stop_editing(cx);
                window.focus(&self.focus, cx);
                self.drag = Drag::Pan(e.position);
            }
        }
    }

    fn on_mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.drag {
            Drag::Pan(last) => {
                let d = e.position - last;
                self.offset.x -= f32::from(d.x) / self.zoom;
                self.offset.y -= f32::from(d.y) / self.zoom;
                self.drag = Drag::Pan(e.position);
                cx.notify();
            }
            Drag::Select => {
                let at = self.index_at(self.to_canvas(e.position));
                self.select_to(at, cx);
            }
            Drag::None => {
                let h = self.hit(self.to_canvas(e.position));
                if h != self.hover {
                    self.hover = h;
                    cx.notify();
                }
            }
        }
    }

    fn on_scroll(&mut self, e: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let d = match e.delta {
            ScrollDelta::Pixels(p) => point(f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => point(l.x * 40.0, l.y * 40.0),
        };
        if e.modifiers.platform || e.modifiers.control {
            self.zoom_at((d.y * 0.01).exp(), e.position);
        } else {
            self.offset.x -= d.x / self.zoom;
            self.offset.y -= d.y / self.zoom;
        }
        cx.notify();
    }

    fn on_pinch(&mut self, e: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_at(1.0 + e.delta, e.position);
        cx.notify();
    }

    /// While the directional picker is up, an arrow adds a linked shape on that side.
    fn picked(&mut self, dir: Point<f32>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.picker {
            return false;
        }
        self.picker = false;
        self.add_linked(dir, window, cx);
        true
    }
}

macro_rules! act {
    ($name:ident, $act:ty, |$s:ident, $w:ident, $cx:ident| $body:expr) => {
        fn $name(&mut self, _: &$act, $w: &mut Window, $cx: &mut Context<Self>) {
            let $s = self;
            let _ = &$w;
            $body
        }
    };
}

impl Spike {
    act!(left, Left, |s, w, cx| {
        if s.picked(point(-1.0, 0.0), w, cx) {
            return;
        }
        let to = if s.sel.is_empty() { s.prev_boundary(s.cursor()) } else { s.sel.start };
        s.move_to(to, cx)
    });
    act!(right, Right, |s, w, cx| {
        if s.picked(point(1.0, 0.0), w, cx) {
            return;
        }
        let to = if s.sel.is_empty() { s.next_boundary(s.cursor()) } else { s.sel.end };
        s.move_to(to, cx)
    });
    act!(up, Up, |s, w, cx| {
        if !s.picked(point(0.0, -1.0), w, cx) {
            s.move_to(s.vertical(s.cursor(), -1), cx)
        }
    });
    act!(down, Down, |s, w, cx| {
        if !s.picked(point(0.0, 1.0), w, cx) {
            s.move_to(s.vertical(s.cursor(), 1), cx)
        }
    });
    act!(select_left, SelectLeft, |s, w, cx| s.select_to(s.prev_boundary(s.cursor()), cx));
    act!(select_right, SelectRight, |s, w, cx| s.select_to(s.next_boundary(s.cursor()), cx));
    act!(select_up, SelectUp, |s, w, cx| s.select_to(s.vertical(s.cursor(), -1), cx));
    act!(select_down, SelectDown, |s, w, cx| s.select_to(s.vertical(s.cursor(), 1), cx));
    act!(select_all, SelectAll, |s, w, cx| {
        let n = s.edited().map_or(0, |e| e.text.len());
        s.sel = 0..n;
        cx.notify()
    });
    act!(home, Home, |s, w, cx| s.move_to(0, cx));
    act!(end, End, |s, w, cx| s.move_to(s.edited().map_or(0, |e| e.text.len()), cx));
    act!(enter, Enter, |s, w, cx| s.replace(s.sel.clone(), "\n", cx));
    act!(escape, Escape, |s, w, cx| s.stop_editing(cx));
    act!(bold, Bold, |s, w, cx| s.toggle_style(|st| &mut st.bold, cx));
    act!(italic, Italic, |s, w, cx| s.toggle_style(|st| &mut st.italic, cx));
    act!(picker, Picker, |s, w, cx| {
        s.picker = s.editing.is_some() && !s.picker;
        cx.notify()
    });
    act!(toggle_panel, TogglePanel, |s, w, cx| {
        s.panel = !s.panel;
        cx.notify()
    });
    act!(load_large, LoadLarge, |s, w, cx| {
        if s.editing.is_none() {
            (s.shapes, s.links) = generate(10_000);
            cx.notify()
        }
    });
    act!(backspace, Backspace, |s, w, cx| {
        if s.sel.is_empty() {
            let p = s.prev_boundary(s.cursor());
            s.sel = p..s.cursor();
        }
        s.replace(s.sel.clone(), "", cx)
    });
    act!(delete, Delete, |s, w, cx| {
        if s.sel.is_empty() {
            let n = s.next_boundary(s.cursor());
            s.sel = s.cursor()..n;
        }
        s.replace(s.sel.clone(), "", cx)
    });
    act!(copy, Copy, |s, w, cx| {
        if let Some(e) = s.edited() {
            cx.write_to_clipboard(ClipboardItem::new_string(e.text[s.sel.clone()].to_string()));
        }
    });
    act!(cut, Cut, |s, w, cx| {
        if let Some(e) = s.edited() {
            cx.write_to_clipboard(ClipboardItem::new_string(e.text[s.sel.clone()].to_string()));
            s.replace(s.sel.clone(), "", cx)
        }
    });
    act!(paste, Paste, |s, w, cx| {
        if let Some(t) = cx.read_from_clipboard().and_then(|c| c.text()) {
            s.replace(s.sel.clone(), &t, cx)
        }
    });
}

// ---- IME / platform text input ----

impl Spike {
    fn utf16_to(&self, u: usize) -> usize {
        let Some(s) = self.edited() else { return 0 };
        let mut n = 0;
        for (i, c) in s.text.char_indices() {
            if n >= u {
                return i;
            }
            n += c.len_utf16();
        }
        s.text.len()
    }
    fn to_utf16(&self, b: usize) -> usize {
        self.edited().map_or(0, |s| s.text[..b.min(s.text.len())].encode_utf16().count())
    }
    fn range_from16(&self, r: &Range<usize>) -> Range<usize> {
        self.utf16_to(r.start)..self.utf16_to(r.end)
    }
    fn range_to16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }
    /// Screen rect of the caret at byte `i`, window coordinates.
    fn caret_rect(&self, i: usize) -> Option<Bounds<Pixels>> {
        let s = self.edited()?;
        let st = s.layout.as_ref()?.stop_for(i);
        let o = self.canvas_origin
            + self.to_screen(point(s.x + PAD + st.x, s.y + PAD + st.line as f32 * LINE_HEIGHT));
        Some(Bounds::new(o, size(px(1.5), px(LINE_HEIGHT * self.zoom))))
    }
}

impl EntityInputHandler for Spike {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.range_from16(&r);
        actual.replace(self.range_to16(&r));
        Some(self.edited()?.text[r].to_string())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.editing?;
        Some(UTF16Selection { range: self.range_to16(&self.sel), reversed: self.reversed })
    }
    fn set_selected_text_range(&mut self, r: Range<usize>, _: &mut Window, cx: &mut Context<Self>) {
        self.sel = self.range_from16(&r);
        cx.notify();
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.to_utf16(usize::MAX))
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to16(r))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = r
            .map(|r| self.range_from16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(range, text, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        new_sel: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = r
            .map(|r| self.range_from16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.sel.clone());
        self.replace(range.clone(), text, cx);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        if let Some(ns) = new_sel {
            let base = self.to_utf16(range.start);
            self.sel = self.utf16_to(base + ns.start)..self.utf16_to(base + ns.end);
        }
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.range_from16(&r);
        let a = self.caret_rect(r.start)?;
        let b = self.caret_rect(r.end)?;
        Some(Bounds::from_corners(a.origin, b.bottom_right()))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let i = self.index_at(self.to_canvas(p));
        Some(self.to_utf16(i))
    }
}

impl Focusable for Spike {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

// ---- drawing ----

struct Frame {
    shapes: Vec<(Bounds<Pixels>, Hsla, Option<Rc<Layout>>)>,
    lines: Vec<(Point<Pixels>, Point<Pixels>)>,
    overlay: Vec<(Bounds<Pixels>, Hsla, f32)>, // selection, caret, midpoints, picker
    zoom: f32,
}

impl Spike {
    fn frame(&mut self, viewport: gpui::Size<Pixels>, window: &mut Window) -> Frame {
        let vw = f32::from(viewport.width) / self.zoom;
        let vh = f32::from(viewport.height) / self.zoom;
        let (x0, y0) = (self.offset.x, self.offset.y);
        let visible = |s: &Shape| {
            s.x < x0 + vw && s.x + s.w > x0 && s.y < y0 + vh && s.y + s.h() > y0
        };
        let draw_text = self.zoom * text::FONT_SIZE >= self.text_cutoff;
        let ts = window.text_system().clone();
        let mut f = Frame { shapes: vec![], lines: vec![], overlay: vec![], zoom: self.zoom };

        for s in self.shapes.iter_mut() {
            // Lay out lazily, and only shapes on screen at a readable zoom.
            if s.layout.is_none() && draw_text && visible(s) {
                s.layout = Some(Rc::new(text::layout(&s.text, &s.styles, s.w - 2.0 * PAD, &ts)));
            }
        }
        if let Some(i) = self.editing
            && self.shapes[i].layout.is_none()
        {
            let s = &mut self.shapes[i];
            s.layout = Some(Rc::new(text::layout(&s.text, &s.styles, s.w - 2.0 * PAD, &ts)));
        }

        for s in &self.shapes {
            if !visible(s) {
                continue;
            }
            let o = self.to_screen(point(s.x, s.y));
            let b = Bounds::new(o, size(px(s.w * self.zoom), px(s.h() * self.zoom)));
            let layout = if draw_text { s.layout.clone() } else { None };
            f.shapes.push((b, rgb(PASTELS[s.color]).into(), layout));
        }
        for &(a, b) in &self.links {
            let (sa, sb) = (&self.shapes[a], &self.shapes[b]);
            let pa = sa.side_toward(sb.center());
            let pb = sb.side_toward(sa.center());
            let (minx, maxx) = (pa.x.min(pb.x), pa.x.max(pb.x));
            let (miny, maxy) = (pa.y.min(pb.y), pa.y.max(pb.y));
            if maxx < x0 || minx > x0 + vw || maxy < y0 || miny > y0 + vh {
                continue;
            }
            f.lines.push((self.to_screen(pa), self.to_screen(pb)));
        }

        let accent: Hsla = rgb(0x3a7bd5).into();
        if let Some(h) = self.hover.filter(|h| Some(*h) != self.editing) {
            let s = &self.shapes[h];
            let (c, hh) = (s.center(), s.h());
            for m in [
                point(c.x, s.y),
                point(c.x, s.y + hh),
                point(s.x, c.y),
                point(s.x + s.w, c.y),
            ] {
                let p = self.to_screen(m);
                f.overlay.push((Bounds::new(p - point(px(4.), px(4.)), size(px(8.), px(8.))), accent, 4.0));
            }
        }
        if let Some(s) = self.edited()
            && let Some(layout) = &s.layout
        {
            let sel_color = accent.opacity(0.25);
            if !self.sel.is_empty() {
                for (li, line) in layout.lines.iter().enumerate() {
                    let (a, b) = (self.sel.start.max(line.start), self.sel.end.min(line.end));
                    if a >= b && !(a == b && self.sel.contains(&line.start) && line.start == line.end) {
                        continue;
                    }
                    let xa = layout.stops.iter().find(|st| st.line == li && st.index >= a).map_or(0.0, |st| st.x);
                    let xb = layout.stops.iter().rev().find(|st| st.line == li && st.index <= b).map_or(0.0, |st| st.x);
                    let o = self.to_screen(point(s.x + PAD + xa, s.y + PAD + line.top));
                    let w = ((xb - xa).max(3.0)) * self.zoom;
                    f.overlay.push((Bounds::new(o, size(px(w), px(LINE_HEIGHT * self.zoom))), sel_color, 0.0));
                }
            }
            if let Some(m) = &self.marked {
                let (a, b) = (layout.stop_for(m.start), layout.stop_for(m.end));
                let y = s.y + PAD + (a.line as f32 + 1.0) * LINE_HEIGHT - 2.0;
                let o = self.to_screen(point(s.x + PAD + a.x, y));
                let w = if a.line == b.line { b.x - a.x } else { s.w - 2.0 * PAD - a.x };
                f.overlay.push((Bounds::new(o, size(px(w * self.zoom), px(1.5))), gpui::black(), 0.0));
            }
            if let Some(r) = self.caret_rect(self.cursor()) {
                let r = Bounds::new(r.origin - self.canvas_origin, r.size);
                f.overlay.push((r, accent, 0.0));
            }
            if self.picker {
                let (c, hh) = (s.center(), s.h());
                for m in [
                    point(c.x, s.y - 24.0),
                    point(c.x, s.y + hh + 24.0),
                    point(s.x - 24.0, c.y),
                    point(s.x + s.w + 24.0, c.y),
                ] {
                    let p = self.to_screen(m);
                    f.overlay.push((Bounds::new(p - point(px(12.), px(12.)), size(px(24.), px(24.))), accent.opacity(0.8), 12.0));
                }
            }
        }
        f
    }
}

fn paint_frame(bounds: Bounds<Pixels>, f: Frame, window: &mut Window) {
    let o = bounds.origin;
    let ink: Hsla = rgb(0x2b2b2b).into();
    window.paint_quad(gpui::fill(bounds, rgb(0xfafaf8)));
    if !f.lines.is_empty() {
        let mut path = PathBuilder::stroke(px(1.5));
        for (a, b) in &f.lines {
            path.move_to(o + *a);
            path.line_to(o + *b);
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(0xb5b5b5));
        }
    }
    let radius = px(RADIUS * f.zoom);
    for (b, color, _) in &f.shapes {
        let b = Bounds::new(o + b.origin, b.size);
        window.paint_quad(quad(b, radius, *color, px(0.), gpui::transparent_black(), Default::default()));
    }
    for (b, _, layout) in &f.shapes {
        if let Some(l) = layout {
            let to = o + b.origin + point(px(PAD * f.zoom), px(PAD * f.zoom));
            l.paint(to, f.zoom, ink, window);
        }
    }
    for (b, color, r) in &f.overlay {
        let b = Bounds::new(o + b.origin, b.size);
        window.paint_quad(quad(b, px(*r), *color, px(0.), gpui::transparent_black(), Default::default()));
    }
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let entity = cx.entity();
        let viewport = window.viewport_size();
        let frame = self.frame(viewport, window);

        // Zoom badge: fade over 600 ms after the last zoom, then stop asking for frames.
        let badge = self.zoom_badge.and_then(|t| {
            let age = t.elapsed().as_secs_f32();
            if age < 1.2 {
                window.request_animation_frame();
                Some((1.0 - ((age - 0.6) / 0.6).clamp(0.0, 1.0), format!("{:.0}%", self.zoom * 100.0)))
            } else {
                self.zoom_badge = None;
                None
            }
        });
        bench::step(self, window);

        let editing = self.editing.is_some();
        let canvas_el = canvas(
            move |_, _, _| (),
            move |bounds, (), window, cx| {
                paint_frame(bounds, frame, window);
                let focus = entity.read(cx).focus.clone();
                if editing {
                    window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx);
                }
                entity.update(cx, |s, _| {
                    s.canvas_origin = bounds.origin;
                    if let Some(b) = &mut s.bench {
                        b.painted();
                    }
                });
            },
        )
        .size_full();

        div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("Spike")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::bold))
            .on_action(cx.listener(Self::italic))
            .on_action(cx.listener(Self::picker))
            .on_action(cx.listener(Self::toggle_panel))
            .on_action(cx.listener(Self::load_large))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .child(title_bar(window))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .child(
                        div()
                            .id("canvas")
                            .flex_1()
                            .h_full()
                            .cursor(if editing { CursorStyle::IBeam } else { CursorStyle::Arrow })
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                            .on_mouse_down(MouseButton::Right, cx.listener(|s, e: &MouseDownEvent, _, cx| {
                                let p = s.to_canvas(e.position);
                                if let Some(i) = s.hit(p) {
                                    s.menu = Some((e.position, i));
                                    cx.notify();
                                }
                            }))
                            .on_mouse_up(MouseButton::Left, cx.listener(|s, _: &MouseUpEvent, _, _| s.drag = Drag::None))
                            .on_mouse_move(cx.listener(Self::on_mouse_move))
                            .on_scroll(cx.listener(Self::on_scroll))
                            .on_pinch(cx.listener(Self::on_pinch))
                            .child(canvas_el),
                    )
                    .when(self.panel, |d| d.child(self.side_panel())),
            )
            .when_some(badge, |d, (alpha, label)| {
                d.child(
                    div()
                        .absolute()
                        .bottom_4()
                        .left_4()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(gpui::black().opacity(0.6 * alpha))
                        .text_color(gpui::white().opacity(alpha))
                        .text_sm()
                        .child(label),
                )
            })
            .when_some(self.menu, |d, (pos, shape)| d.child(self.context_menu(pos, shape, cx)))
    }
}

impl Spike {
    fn context_menu(&self, pos: Point<Pixels>, shape: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let swatches = (0..PASTELS.len()).map(|c| {
            div()
                .id(("color", c))
                .size_5()
                .rounded_full()
                .bg(rgb(PASTELS[c]))
                .border_1()
                .border_color(rgb(0x999999))
                .hover(|s| s.border_color(rgb(0x3a7bd5)))
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.shapes[shape].color = c;
                    s.menu = None;
                    cx.notify();
                }))
        });
        deferred(
            anchored().position(pos).child(
                div()
                    .occlude()
                    .p_2()
                    .gap_2()
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .shadow_lg()
                    .bg(gpui::white())
                    .text_sm()
                    .child("Recolor chain")
                    .child(div().flex().gap_1().children(swatches))
                    .child(
                        div()
                            .id("delete")
                            .px_1()
                            .rounded_sm()
                            .hover(|s| s.bg(rgb(0xeeeeee)))
                            .child("Delete shape (no-op in spike)")
                            .on_click(cx.listener(|s, _, _, cx| {
                                s.menu = None;
                                cx.notify();
                            })),
                    ),
            ),
        )
    }

    fn side_panel(&self) -> impl IntoElement {
        let text = self.edited().map_or_else(
            || format!("{} shapes, {} links", self.shapes.len(), self.links.len()),
            |s| s.text.clone(),
        );
        div()
            .w(px(280.))
            .h_full()
            .p_3()
            .bg(gpui::white())
            .border_l_1()
            .border_color(rgb(0xdddddd))
            .text_sm()
            .font_family("Menlo")
            .child(text)
    }
}

#[cfg(target_os = "windows")]
fn title_bar(window: &mut Window) -> impl IntoElement {
    use gpui::WindowControlArea;
    let button = |id: &'static str, glyph: &'static str, area| {
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family("Segoe Fluent Icons")
            .text_size(px(10.))
            .hover(|s| s.bg(rgb(0xe5e5e5)))
            .window_control_area(area)
            .child(glyph)
    };
    div()
        .h(px(32.))
        .w_full()
        .flex()
        .items_center()
        .bg(rgb(0xf3f3f3))
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .pl_3()
                .gap_2()
                .text_sm()
                .window_control_area(WindowControlArea::Drag)
                .child("Untitled.willow")
                .child(div().size(px(6.)).rounded_full().bg(rgb(0xbbbbbb))),
        )
        .child(button("min", "\u{e921}", WindowControlArea::Min))
        .child(button(
            "max",
            if window.is_maximized() { "\u{e923}" } else { "\u{e922}" },
            WindowControlArea::Max,
        ))
        .child(button("close", "\u{e8bb}", WindowControlArea::Close))
}

#[cfg(not(target_os = "windows"))]
fn title_bar(_: &mut Window) -> impl IntoElement {
    // Transparent macOS title bar: leave room for the traffic lights.
    div().h(px(28.)).w_full().bg(rgb(0xf3f3f3))
}

// ---- generator ----

/// Deterministic willow-shaped document: `n` shapes in vertical trunks with
/// short side branches, prose of varied length with some bold/italic words.
pub fn generate(n: usize) -> (Vec<Shape>, Vec<(usize, usize)>) {
    let seed = std::cell::Cell::new(0x9e3779b97f4a7c15u64);
    let rnd = |m: u64| {
        let mut x = seed.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        seed.set(x);
        x % m
    };
    const WORDS: &[&str] = &[
        "the", "storm", "breaks", "over", "harbour", "while", "Mara", "waits", "for", "a",
        "letter", "that", "never", "comes", "and", "her", "brother", "lies", "about", "where",
        "he", "was", "on", "night", "of", "fire", "everyone", "remembers", "differently",
        "quietly", "betrayal", "returns", "with", "an", "old", "map", "uncertain", "promise",
    ];
    let mut shapes = Vec::with_capacity(n);
    let mut links = Vec::new();
    let (trunk_len, col_w, row_h) = (40usize, 760.0, 150.0);
    let mut col = 0;
    while shapes.len() < n {
        let base_x = col as f32 * col_w;
        let mut prev_trunk = None;
        for row in 0..trunk_len {
            if shapes.len() >= n {
                break;
            }
            let make = |x: f32, y: f32, color: usize, shapes: &mut Vec<Shape>| {
                let words = 6 + rnd(30) as usize;
                let (mut text, mut styles) = (String::new(), Vec::new());
                for w in 0..words {
                    let word = WORDS[rnd(WORDS.len() as u64) as usize];
                    let st = match rnd(12) {
                        0 => Style { bold: true, italic: false },
                        1 => Style { bold: false, italic: true },
                        _ => Style::default(),
                    };
                    let piece = if w + 1 < words { format!("{word} ") } else { format!("{word}.") };
                    styles.extend(std::iter::repeat_n(st, piece.len()));
                    text.push_str(&piece);
                }
                shapes.push(Shape::new(x, y, 220.0, color, text, styles));
                shapes.len() - 1
            };
            let y = row as f32 * row_h;
            let t = make(base_x, y, 1 + col % 6, &mut shapes);
            if let Some(p) = prev_trunk {
                links.push((p, t));
            }
            prev_trunk = Some(t);
            // Branch: 0-2 shapes to the right, every other row.
            if row % 2 == 0 {
                let mut prev = None;
                for k in 0..rnd(3) as usize {
                    if shapes.len() >= n {
                        break;
                    }
                    let b = make(base_x + 270.0 + k as f32 * 250.0, y, 0, &mut shapes);
                    if let Some(p) = prev {
                        links.push((p, b));
                    }
                    prev = Some(b);
                }
            }
        }
        col += 1;
    }
    (shapes, links)
}

fn demo() -> (Vec<Shape>, Vec<(usize, usize)>) {
    let para = |t: &str| {
        // **bold** and _italic_ markers, stripped.
        let (mut text, mut styles, mut st) = (String::new(), Vec::new(), Style::default());
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                    st.bold = !st.bold;
                }
                '_' => st.italic = !st.italic,
                c => {
                    text.push(c);
                    styles.extend(std::iter::repeat_n(st, c.len_utf8()));
                }
            }
        }
        (text, styles)
    };
    let items = [
        (0.0, 0.0, 1, "The **storm** breaks over the harbour while _Mara_ waits for a letter that never comes. Justified text, with **bold** and _italic_ words mixed into one paragraph so the shaper sees several fonts."),
        (0.0, 220.0, 1, "Her brother lies about where he was on the night of the fire. 日本語のテキストも折り返します。 Emoji 🌳🍂 fallback check."),
        (300.0, 0.0, 0, "Click to edit. Type to grow the shape."),
        (300.0, 220.0, 0, "Tab, then an arrow, adds a linked shape."),
    ];
    let mut shapes = Vec::new();
    for (x, y, c, t) in items {
        let (text, styles) = para(t);
        shapes.push(Shape::new(x, y, 240.0, c, text, styles));
    }
    (shapes, vec![(0, 1), (0, 2), (1, 3)])
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let bench = flag("--bench");
    let selftest = flag("--selftest");
    let opt = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1).cloned());
    let machine = opt("--machine").unwrap_or_else(|| "unknown".into());
    let cutoff: Option<f32> = opt("--text-cutoff").and_then(|v| v.parse().ok());
    let doc = if flag("--large") || bench { generate(10_000) } else { demo() };

    gpui_platform::application().run(move |cx: &mut App| {
        let fonts: Vec<Cow<'static, [u8]>> = vec![
            Cow::Borrowed(include_bytes!("../assets/IBMPlexSans-Regular.ttf")),
            Cow::Borrowed(include_bytes!("../assets/IBMPlexSans-Italic.ttf")),
            Cow::Borrowed(include_bytes!("../assets/IBMPlexSans-SemiBold.ttf")),
            Cow::Borrowed(include_bytes!("../assets/IBMPlexSans-SemiBoldItalic.ttf")),
        ];
        cx.text_system().add_fonts(fonts).expect("bundled fonts");
        let m = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
        let k = |s: &str| -> &'static str { format!("{m}-{s}").leak() };
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("delete", Delete, None),
            KeyBinding::new("left", Left, None),
            KeyBinding::new("right", Right, None),
            KeyBinding::new("up", Up, None),
            KeyBinding::new("down", Down, None),
            KeyBinding::new("shift-left", SelectLeft, None),
            KeyBinding::new("shift-right", SelectRight, None),
            KeyBinding::new("shift-up", SelectUp, None),
            KeyBinding::new("shift-down", SelectDown, None),
            KeyBinding::new("home", Home, None),
            KeyBinding::new("end", End, None),
            KeyBinding::new("enter", Enter, None),
            KeyBinding::new("escape", Escape, None),
            KeyBinding::new("tab", Picker, None),
            KeyBinding::new("l", LoadLarge, None),
            KeyBinding::new(k("a"), SelectAll, None),
            KeyBinding::new(k("b"), Bold, None),
            KeyBinding::new(k("i"), Italic, None),
            KeyBinding::new(k("c"), Copy, None),
            KeyBinding::new(k("x"), Cut, None),
            KeyBinding::new(k("v"), Paste, None),
            KeyBinding::new(k("\\"), TogglePanel, None),
            KeyBinding::new(k("q"), Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Willow spike".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(10.), px(8.))),
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| {
                        let mut s = Spike::new(cx, doc);
                        s.text_cutoff = cutoff.unwrap_or(4.0);
                        window.focus(&s.focus, cx);
                        if selftest {
                            cx.spawn_in(window, async move |this, cx| bench::selftest(this, cx).await)
                                .detach();
                        }
                        if bench {
                            let mut b = bench::Bench::new(machine);
                            if let Some(c) = cutoff {
                                b.tag = format!("@cutoff{c}");
                            }
                            s.bench = Some(b);
                            cx.spawn_in(window, async move |this, cx| bench::run(this, cx).await)
                                .detach();
                        }
                        s
                    })
                },
            )
            .unwrap();
        let _ = window;
        cx.activate(true);
    });
}
