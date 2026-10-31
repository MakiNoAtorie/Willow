//! Themes as plain data, from `design/mockups/themes.js`. Drawing reads every visual value from here.

use std::time::Duration;

use crate::Color;

/// A color as 0xRRGGBBAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u32);

/// `#rrggbb`.
const fn hex(rgb: u32) -> Rgba {
    Rgba(rgb << 8 | 0xff)
}

/// CSS `rgba(r,g,b,a)`.
const fn rgba(r: u32, g: u32, b: u32, a: f32) -> Rgba {
    Rgba(r << 24 | g << 16 | b << 8 | (a * 255.0 + 0.5) as u32)
}

/// A CSS-style drop shadow, in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeStyle {
    pub radius: f32,
    pub pad_x: f32,
    /// Below the text.
    pub pad_y: f32,
    /// Above the text.
    pub pad_top: f32,
    /// A strip this tall along the top edge in the picker color; 0 for none. Inside `pad_top`.
    pub tab: f32,
    /// Resting shadow layers, bottom first.
    pub shadow: [Shadow; 2],
}

/// The straight line drawn for a link, under shapes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectorStyle {
    pub color: Rgba,
    /// Canvas units, but never thinner than `min_screen` screen pixels.
    pub width: f32,
    pub min_screen: f32,
    /// Hovered: `hover` color, `hover_width` canvas units (same floor).
    pub hover: Rgba,
    pub hover_width: f32,
    /// Screen pixels either side of the line that hover or right-click it.
    pub hit: f32,
}

/// A zone: a rounded rectangle of its pastel mixed into the canvas, with an inset edge and a
/// title at the top-left. Sizes are canvas units unless named screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneStyle {
    pub radius: f32,
    /// How far the fill moves from the canvas toward the pastel; deeper for nested zones.
    pub fill_mix: f32,
    pub nested_fill_mix: f32,
    /// The fill moves from the canvas toward the picker color rather than the pastel.
    pub fill_picker: bool,
    /// A soft shadow inside the zone's edge (a transparent color for none).
    pub inset: Shadow,
    /// The inset edge: the pastel moved this far toward the picker color, `edge_width` wide.
    pub edge_mix: f32,
    pub edge_width: f32,
    /// The hover ring on a zone's edge (a hovered band, a drop's target), in `accent`.
    pub ring: f32,
    /// Title in the theme font, ink color; never smaller than `title_min_screen` screen px and
    /// never faded. Its line box (`line_height` times its size, glyphs centered as in shape
    /// text) has its top-left `title_x`, `title_y` in, constant on screen when zoomed out.
    pub title_size: f32,
    pub title_weight: f32,
    /// Titles drawn in capitals, with `title_spacing` (a multiple of the size) after each letter.
    pub title_upper: bool,
    pub title_spacing: f32,
    pub title_min_screen: f32,
    pub title_x: f32,
    pub title_y: f32,
    /// The title band's height: screen px when zoomed out, scaled with the title zoomed in.
    pub band: f32,
    /// Room around the members of a zone the loader wraps (clears the title).
    pub padding: f32,
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier(pub f32, pub f32, pub f32, pub f32);

impl CubicBezier {
    /// CSS `ease-in`.
    pub const EASE_IN: CubicBezier = CubicBezier(0.42, 0.0, 1.0, 1.0);
    /// CSS `ease-out`.
    pub const EASE_OUT: CubicBezier = CubicBezier(0.0, 0.0, 0.58, 1.0);

    /// Eased progress for linear progress `x` (clamped to 0..=1).
    pub fn at(&self, x: f32) -> f32 {
        // Exact at the ends, where bisection would leave a speck.
        if x <= 0.0 || x >= 1.0 {
            return x.clamp(0.0, 1.0);
        }
        let b = |p1: f32, p2: f32, t: f32| {
            let u = 1.0 - t;
            3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
        };
        // x(t) is monotonic for CSS curves (x1, x2 in 0..=1), so bisection always converges.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..30 {
            let mid = (lo + hi) / 2.0;
            if b(self.0, self.2, mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        b(self.1, self.3, (lo + hi) / 2.0)
    }
}

/// A duration and its timing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: CubicBezier,
}

