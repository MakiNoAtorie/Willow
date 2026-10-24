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

/// The selected shape's look: a canvas-colored `gap`, then a `ring` of `color`, and `shadow`
/// in place of the resting one. Fades in and out over `fade`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionStyle {
    pub gap: f32,
    pub ring: f32,
    pub color: Rgba,
    /// Same layer count as `ShapeStyle::shadow`, so one can ease into the other.
    pub shadow: [Shadow; 2],
    pub fade: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub name: &'static str,
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
    pub selection: SelectionStyle,
    /// Shape fill per color, indexed in `Color::ALL` order.
    pub pastel: [Rgba; 7],
    /// Saturated swatch per color, indexed in `Color::ALL` order.
    pub picker: [Rgba; 7],
    /// Window title text.
    pub chrome_title: Rgba,
    /// Windows title-bar button hover fill.
    pub chrome_button_hover: Rgba,
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
    selection: SelectionStyle {
        gap: 2.0,
        ring: 2.0,
        color: hex(0x8b7760),
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
        fade: Duration::from_millis(120),
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
    chrome_title: rgba(59, 52, 44, 0.55),
    chrome_button_hover: rgba(70, 50, 20, 0.07),
};
