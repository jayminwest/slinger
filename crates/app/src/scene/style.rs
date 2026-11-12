//! The window's one look (SPEC P6: one theme): the named colours and sizes
//! of the chrome the app draws itself. Every surface takes its panel,
//! hairline, chip, field and selection from here, so they match, and none
//! writes a colour of its own.
//!
//! Each colour is mixed from the [`Theme`]: the background moved a little
//! toward the text (a panel, a hairline, a chip) or toward the focus colour
//! (a selected row). So a rethemed window keeps its contrasts. With the
//! default theme they are the design's: panel `#262626`, hairline
//! `#3d3d3d`, chip `#363636`, footer `#232323`, selected row `#2e4a6e`.
//! Pure: [`rules`] also decides where the window's hairlines go.

use crate::config::{Rgb, Theme};
use crate::layout::Rect;

/// A panel's corners (the key overlay, the palette), in points.
pub const PANEL_RADIUS: f64 = 12.0;
/// A one-line entry's corners.
pub const FIELD_RADIUS: f64 = 8.0;
/// A row's highlight's corners (the palette's selection, a row under the pen).
pub const ROW_RADIUS: f64 = 8.0;
/// A key chip's corners, and its text's size.
pub const CHIP_RADIUS: f64 = 4.0;
pub const CHIP_TEXT: f64 = 12.0;
/// How wide a hairline is, in points.
pub const HAIRLINE: f64 = 1.0;
/// The least height of a row the pen taps.
pub const TAP: f64 = 28.0;

/// How far each colour moves from the background toward the text, out of 255.
const SIDEBAR: u16 = 4;
const FOOTER: u16 = 6;
const PANEL: u16 = 10;
const HOVER: u16 = 20;
const CHIP: u16 = 31;
const EDGE: u16 = 40;
/// How far a selected row moves from the background toward the focus colour.
const SELECTED: u16 = 91;
/// How far a selected row's second label moves from the text toward the focus colour.
const SELECTED_LABEL: u16 = 56;

/// `from` moved `by` (out of 255) toward `to`, channel by channel.
pub fn mix(from: Rgb, to: Rgb, by: u16) -> Rgb {
    let by = by.min(255);
    let toward = |from: u8, to: u8| {
        let (from, to) = (u32::from(from), u32::from(to));
        let mixed = (from * u32::from(255 - by) + to * u32::from(by) + 127) / 255;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    Rgb { r: toward(from.r, to.r), g: toward(from.g, to.g), b: toward(from.b, to.b) }
}

/// The sidebar's fill: a shade off the canvas.
pub fn sidebar(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, SIDEBAR)
}

/// A pane's footer's fill.
pub fn footer(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, FOOTER)
}

/// A panel's and an entry's fill.
pub fn panel(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, PANEL)
}

/// A row under the pen.
pub fn hover(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, HOVER)
}

/// A key chip's fill.
pub fn chip(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, CHIP)
}

/// A hairline: a panel's edge, the sidebar's edge, a divider.
pub fn edge(theme: &Theme) -> Rgb {
    mix(theme.background, theme.text, EDGE)
}

/// A selected row's fill: the focus colour, quietened.
pub fn selected(theme: &Theme) -> Rgb {
    mix(theme.background, theme.focus, SELECTED)
}

/// A muted label on a selected row: readable on [`selected`].
pub fn selected_label(theme: &Theme) -> Rgb {
    mix(theme.text, theme.focus, SELECTED_LABEL)
}

/// The window's fills and hairlines under the focus border, in `theme`'s
/// colours: the sidebar's fill and its edge when it takes `sidebar` (a
/// zero-wide sidebar is closed), a hairline under the title bar across
/// `window`, and a divider along each edge one pane in `panes` shares with
/// another (an edge inside `area`, the panes' share of the window).
pub fn rules(theme: &Theme, window: Rect, sidebar: Rect, area: Rect, panes: &[Rect]) -> Vec<Rule> {
    let mut rules = Vec::new();
    if sidebar.width > 0.0 {
        rules.push(Rule { frame: sidebar, colour: self::sidebar(theme) });
        let x = sidebar.x + sidebar.width - HAIRLINE;
        rules.push(Rule { frame: Rect { x, width: HAIRLINE, ..sidebar }, colour: edge(theme) });
    }
    let top = Rect { height: HAIRLINE, ..window };
    rules.push(Rule { frame: top, colour: edge(theme) });
    let (right, bottom) = (area.x + area.width, area.y + area.height);
    for pane in panes {
        let (end, foot) = (pane.x + pane.width, pane.y + pane.height);
        if end < right - HAIRLINE {
            let frame = Rect { x: end - HAIRLINE / 2.0, width: HAIRLINE, ..*pane };
            rules.push(Rule { frame, colour: edge(theme) });
        }
        if foot < bottom - HAIRLINE {
            let frame = Rect { y: foot - HAIRLINE / 2.0, height: HAIRLINE, ..*pane };
            rules.push(Rule { frame, colour: edge(theme) });
        }
    }
    rules
}

/// A filled rectangle with no text: a fill or a hairline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub frame: Rect,
    pub colour: Rgb,
}

#[cfg(test)]
mod tests;
