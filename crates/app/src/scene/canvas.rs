//! What a canvas pane shows. Pure: from a page's items and a [`Viewport`], the
//! marks to draw in screen points, and the screen box that changed since the
//! last scene, so `platform` repaints only that box. Items off the screen are
//! not in the draw list, so the cost of a frame follows what is visible, not
//! the size of the page.
//!
//! Pen lines are smoothed here, not in `platform` ([`smooth`]): the line
//! keeps a constant width, its ends and no corners or bumps.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_6;
use std::path::PathBuf;

use super::sidebar::Status;
use super::{button, glyphs};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Size, Viewport};

/// An item's id, unique within its canvas: the number of the server's `i<n>`.
/// The id is how a scene knows an item it drew last time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

/// One item on a page. Later items draw on top of earlier ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub colour: Rgb,
    pub shape: Shape,
}

/// What an item is, in canvas points. Widths are line widths.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A pen line through `points` at a constant width.
    Stroke { points: Vec<Point>, width: f64 },
    /// Text set inside `frame`, `size` points high. A canvas text has the
    /// `column` it is stored in, and is laid out in the core's rows
    /// ([`glyphs::rows`]); chrome's text (`None`) in the system font's own.
    Text { frame: Rect, text: String, size: f64, column: Option<f64> },
    /// The outline of `frame`.
    Box { frame: Rect, width: f64 },
    /// A line from `from` to `to`, with its head at `to`. `bound` holds the
    /// items its two ends point at, if any: a bound end follows its item.
    Arrow { from: Point, to: Point, width: f64, bound: [Option<ItemId>; 2] },
    /// An agent UI on the canvas, in `frame`: the UI's `name` and the
    /// showing (`open`, `pick-slot#12`) whose page shows in it.
    Live { frame: Rect, name: String, open: String },
    /// The "send to agent" button in `frame`, with the status of the agent
    /// of its canvas's workspace (`scene::button`).
    Button { frame: Rect, status: Option<Status> },
    /// One page of a PDF, or part of one (docs/design/pdf.md), stretched
    /// to fill `frame`: page `pdf_page` (from 1) of the PDF that is blob
    /// `hash`, `page_size` in page points as displayed, and the part of it
    /// shown, `crop`, in page points (`None`: the whole page). `name` is
    /// the PDF's file name when it was opened.
    Pdf {
        frame: Rect,
        hash: String,
        pdf_page: u32,
        page_size: (f64, f64),
        crop: Option<Rect>,
        name: Option<String>,
    },
    /// App chrome over a page, never an item of the server's
    /// (`scene::select`, `scene::shapes`): `frame` filled with `fill`, if
    /// any, under its outline `width` wide, dashed when `dashed`.
    Panel { frame: Rect, fill: Option<Fill>, width: f64, dashed: bool },
    /// The caret of a text being typed, also chrome: a bar `width` wide
    /// after the last character of `text` set `size` high in `frame`.
    Caret { frame: Rect, text: String, size: f64, width: f64 },
}

/// The paint inside a [`Shape::Panel`]: `colour` at `alpha` (0 to 1), over
/// what is under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub colour: Rgb,
    pub alpha: f64,
}

/// One thing to draw: a [`Shape`] placed by the viewport, in screen points.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// A line from `start` through each cubic curve: two control points, then
    /// the curve's end. A line of one point is one curve of no length, which
    /// a round cap draws as a dot.
    Stroke {
        start: Point,
        curves: Vec<[Point; 3]>,
        width: f64,
    },
    /// `text` laid out `size` canvas points high in a column `width` canvas
    /// points wide, its lines `line` apart (0: the font's own), drawn from `at` at
    /// `zoom` times that size. The layout is in canvas points so a pan or a
    /// zoom never lays the text out again.
    Text {
        at: Point,
        width: f64,
        text: String,
        size: f64,
        line: f64,
        zoom: f64,
    },
    Box {
        frame: Rect,
        width: f64,
    },
    /// The shaft from `from` to `to`, and the head: a line from `to` to each barb.
    Arrow {
        from: Point,
        to: Point,
        barbs: [Point; 2],
        width: f64,
    },
    /// A live item's box, and its UI's `name` laid out like a text, from
    /// `at`, inside its top left corner: what shows until, or unless, the
    /// page of the showing `open` shows over it, `zoom` times its size.
    Live {
        frame: Rect,
        open: String,
        name: String,
        at: Point,
        size: f64,
        zoom: f64,
    },
    /// The button's outline, `width` wide, and its `label` (its status mark
    /// first) laid out like a text from `at`, `zoom` times its size.
    Button {
        frame: Rect,
        label: String,
        at: Point,
        size: f64,
        width: f64,
        zoom: f64,
    },
    /// A `pdf` item: the part `shown` of page `page` (from 1), in page
    /// points as displayed, stretched to fill `frame`, on white, from the
    /// PDF at `file` once it has come ([`build_with`]) for the blob `hash`. Until then, or when the PDF cannot be read, the frame's
    /// outline with `PDF` laid out like a text from `at`, `zoom` times
    /// `size`.
    Pdf {
        frame: Rect,
        page: u32,
        shown: Rect,
        hash: String,
        file: Option<PathBuf>,
        at: Point,
        size: f64,
        zoom: f64,
    },
    /// The panel's `frame`, filled, then outlined `width` wide.
    Panel {
        frame: Rect,
        fill: Option<Fill>,
        width: f64,
        dashed: bool,
    },
    /// A bar `width` wide, as tall as a line, after the last character of
    /// `text` laid out like a [`Mark::Text`] from `at`: only the layout
    /// knows where that character ends.
    Caret {
        at: Point,
        column: f64,
        text: String,
        size: f64,
        line: f64,
        zoom: f64,
        width: f64,
    },
}

