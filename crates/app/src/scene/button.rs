//! The built-in "send to agent" button (SPEC section 8) on a canvas page.
//! Pure: what it shows.
//!
//! The button is an item of the canvas (`kind` `button`): it moves, scales
//! and is selected, deleted and undone like a box. It draws as an outline
//! with its label, after one mark for the status of the agent of the
//! canvas's workspace: the sidebar's mark and colour (working, blocked,
//! done; a ring when the agent has said nothing, or there is none). A tap
//! on it slings what is new on its page (`canvases::button`).

use std::borrow::Cow;

use super::canvas::{Item, Mark, Shape};
use super::sidebar::{self, Status};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Viewport};

/// What the button says, after its mark.
pub const LABEL: &str = "send to agent";
/// How high its label is set, and how far inside its left edge, in canvas
/// points; its outline's width.
const SIZE: f64 = 16.0;
const INSET: f64 = 12.0;
const LINE: f64 = 1.5;

/// `items` as they show while the agent of their canvas is at `status`:
/// each button carries it.
pub fn shown(items: Vec<Cow<'_, Item>>, status: Option<Status>) -> Vec<Cow<'_, Item>> {
    items
        .into_iter()
        .map(|item| match &item.shape {
            Shape::Button { frame, status: was } if *was != status => {
                let shape = Shape::Button { frame: *frame, status };
                Cow::Owned(Item { shape, ..item.into_owned() })
            }
            _ => item,
        })
        .collect()
}

/// The first button of `items` under `at`, a canvas point: its frame.
pub fn under(items: &[Cow<'_, Item>], at: Point) -> Option<Rect> {
    items.iter().find_map(|i| match i.shape {
        Shape::Button { frame, .. } if frame.contains(at) => Some(frame),
        _ => None,
    })
}

/// The first button of `items`: its frame.
pub fn first(items: &[Cow<'_, Item>]) -> Option<Rect> {
    items.iter().find_map(|i| match i.shape {
        Shape::Button { frame, .. } => Some(frame),
        _ => None,
    })
}

/// The button in `frame` with its agent at `status`, seen through `view`.
pub(super) fn mark(frame: Rect, status: Option<Status>, view: Viewport) -> Mark {
    let zoom = view.zoom();
    let at = Point { x: frame.x + INSET, y: frame.y + (frame.height - SIZE) / 2.0 };
    Mark::Button {
        frame: view.rect_to_screen(frame),
        label: format!("{} {LABEL}", sidebar::glyph(status)),
        at: view.to_screen(at),
        size: SIZE,
        width: LINE * zoom,
        zoom,
    }
}

/// The colour of the button with its agent at `status`, in `theme`: the
/// status mark's, as in the sidebar.
pub(super) fn colour(status: Option<Status>, theme: &Theme) -> Rgb {
    sidebar::colour(status, theme)
}

#[cfg(test)]
mod tests;