impl Transition {
    /// Eased progress `elapsed` into the transition: 0 at the start, 1 at and after the end.
    pub fn progress(&self, elapsed: Duration) -> f32 {
        if elapsed >= self.duration {
            return 1.0;
        }
        self.easing
            .at(elapsed.as_secs_f32() / self.duration.as_secs_f32())
    }
}

/// Zoom limits, in screen pixels per canvas unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomStyle {
    pub min: f32,
    pub max: f32,
    /// Text is fully shown at and above `fade_start` and fades linearly to gone at `fade_end`,
    /// while fills deepen up to `block_mix` toward their picker color.
    pub fade_start: f32,
    pub block_mix: f32,
    /// Below this zoom text is gone and shadows and the dot grid are dropped.
    pub fade_end: f32,
}

/// The zoom level indicator at the bottom center of the canvas, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomPillStyle {
    pub bg: Rgba,
    pub ink: Rgba,
    pub font_size: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    /// Gap between the pill and the canvas's bottom edge.
    pub bottom: f32,
    /// Fully shown this long after the last zoom change, then fades.
    pub hold: Duration,
    pub fade: Transition,
}

/// App menus drawn in the window (context menus, Windows' menu button), in screen pixels.
/// macOS' menu bar is native.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuStyle {
    /// `None` is the system UI font.
    pub font: Option<&'static str>,
    pub bg: Rgba,
    pub ink: Rgba,
    pub hover: Rgba,
    /// A destructive item (zone Delete).
    pub destructive: Rgba,
    pub separator: Rgba,
    /// Space above and below a separator line, and its thickness.
    pub separator_pad_y: f32,
    pub separator_width: f32,
    pub radius: f32,
    pub item_radius: f32,
    /// Inside the menu, around the items.
    pub padding: f32,
    pub item_pad_x: f32,
    pub item_pad_y: f32,
    /// Between an item's label and its shortcut.
    pub item_gap: f32,
    pub min_width: f32,
    pub font_size: f32,
    /// A multiple of `font_size`.
    pub line_height: f32,
    /// CSS weight (500 is medium).
    pub font_weight: f32,
    /// A heading over a group of entries ("Color"): size, opacity and the space above and
    /// below it.
    pub heading_size: f32,
    pub heading_opacity: f32,
    pub heading_pad_top: f32,
    pub heading_pad_bottom: f32,
    /// Shortcut text and the submenu arrow.
    pub shortcut_opacity: f32,
    /// A command that can't run now (Undo with nothing to undo).
    pub disabled_opacity: f32,
    /// The color row's circles: diameter, space between, and the current one's ring width and
    /// its gap from the circle.
    pub swatch: f32,
    /// Corner radius: half of `swatch` for circles.
    pub swatch_radius: f32,
    pub swatch_gap: f32,
    pub swatch_ring: f32,
    pub swatch_ring_gap: f32,
    /// Ring, then drop shadow.
    pub shadow: [Shadow; 2],
    /// The menu's left edge from the window's.
    pub left: f32,
    /// Between the title bar and the menu, and between a menu and its cascade.
    pub gap: f32,
}

/// A color change rippling along a chain: each shape fades `fade` long, starting `step`
/// after the one before it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RippleStyle {
    pub fade: Transition,
    pub step: Duration,
}

/// The window's title bar, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeStyle {
    /// A new window's size.
    pub window_width: f32,
    pub window_height: f32,
    /// Window title text.
    pub title: Rgba,
    /// Windows title-bar button hover fill.
    pub button_hover: Rgba,
    /// macOS: the transparent bar over the canvas, and its title size.
    pub mac_height: f32,
    pub mac_title_size: f32,
    /// macOS: the traffic lights' offset from the window's top-left corner.
    pub mac_traffic_light: f32,
    /// Windows: the custom bar, its title size, the min/max/close buttons and their icon
    /// glyph size, and the menu button.
    pub win_height: f32,
    pub win_title_size: f32,
    pub win_button_width: f32,
    pub win_glyph_size: f32,
    pub win_menu_button_width: f32,
    /// Windows: the unsaved dot after the document name (title color), and its gap.
    pub win_unsaved_dot: f32,
    pub win_unsaved_gap: f32,
}

