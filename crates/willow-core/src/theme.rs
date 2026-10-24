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
    pub pad_y: f32,
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
}

/// A zone: a rounded rectangle of its pastel mixed into the canvas, with an inset edge and a
/// title at the top-left. Sizes are canvas units unless named screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneStyle {
    pub radius: f32,
    /// How far the fill moves from the canvas toward the pastel; deeper for nested zones.
    pub fill_mix: f32,
    pub nested_fill_mix: f32,
    /// The inset edge: the pastel moved this far toward the picker color, `edge_width` wide.
    pub edge_mix: f32,
    pub edge_width: f32,
    /// Title in the theme font, ink color; never smaller than `title_min_screen` screen px and
    /// never faded. Its top-left sits `title_x`, `title_y` in, constant on screen when zoomed out.
    pub title_size: f32,
    pub title_weight: f32,
    pub title_min_screen: f32,
    pub title_x: f32,
    pub title_y: f32,
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

/// App menus drawn in the window (Windows' menu button), in screen pixels. Text is the
/// system UI font. macOS uses the native menus instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuStyle {
    pub bg: Rgba,
    pub ink: Rgba,
    pub hover: Rgba,
    pub separator: Rgba,
    /// Space above and below a separator line.
    pub separator_pad_y: f32,
    pub radius: f32,
    pub item_radius: f32,
    /// Inside the menu, around the items.
    pub padding: f32,
    pub item_pad_x: f32,
    pub item_pad_y: f32,
    pub min_width: f32,
    pub font_size: f32,
    /// Shortcut text and the submenu arrow.
    pub shortcut_opacity: f32,
    /// A command that can't run now (Undo with nothing to undo).
    pub disabled_opacity: f32,
    /// Ring, then drop shadow.
    pub shadow: [Shadow; 2],
    /// The menu's left edge from the window's.
    pub left: f32,
    /// Between the title bar and the menu, and between a menu and its cascade.
    pub gap: f32,
}

/// The window's title bar, in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromeStyle {
    /// Window title text.
    pub title: Rgba,
    /// Windows title-bar button hover fill.
    pub button_hover: Rgba,
    /// macOS: the transparent bar over the canvas, and its title size.
    pub mac_height: f32,
    pub mac_title_size: f32,
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
    pub shape: ShapeStyle,
    /// Edge-to-edge distance, either direction, from a shape to a new or auto-placed shape
    /// linked to it.
    pub creation_gap: f32,
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
    shape: ShapeStyle {
        radius: 14.0,
        pad_x: 16.0,
        pad_y: 13.0,
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
    connector: ConnectorStyle {
        color: hex(0xcbbfae),
        width: 2.0,
        min_screen: 1.0,
    },
    zone: ZoneStyle {
        radius: 22.0,
        fill_mix: 0.45,
        nested_fill_mix: 0.7,
        edge_mix: 0.35,
        edge_width: 1.0,
        title_size: 13.0,
        title_weight: 600.0,
        title_min_screen: 12.0,
        title_x: 16.0,
        title_y: 12.0,
        padding: 40.0,
    },
    selection: SelectionStyle {
        gap: 2.0,
        ring: 2.0,
        color: hex(0x8b7760),
        text: rgba(139, 119, 96, 0.22),
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
        title: rgba(59, 52, 44, 0.55),
        button_hover: rgba(70, 50, 20, 0.07),
        mac_height: 38.0,
        mac_title_size: 13.0,
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
        bg: hex(0xfffdf8),
        ink: hex(0x3b342c),
        hover: hex(0xf1ebe0),
        // The mockups' ink at 0x18 alpha.
        separator: rgba(59, 52, 44, 0.094),
        separator_pad_y: 5.0,
        radius: 10.0,
        item_radius: 6.0,
        padding: 6.0,
        item_pad_x: 10.0,
        item_pad_y: 6.0,
        min_width: 200.0,
        font_size: 13.0,
        shortcut_opacity: 0.45,
        disabled_opacity: 0.4,
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
};