/// A mark, the item it draws, its colour and the screen box its ink covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    pub id: ItemId,
    pub bounds: Rect,
    pub colour: Rgb,
    pub mark: Mark,
}

/// One canvas pane, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub viewport: Viewport,
    /// The pane's size in screen points.
    pub screen: Size,
    /// The pane's paper, under every mark.
    pub background: Rgb,
    /// The dots on the paper, over the background and under every mark
    /// (`scene::grid`); `None` for plain paper. Set by [`Scene::on`].
    pub grid: Option<super::grid::Grid>,
    /// What to draw, bottom first.
    pub draws: Vec<Draw>,
    /// The screen box to repaint since the last scene; `None` when nothing
    /// on the screen changed.
    pub changed: Option<Rect>,
    /// When the oldest pen event this scene is the first to show happened,
    /// on the clock the pen's events carry (seconds since the machine
    /// started); `None` for a scene no pen event made. Only to measure the
    /// delay from pen to paint (SPEC O2).
    pub pen_event: Option<f64>,
}

/// The length of an arrow's head barbs, in canvas points.
const HEAD_LENGTH: f64 = 12.0;
/// The angle between an arrow's shaft and each barb.
const HEAD_ANGLE: f64 = FRAC_PI_6;
/// How high a live item's name is set, and how far inside its corner, in
/// canvas points.
const NAME_SIZE: f64 = 16.0;
const NAME_INSET: f64 = 8.0;
/// The least contrast (WCAG ratio) ink keeps with the paper. Ink closer to
/// the background than this, such as black ink on a dark theme, draws in
/// the theme's text colour, so nothing on a page is invisible.
const MIN_CONTRAST: f64 = 1.5;
/// The outline and the word of a `pdf` item whose PDF has not come.
const PDF_GREY: Rgb = Rgb { r: 0xc8, g: 0xc8, b: 0xc8 };

/// The scene for `items` seen through `viewport` on a pane of `screen` size,
/// on `theme`'s background. Pass the scene before this one as `last` to
/// learn what changed: with no last scene, or after the viewport, the size
/// or the background changed, the whole pane did. No `pdf` item has its
/// file: the tests' scenes.
#[cfg(test)]
pub fn build<'a>(
    items: impl IntoIterator<Item = &'a Item>,
    viewport: Viewport,
    screen: Size,
    theme: &Theme,
    last: Option<&Scene>,
) -> Scene {
    build_with(items, (viewport, screen), theme, last, &|_| None)
}

/// [`build`], each `pdf` item drawn from the file `files` gives for its
/// blob id, if any (docs/design/pdf.md): a file that comes repaints it.
pub fn build_with<'a>(
    items: impl IntoIterator<Item = &'a Item>,
    (viewport, screen): (Viewport, Size),
    theme: &Theme,
    last: Option<&Scene>,
    files: &dyn Fn(&str) -> Option<PathBuf>,
) -> Scene {
    let visible = viewport.visible(screen);
    let draws: Vec<Draw> = items
        .into_iter()
        .filter_map(|item| {
            let bounds = item.shape.bounds()?;
            bounds.intersection(visible)?;
            Some(Draw {
                id: item.id,
                bounds: viewport.rect_to_screen(bounds),
                colour: match item.shape {
                    Shape::Button { status, .. } => button::colour(status, theme),
                    Shape::Pdf { .. } => PDF_GREY,
                    _ => legible(item.colour, theme),
                },
                mark: match item.shape.mark(viewport) {
                    Mark::Pdf { hash, frame, page, shown, at, size, zoom, .. } => {
                        let file = files(&hash);
                        Mark::Pdf { frame, page, shown, hash, file, at, size, zoom }
                    }
                    mark => mark,
                },
            })
        })
        .collect();
    let background = theme.background;
    let pane = screen.rect();
    let changed = match last {
        Some(last)
            if last.viewport == viewport
                && last.screen == screen
                && last.background == background =>
        {
            changed_between(&last.draws, &draws).and_then(|r| r.intersection(pane))
        }
        _ => Some(pane),
    };
    Scene { viewport, screen, background, grid: None, draws, changed, pen_event: None }
}