/// The selected shape's look: a canvas-colored `gap`, then a `ring` of `color`, and `shadow`
/// in place of the resting one. Fades in and out over `fade`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionStyle {
    pub gap: f32,
    pub ring: f32,
    pub color: Rgba,
    /// Same layer count as `ShapeStyle::shadow`, so one can ease into the other.
    pub shadow: [Shadow; 2],
    pub fade: Transition,
    /// Highlight behind selected text while editing.
    pub text: Rgba,
    /// The Shift+drag marquee: border, fill, and border width and corner radius in screen px.
    pub marquee: Rgba,
    pub marquee_fill: Rgba,
    pub marquee_width: f32,
    pub marquee_radius: f32,
    pub marquee_dashed: bool,
    /// How far a selected shape moves from where it is drawn, canvas units, as the ring fades in.
    pub lift_x: f32,
    pub lift_y: f32,
}

/// One set of parallel lines in the canvas texture: `width` thick every `period`, screen px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weave {
    pub color: Rgba,
    pub width: f32,
    pub period: f32,
}

/// The text caret while editing: `width` wide in `color`, blinking every `blink` until `idle`
/// after the last edit input, then solid so the redraw loop ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretStyle {
    pub color: Rgba,
    pub width: f32,
    /// How long the caret stays shown, then hidden.
    pub blink: Duration,
    pub idle: Duration,
    /// Underline under IME marked text, in ink color: thickness in screen px at any zoom.
    pub marked_underline: f32,
}

/// The circles on a hovered shape's edge midpoints, in screen pixels at any zoom: `size`
/// across, `fill`, with a `stroke_width` ring of `stroke` outside it and a drop shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidpointStyle {
    pub size: f32,
    pub fill: Rgba,
    pub stroke: Rgba,
    pub stroke_width: f32,
    pub shadow: Shadow,
}

/// The Tab directional picker, screen pixels: `size` keycaps centered `offset` out from each
/// edge midpoint, an arrow glyph `glyph_size` long in each, and a dashed stub (`dash` on, `dash`
/// off) `stub_length` out from the midpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabPickerStyle {
    pub size: f32,
    pub offset: f32,
    pub radius: f32,
    pub fill: Rgba,
    pub shadow: [Shadow; 3],
    pub glyph: Rgba,
    pub glyph_size: f32,
    pub glyph_width: f32,
    pub stub: Rgba,
    pub stub_length: f32,
    pub stub_width: f32,
    pub dash: f32,
}

/// Keyboard pan, in screen pixels: a held arrow's speed ramps linearly up to `speed` (px/s)
/// over `ease_in` and back to 0 over `ease_out` after key-up; a press that would pan less
/// than `step` pans `step`, finishing `ease_out` after key-up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyPanStyle {
    pub step: f32,
    pub speed: f32,
    pub ease_in: Duration,
    pub ease_out: Duration,
}

/// A shape created from a midpoint fades in from `rise` canvas units off (negative is up)
/// and `scale`, while its connector draws from the source, over `motion`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreateStyle {
    pub motion: Transition,
    pub rise: f32,
    pub scale: f32,
}

