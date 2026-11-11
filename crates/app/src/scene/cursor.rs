//! The pointer over a canvas pane: it says what the pen does there. Pure:
//! the platform draws or picks the cursor a [`Cursor`] names, over the
//! canvas panes only; anywhere else (the sidebar, an agent UI, a live item,
//! the chrome) it keeps the system's arrow.

use crate::config::{Rgb, Theme};
use crate::scene::ink::PEN_WIDTH;
use crate::scene::select::ERASER;
use crate::scene::tools::{Tool, Tools};

/// What the pointer looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cursor {
    /// The system's arrow.
    Arrow,
    /// A ring `diameter` points across in `colour`, centred on the point.
    Ring { colour: Rgb, diameter: f64 },
    /// Cross hairs: the point where a lasso or a shape starts.
    Crosshair,
    /// The I-beam: where a text goes.
    IBeam,
}

/// How far across the pen's ring is, in points: wider than its line, so it
/// shows round the point.
pub const PEN_RING: f64 = PEN_WIDTH * 4.0;

/// The pointer over a canvas pane for `tools` in `theme`: the pen is a ring
/// in its colour, the eraser a ring as wide as it erases, the lasso and the
/// shapes cross hairs, text an I-beam. An open pop-up takes a press, not the
/// pen, so it has the arrow.
pub fn over_canvas(tools: &Tools, theme: &Theme) -> Cursor {
    if tools.open_kind().is_some() {
        return Cursor::Arrow;
    }
    match tools.tool() {
        Tool::Pen => Cursor::Ring { colour: tools.colour(), diameter: PEN_RING },
        Tool::Eraser => Cursor::Ring { colour: theme.muted, diameter: ERASER * 2.0 },
        Tool::Lasso | Tool::Box | Tool::Arrow => Cursor::Crosshair,
        Tool::Text => Cursor::IBeam,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Command;
    use crate::layout::{Point, Rect};
    use crate::scene::tools::Kind;

    const T: Theme = Theme {
        background: Rgb { r: 0, g: 0, b: 0 },
        text: Rgb { r: 1, g: 1, b: 1 },
        muted: Rgb { r: 2, g: 2, b: 2 },
        error: Rgb { r: 3, g: 3, b: 3 },
        focus: Rgb { r: 4, g: 4, b: 4 },
    };

    #[test]
    fn each_tool_has_its_cursor_and_an_open_popup_the_arrow() {
        let palette = crate::config::Palette::default();
        let ink = palette.colours()[0];
        let mut tools = Tools::new(palette, Point { x: 10.0, y: 10.0 });
        let pen = Cursor::Ring { colour: ink, diameter: PEN_RING };
        assert_eq!(over_canvas(&tools, &T), pen);
        let each = [
            (Command::ToolEraser, Cursor::Ring { colour: T.muted, diameter: ERASER * 2.0 }),
            (Command::ToolLasso, Cursor::Crosshair),
            (Command::ToolText, Cursor::IBeam),
            (Command::ToolBox, Cursor::Crosshair),
            (Command::ToolArrow, Cursor::Crosshair),
            (Command::ToolPen, pen),
        ];
        for (command, cursor) in each {
            assert!(tools.command(command));
            assert_eq!(over_canvas(&tools, &T), cursor, "{command:?}");
        }
        tools.open(Kind::Colour, Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 });
        assert_eq!(over_canvas(&tools, &T), Cursor::Arrow);
    }

    #[test]
    fn the_pen_s_eraser_end_has_the_eraser_s_ring_until_the_tip_is_back() {
        let palette = crate::config::Palette::default();
        let mut tools = Tools::new(palette, Point { x: 10.0, y: 10.0 });
        assert!(tools.command(Command::ToolLasso));
        tools.set_eraser_end(true);
        let ring = Cursor::Ring { colour: T.muted, diameter: ERASER * 2.0 };
        assert_eq!(over_canvas(&tools, &T), ring);
        tools.set_eraser_end(false);
        assert_eq!(over_canvas(&tools, &T), Cursor::Crosshair);
    }
}