impl Scene {
    /// The part of `dirty` to paint: what of it lies inside the pane, or
    /// `None` when none does. Paint is cut off at the pane's edges. A mark
    /// that runs past an edge would otherwise land on what lies beside the
    /// pane (the sidebar, another pane), which the pane never paints again.
    pub fn clip(&self, dirty: Rect) -> Option<Rect> {
        dirty.intersection(self.screen.rect())
    }

    /// The draws whose ink touches `area`, bottom first.
    pub fn touching(&self, area: Rect) -> impl Iterator<Item = &Draw> {
        self.draws.iter().filter(move |d| d.bounds.intersection(area).is_some())
    }
}

/// The box around every draw that is in one list and not, unchanged, in the
/// other. A pen line that only grew at its end (the stroke under the pen)
/// counts only its new end, so a long stroke does not repaint whole at
/// every pen event.
fn changed_between(old: &[Draw], new: &[Draw]) -> Option<Rect> {
    let before: HashMap<ItemId, &Draw> = old.iter().map(|d| (d.id, d)).collect();
    let after: HashMap<ItemId, &Draw> = new.iter().map(|d| (d.id, d)).collect();
    let gone = old.iter().filter(|d| !after.contains_key(&d.id)).map(|d| d.bounds);
    let came = new.iter().filter_map(|d| match before.get(&d.id) {
        None => Some(d.bounds),
        Some(was) if *was == d => None,
        Some(was) => Some(grown(was, d).unwrap_or_else(|| was.bounds.union(d.bounds))),
    });
    gone.chain(came).reduce(Rect::union)
}

/// The screen box of what changed between `old` and `new`, when both are
/// the same pen line from the same start and only its end changed (the
/// stroke under the pen). Smoothing changes only a line's last few curves
/// when a point comes, so the box holds the curves of each after those they
/// share. A cubic stays inside its points' box.
#[expect(clippy::float_cmp, reason = "the same line's width is the same value, from one product")]
fn grown(old: &Draw, new: &Draw) -> Option<Rect> {
    let (
        Mark::Stroke { start: a, curves: was, width: w },
        Mark::Stroke { start: b, curves: now, width },
    ) = (&old.mark, &new.mark)
    else {
        return None;
    };
    let shared = was.iter().zip(now).take_while(|(x, y)| x == y).count();
    let same = a == b && w == width && old.colour == new.colour;
    if !same {
        return None;
    }
    let from = if shared == 0 { *a } else { was[shared - 1][2] };
    let points = was[shared..].iter().chain(&now[shared..]).flatten().copied();
    // One point more on each side for the edge's smoothing.
    Rect::around(points.chain([from])).map(|r| r.outset(width / 2.0 + 1.0))
}