/// The code panel on the right, in screen pixels. Text is `font` (bundled in regular and
/// `id_weight`), monospaced: every column is `advance` times `font_size` wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeStyle {
    /// The width a window's panel starts at.
    pub width: f32,
    /// The narrowest the panel is dragged to; the widest is half the window.
    pub min_width: f32,
    /// How wide the hairline's grab area is.
    pub grab_width: f32,
    pub bg: Rgba,
    /// The hairline on its left.
    pub border: Rgba,
    pub border_width: f32,
    pub font: &'static str,
    pub font_size: f32,
    /// A multiple of `font_size`.
    pub line_height: f32,
    pub advance: f32,
    /// Above the first line (clears a macOS title bar) and below the last.
    pub pad_top: f32,
    pub pad_bottom: f32,
    /// The line-number column, from the hairline to the text, and the space between a
    /// number's right edge and the text.
    pub gutter_width: f32,
    pub gutter_gap: f32,
    /// Between the longest row and the panel's right edge.
    pub pad_right: f32,
    /// Line numbers. Plain text is `Theme.ink`.
    pub gutter: Rgba,
    pub keyword: Rgba,
    pub id: Rgba,
    /// CSS weight of IDs.
    pub id_weight: f32,
    pub string: Rgba,
    pub direction: Rgba,
    pub layout: Rgba,
    /// An invalid line's underline, its thickness, whether it is wavy, and the line's tint.
    pub invalid: Rgba,
    pub invalid_width: f32,
    pub invalid_wavy: bool,
    pub invalid_tint: Rgba,
    pub toggle: ToggleStyle,
}

/// The title bar's panel icon: a `width` x `height` rounded outline with a divider
/// `divider` in from its left, the part right of it filled while the panel is open. Ink
/// while open, else the title color. macOS puts it `mac_right` from the window's right edge;
/// Windows in a caption-sized button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToggleStyle {
    pub width: f32,
    pub height: f32,
    pub stroke: f32,
    pub radius: f32,
    pub divider: f32,
    /// The open fill: inset from the outline's top, bottom and right, its radius and opacity.
    pub fill_inset: f32,
    pub fill_radius: f32,
    pub fill_opacity: f32,
    pub mac_right: f32,
}

impl CodeStyle {
    /// One row of text.
    pub fn row_height(&self) -> f32 {
        self.font_size * self.line_height
    }

    /// One column of text.
    pub fn column(&self) -> f32 {
        self.advance * self.font_size
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    /// Shape text font family; regular, bold, italic and bold italic are bundled.
    pub font: &'static str,
    /// Shape text size in canvas units, and line height as a multiple of it.
    pub font_size: f32,
    pub line_height: f32,
    pub canvas: Rgba,
    pub ink: Rgba,
    pub accent: Rgba,
    /// Dot grid: one dot of `grid_dot_radius` at the center of every `grid_spacing` cell.
    pub grid_dot: Rgba,
    pub grid_dot_radius: f32,
    pub grid_spacing: f32,
    /// Canvas texture of horizontal then vertical lines, at a fixed screen size at any zoom.
    pub weave: Option<[Weave; 2]>,
    pub shape: ShapeStyle,
    /// Edge-to-edge distance, either direction, from a shape to a new or auto-placed shape
    /// linked to it.
    pub creation_gap: f32,
    /// Edge-to-edge distance a push keeps between a shape and another chain.
    pub min_gap: f32,
    pub connector: ConnectorStyle,
    pub zone: ZoneStyle,
    pub selection: SelectionStyle,
    pub caret: CaretStyle,
    /// Shape fill per color, indexed in `Color::ALL` order.
    pub pastel: [Rgba; 7],
    /// Saturated swatch per color, indexed in `Color::ALL` order.
    pub picker: [Rgba; 7],
    pub chrome: ChromeStyle,
    pub zoom: ZoomStyle,
    /// One wheel zoom step easing to its new level.
    pub zoom_step: Transition,
    pub zoom_pill: ZoomPillStyle,
    pub menu: MenuStyle,
    /// A chain's color change rippling out from the shape that started it.
    pub ripple: RippleStyle,
    pub midpoint: MidpointStyle,
    pub tab_picker: TabPickerStyle,
    pub create: CreateStyle,
    /// Pushed shapes sliding to where the spacing pass, or insert-and-push, sends them.
    pub push: Transition,
    /// The view panning to show a shape the keyboard moved to, and the screen pixels it then
    /// keeps between the shape and the screen's edge.
    pub follow: Transition,
    pub follow_margin: f32,
    pub key_pan: KeyPanStyle,
    /// Canvas units one Ctrl/Cmd+Arrow press moves the selection, or resizes a shape, by.
    pub nudge: i32,
    pub code: CodeStyle,
}

impl ZoneStyle {
    /// Whether titles use the bold face: only Regular and Bold are bundled, and CSS matching
    /// takes Bold from 600 up. Drawn and measured alike.
    pub fn title_bold(&self) -> bool {
        self.title_weight >= 600.0
    }
}

impl Theme {
    /// One line of shape text, in canvas units.
    pub fn line_height(&self) -> f32 {
        self.font_size * self.line_height
    }

    pub fn pastel(&self, c: Color) -> Rgba {
        self.pastel[c as usize]
    }
}

pub const PAPER: Theme = Theme {
    name: "Paper",
    font: "Literata",
    font_size: 15.0,
    line_height: 1.5,
    canvas: hex(0xf6f3ec),
    ink: hex(0x3b342c),
    accent: hex(0x8b7760),
    grid_dot: rgba(70, 50, 20, 0.10),
    grid_dot_radius: 1.0,
    grid_spacing: 24.0,
    weave: None,
    shape: ShapeStyle {
        radius: 14.0,
        pad_x: 16.0,
        pad_y: 13.0,
        pad_top: 13.0,
        tab: 0.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 1.0,
                blur: 2.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.07),
            },
            Shadow {
                x: 0.0,
                y: 4.0,
                blur: 14.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.08),
            },
        ],
    },
    creation_gap: 48.0,
    min_gap: 24.0,
    connector: ConnectorStyle {
        color: hex(0xcbbfae),
        width: 2.0,
        min_screen: 1.0,
        hover: hex(0x8b7760),
        hover_width: 3.0,
        hit: 8.0,
    },
    zone: ZoneStyle {
        radius: 22.0,
        fill_picker: false,
        inset: Shadow {
            x: 0.0,
            y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: rgba(0, 0, 0, 0.0),
        },
        fill_mix: 0.45,
        nested_fill_mix: 0.7,
        edge_mix: 0.35,
        edge_width: 1.0,
        ring: 2.0,
        title_size: 13.0,
        title_weight: 600.0,
        title_upper: false,
        title_spacing: 0.0,
        title_min_screen: 12.0,
        title_x: 16.0,
        title_y: 12.0,
        band: 40.0,
        padding: 40.0,
    },
    selection: SelectionStyle {
        gap: 2.0,
        ring: 2.0,
        color: hex(0x8b7760),
        text: rgba(139, 119, 96, 0.22),
        marquee: rgba(139, 119, 96, 0.65),
        marquee_fill: rgba(139, 119, 96, 0.08),
        marquee_width: 1.0,
        marquee_radius: 3.0,
        marquee_dashed: false,
        lift_x: 0.0,
        lift_y: 0.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 1.0,
                blur: 2.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.07),
            },
            Shadow {
                x: 0.0,
                y: 6.0,
                blur: 18.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.12),
            },
        ],
        fade: Transition {
            duration: Duration::from_millis(120),
            easing: CubicBezier::EASE_OUT,
        },
    },
    caret: CaretStyle {
        color: hex(0x3b342c),
        width: 1.5,
        blink: Duration::from_millis(530),
        // ponytail: "a few seconds" after the last edit; tune by feel.
        idle: Duration::from_secs(5),
        marked_underline: 1.0,
    },
    pastel: [
        hex(0xe9e5dd),
        hex(0xf3d8d3),
        hex(0xf6dfc8),
        hex(0xf2e8c0),
        hex(0xd9e5cf),
        hex(0xd3e0ea),
        hex(0xe2d8ea),
    ],
    picker: [
        hex(0xa39b8f),
        hex(0xd9776c),
        hex(0xe3955a),
        hex(0xd2ad3a),
        hex(0x7ea468),
        hex(0x5d8fbf),
        hex(0x9479b8),
    ],
    chrome: ChromeStyle {
        window_width: 1180.0,
        window_height: 740.0,
        title: rgba(59, 52, 44, 0.55),
        button_hover: rgba(70, 50, 20, 0.07),
        mac_height: 38.0,
        mac_title_size: 13.0,
        mac_traffic_light: 12.0,
        win_height: 34.0,
        win_title_size: 12.5,
        win_button_width: 46.0,
        win_glyph_size: 10.0,
        win_menu_button_width: 44.0,
        win_unsaved_dot: 6.0,
        win_unsaved_gap: 7.0,
    },
    zoom: ZoomStyle {
        min: 0.1,
        max: 4.0,
        fade_start: 0.75,
        fade_end: 0.5,
        block_mix: 0.3,
    },
    zoom_step: Transition {
        duration: Duration::from_millis(120),
        easing: CubicBezier(0.2, 0.8, 0.2, 1.0),
    },
    ripple: RippleStyle {
        fade: Transition {
            duration: Duration::from_millis(200),
            easing: CubicBezier::EASE_OUT,
        },
        step: Duration::from_millis(40),
    },
    zoom_pill: ZoomPillStyle {
        bg: rgba(59, 52, 44, 0.84),
        ink: hex(0xfffdf8),
        font_size: 12.0,
        pad_x: 11.0,
        pad_y: 5.0,
        bottom: 22.0,
        hold: Duration::from_millis(700),
        fade: Transition {
            duration: Duration::from_millis(400),
            easing: CubicBezier::EASE_IN,
        },
    },
    menu: MenuStyle {
        font: None,
        bg: hex(0xfffdf8),
        ink: hex(0x3b342c),
        hover: hex(0xf1ebe0),
        destructive: hex(0xb5463b),
        // The mockups' ink at 0x18 alpha.
        separator: rgba(59, 52, 44, 0.094),
        separator_pad_y: 5.0,
        separator_width: 1.0,
        radius: 10.0,
        item_radius: 6.0,
        padding: 6.0,
        item_pad_x: 10.0,
        item_pad_y: 6.0,
        item_gap: 24.0,
        min_width: 200.0,
        font_size: 13.0,
        line_height: 1.2,
        font_weight: 500.0,
        heading_size: 11.5,
        heading_opacity: 0.55,
        heading_pad_top: 6.0,
        heading_pad_bottom: 3.0,
        shortcut_opacity: 0.45,
        disabled_opacity: 0.4,
        swatch: 18.0,
        swatch_radius: 9.0,
        swatch_gap: 7.0,
        swatch_ring: 1.5,
        swatch_ring_gap: 2.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 1.0,
                color: rgba(70, 50, 20, 0.08),
            },
            Shadow {
                x: 0.0,
                y: 10.0,
                blur: 30.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.18),
            },
        ],
        left: 8.0,
        gap: 2.0,
    },
    midpoint: MidpointStyle {
        size: 9.0,
        fill: hex(0xffffff),
        stroke: hex(0x8b7760),
        stroke_width: 1.5,
        shadow: Shadow {
            x: 0.0,
            y: 1.0,
            blur: 3.0,
            spread: 0.0,
            color: rgba(0, 0, 0, 0.12),
        },
    },
    tab_picker: TabPickerStyle {
        size: 28.0,
        offset: 30.0,
        radius: 8.0,
        fill: hex(0xfffdf8),
        shadow: [
            Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 1.0,
                color: rgba(70, 50, 20, 0.10),
            },
            Shadow {
                x: 0.0,
                y: 2.0,
                blur: 0.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.12),
            },
            Shadow {
                x: 0.0,
                y: 6.0,
                blur: 14.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.14),
            },
        ],
        glyph: hex(0x3b342c),
        glyph_size: 10.0,
        glyph_width: 1.5,
        stub: rgba(139, 119, 96, 0.55),
        stub_length: 18.0,
        stub_width: 2.0,
        dash: 3.0,
    },
    create: CreateStyle {
        motion: Transition {
            duration: Duration::from_millis(160),
            easing: CubicBezier(0.2, 0.8, 0.2, 1.0),
        },
        rise: -8.0,
        scale: 0.96,
    },
    push: Transition {
        duration: Duration::from_millis(220),
        easing: CubicBezier(0.2, 0.8, 0.2, 1.0),
    },
    follow: Transition {
        duration: Duration::from_millis(120),
        easing: CubicBezier(0.2, 0.8, 0.2, 1.0),
    },
    follow_margin: 48.0,
    key_pan: KeyPanStyle {
        step: 80.0,
        speed: 900.0,
        ease_in: Duration::from_millis(120),
        ease_out: Duration::from_millis(120),
    },
    nudge: 24,
    code: CodeStyle {
        width: 420.0,
        min_width: 280.0,
        grab_width: 6.0,
        bg: hex(0xfbf9f4),
        border: rgba(70, 50, 20, 0.10),
        border_width: 1.0,
        font: "JetBrains Mono",
        font_size: 12.5,
        line_height: 1.65,
        advance: 0.6,
        // The mockup's 46 px under the title bar, plus its 14 px padding.
        pad_top: 60.0,
        pad_bottom: 14.0,
        gutter_width: 40.0,
        gutter_gap: 14.0,
        pad_right: 14.0,
        gutter: hex(0xc9bfb0),
        keyword: hex(0x8b7760),
        id: hex(0x3b342c),
        id_weight: 600.0,
        string: hex(0x5f7d4c),
        direction: hex(0x8a6fae),
        layout: hex(0xa39b8f),
        invalid: hex(0xd9776c),
        invalid_width: 1.0,
        invalid_wavy: true,
        invalid_tint: rgba(217, 119, 108, 0.06),
        // The mockup's 16 x 14 SVG: outline inset .75, divider at 10, fill 10..14.6 x 1.4..12.6.
        toggle: ToggleStyle {
            width: 16.0,
            height: 14.0,
            stroke: 1.3,
            radius: 3.0,
            divider: 10.0,
            fill_inset: 1.4,
            fill_radius: 1.6,
            fill_opacity: 0.55,
            mac_right: 14.0,
        },
    },
};