impl Shape {
    /// The canvas box the shape's ink covers; `None` when it has no ink.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Stroke { points, width } => {
                Rect::around(points.iter().copied()).map(|r| r.outset(width / 2.0))
            }
            Self::Text { frame, .. }
            | Self::Live { frame, .. }
            | Self::Button { frame, .. }
            | Self::Pdf { frame, .. } => Some(*frame),
            Self::Box { frame, width } | Self::Panel { frame, width, .. } => {
                Some(frame.outset(width / 2.0))
            }
            // The text's frame holds every character, and the bar may stand
            // just past it.
            Self::Caret { frame, width, .. } => Some(frame.outset(*width)),
            Self::Arrow { from, to, width, .. } => {
                Rect::around([*from, *to]).map(|r| r.outset(width / 2.0 + HEAD_LENGTH))
            }
        }
    }

    /// The shape moved by `by` canvas points.
    pub fn moved(&self, by: Point) -> Shape {
        let at = |p: &Point| Point { x: p.x + by.x, y: p.y + by.y };
        let shift = |r: &Rect| Rect { x: r.x + by.x, y: r.y + by.y, ..*r };
        match self {
            Self::Stroke { points, width } => {
                Self::Stroke { points: points.iter().map(at).collect(), width: *width }
            }
            Self::Text { frame, text, size, column } => {
                Self::Text { frame: shift(frame), text: text.clone(), size: *size, column: *column }
            }
            Self::Box { frame, width } => Self::Box { frame: shift(frame), width: *width },
            Self::Button { frame, status } => Self::Button { frame: shift(frame), status: *status },
            Self::Live { frame, name, open } => {
                Self::Live { frame: shift(frame), name: name.clone(), open: open.clone() }
            }
            Self::Arrow { from, to, width, bound } => {
                Self::Arrow { from: at(from), to: at(to), width: *width, bound: *bound }
            }
            Self::Pdf { frame, .. } => self.reframed(shift(frame)),
            Self::Panel { frame, fill, width, dashed } => {
                Self::Panel { frame: shift(frame), fill: *fill, width: *width, dashed: *dashed }
            }
            Self::Caret { frame, text, size, width } => {
                Self::Caret { frame: shift(frame), text: text.clone(), size: *size, width: *width }
            }
        }
    }

    /// The shape on the screen.
    fn mark(&self, view: Viewport) -> Mark {
        let zoom = view.zoom();
        match self {
            Self::Stroke { points, width } => {
                // Smoothed in canvas points, so a zoom does not change the
                // line's shape; the screen is the canvas scaled and moved.
                let curves = smooth::smooth(points, *width);
                let curves = curves.iter().map(|c| c.map(|p| view.to_screen(p))).collect();
                // A stroke has a point, or it has no bounds and is not drawn.
                let start = points.first().map_or(Point { x: 0.0, y: 0.0 }, |p| view.to_screen(*p));
                Mark::Stroke { start, curves, width: width * zoom }
            }
            Self::Text { frame, text, size, column } => {
                let (width, text, size, line) = match column {
                    Some(c) => (
                        glyphs::UNWRAPPED,
                        glyphs::rows(text, *size, *c),
                        size * glyphs::FONT,
                        *size,
                    ),
                    None => (frame.width, text.clone(), *size, 0.0),
                };
                let at = view.to_screen(Point { x: frame.x, y: frame.y });
                Mark::Text { at, width, text, size, line, zoom }
            }
            Self::Box { frame, width } => {
                Mark::Box { frame: view.rect_to_screen(*frame), width: width * zoom }
            }
            Self::Arrow { from, to, width, .. } => Mark::Arrow {
                from: view.to_screen(*from),
                to: view.to_screen(*to),
                barbs: barbs(*from, *to).map(|p| view.to_screen(p)),
                width: width * zoom,
            },
            Self::Live { frame, name, open } => Mark::Live {
                frame: view.rect_to_screen(*frame),
                open: open.clone(),
                name: name.clone(),
                at: view.to_screen(Point { x: frame.x + NAME_INSET, y: frame.y + NAME_INSET }),
                size: NAME_SIZE,
                zoom,
            },
            Self::Button { frame, status } => button::mark(*frame, *status, view),
            Self::Pdf { frame, pdf_page, hash, .. } => Mark::Pdf {
                frame: view.rect_to_screen(*frame),
                page: *pdf_page,
                shown: self.shown().unwrap_or(*frame),
                hash: hash.clone(),
                file: None,
                at: view.to_screen(Point { x: frame.x + NAME_INSET, y: frame.y + NAME_INSET }),
                size: NAME_SIZE,
                zoom,
            },
            Self::Panel { frame, fill, width, dashed } => Mark::Panel {
                frame: view.rect_to_screen(*frame),
                fill: *fill,
                width: width * zoom,
                dashed: *dashed,
            },
            // The caret is in a canvas text being typed, in its rows.
            Self::Caret { frame, text, size, width } => Mark::Caret {
                at: view.to_screen(Point { x: frame.x, y: frame.y }),
                column: glyphs::UNWRAPPED,
                text: text.clone(),
                size: size * glyphs::FONT,
                line: *size,
                zoom,
                width: width * zoom,
            },
        }
    }
}

/// `ink`, or the theme's text colour when `ink` would not show on its
/// background.
fn legible(ink: Rgb, theme: &Theme) -> Rgb {
    let (a, b) = (luminance(ink), luminance(theme.background));
    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    if contrast < MIN_CONTRAST { theme.text } else { ink }
}

/// The relative luminance of an sRGB colour, 0 (black) to 1 (white).
fn luminance(c: Rgb) -> f64 {
    let linear = |v: u8| (f64::from(v) / 255.0).powf(2.2);
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// The ends of an arrow's two barbs, in canvas points. They point back along
/// the shaft; a shaft of no length points right, so its head points left.
fn barbs(from: Point, to: Point) -> [Point; 2] {
    let heading = (to.y - from.y).atan2(to.x - from.x);
    [heading - HEAD_ANGLE, heading + HEAD_ANGLE].map(|angle| Point {
        x: to.x - HEAD_LENGTH * angle.cos(),
        y: to.y - HEAD_LENGTH * angle.sin(),
    })
}

mod smooth;

#[cfg(test)]
mod tests;