/// Typewritten index cards on a linen desk. Only what differs from Paper, as in
/// `design/visual-language.md`.
pub const DESK: Theme = Theme {
    name: "Desk",
    font: "Courier Prime",
    font_size: 14.0,
    canvas: hex(0xe6dfd1),
    ink: hex(0x2b2621),
    accent: hex(0x2b2621),
    // No dot grid.
    grid_dot: rgba(0, 0, 0, 0.0),
    // The mockups' two repeating gradients.
    weave: Some([
        Weave {
            color: rgba(120, 100, 70, 0.035),
            width: 1.0,
            period: 3.0,
        },
        Weave {
            color: rgba(120, 100, 70, 0.03),
            width: 1.0,
            period: 4.0,
        },
    ]),
    shape: ShapeStyle {
        radius: 4.0,
        pad_x: 15.0,
        // 12 under the 6 tall tab.
        pad_top: 18.0,
        tab: 6.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 1.0,
                blur: 1.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.18),
            },
            Shadow {
                x: 0.0,
                y: 4.0,
                blur: 10.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.14),
            },
        ],
        ..PAPER.shape
    },
    connector: ConnectorStyle {
        color: hex(0x9b8c74),
        width: 1.5,
        hover: hex(0x2b2621),
        hover_width: 2.5,
        ..PAPER.connector
    },
    zone: ZoneStyle {
        radius: 6.0,
        fill_picker: true,
        inset: Shadow {
            x: 0.0,
            y: 1.0,
            blur: 2.0,
            spread: 0.0,
            color: rgba(70, 50, 20, 0.12),
        },
        fill_mix: 0.16,
        nested_fill_mix: 0.3,
        edge_width: 0.0,
        title_size: 12.0,
        title_weight: 700.0,
        title_upper: true,
        title_spacing: 0.08,
        ..PAPER.zone
    },
    selection: SelectionStyle {
        gap: 0.0,
        ring: 1.5,
        color: hex(0x2b2621),
        text: rgba(199, 154, 62, 0.32),
        marquee: rgba(43, 38, 33, 0.6),
        marquee_fill: rgba(43, 38, 33, 0.05),
        marquee_radius: 2.0,
        marquee_dashed: true,
        lift_y: -4.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 16.0,
                blur: 30.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.28),
            },
            Shadow {
                x: 0.0,
                y: 3.0,
                blur: 6.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.12),
            },
        ],
        fade: Transition {
            duration: Duration::from_millis(160),
            easing: CubicBezier(0.2, 0.8, 0.2, 1.0),
        },
        ..PAPER.selection
    },
    caret: CaretStyle {
        color: hex(0x2b2621),
        ..PAPER.caret
    },
    pastel: [
        hex(0xfbfaf6),
        hex(0xfff3f1),
        hex(0xfff6ec),
        hex(0xfffbea),
        hex(0xf5fbf1),
        hex(0xf2f7fc),
        hex(0xf8f4fc),
    ],
    picker: [
        hex(0xb9b1a3),
        hex(0xd8574c),
        hex(0xe8873a),
        hex(0xe2b52c),
        hex(0x5c9d4d),
        hex(0x4a84c4),
        hex(0x8a66c0),
    ],
    chrome: ChromeStyle {
        title: rgba(43, 38, 33, 0.55),
        button_hover: rgba(70, 50, 20, 0.08),
        ..PAPER.chrome
    },
    zoom: ZoomStyle {
        fade_end: 0.52,
        block_mix: 0.6,
        ..PAPER.zoom
    },
    zoom_pill: ZoomPillStyle {
        bg: rgba(43, 38, 33, 0.88),
        ink: hex(0xfffdf7),
        ..PAPER.zoom_pill
    },
    menu: MenuStyle {
        font: Some("Courier Prime"),
        bg: hex(0xfffdf7),
        ink: hex(0x2b2621),
        hover: hex(0xf3ead6),
        destructive: hex(0xb8483e),
        // The mockups' ink at 0x18 alpha.
        separator: rgba(43, 38, 33, 0.094),
        radius: 4.0,
        item_radius: 2.0,
        swatch_radius: 3.0,
        font_weight: 400.0,
        shadow: [
            Shadow {
                x: 0.0,
                y: 1.0,
                blur: 1.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.2),
            },
            Shadow {
                x: 0.0,
                y: 14.0,
                blur: 30.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.25),
            },
        ],
        ..PAPER.menu
    },
    midpoint: MidpointStyle {
        size: 10.0,
        fill: hex(0xc79a3e),
        stroke: hex(0xfff7e6),
        ..PAPER.midpoint
    },
    tab_picker: TabPickerStyle {
        radius: 3.0,
        fill: hex(0xfffdf7),
        shadow: [
            // Two layers in the mockups.
            Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread: 0.0,
                color: rgba(0, 0, 0, 0.0),
            },
            Shadow {
                x: 0.0,
                y: 1.0,
                blur: 1.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.25),
            },
            Shadow {
                x: 0.0,
                y: 3.0,
                blur: 6.0,
                spread: 0.0,
                color: rgba(70, 50, 20, 0.15),
            },
        ],
        glyph: hex(0x2b2621),
        stub: rgba(43, 38, 33, 0.45),
        ..PAPER.tab_picker
    },
    code: CodeStyle {
        bg: hex(0xfbf8f1),
        border: rgba(70, 50, 20, 0.15),
        font: "Courier Prime",
        font_size: 13.0,
        line_height: 1.6,
        // Courier Prime's advance, 1228 of 2048 units.
        advance: 1228.0 / 2048.0,
        gutter: hex(0xc4b8a2),
        keyword: hex(0x8a6a2c),
        id: hex(0x2b2621),
        string: hex(0x5c7d46),
        direction: hex(0x7a5aa8),
        layout: hex(0xa89d8a),
        invalid: hex(0xd8574c),
        ..PAPER.code
    },
    ..PAPER
};

/// Neo-brutalist: thick outlines, hard offset shadows, punchy pastels. Only what differs from
/// Paper, as in `design/visual-language.md`.
pub const POSTER: Theme = Theme {
    name: "Poster",
    font: "Space Grotesk",
    font_size: 14.5,
    line_height: 1.45,
    ..PAPER
};

/// Every theme, as View > Theme lists them. A new theme is a const over `PAPER` naming what
/// differs, one entry here and its line in the app's `view_menu`. A new field needs only
/// Paper's value: the others inherit it.
pub static THEMES: [&Theme; 3] = [&PAPER, &DESK, &POSTER];
