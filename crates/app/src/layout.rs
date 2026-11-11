//! Layout math. Pure: sizes in, rectangles out, no `AppKit`, so every branch is
//! unit-tested (this file has a 100% coverage floor). Units are points. The
//! origin is the top left; `platform` converts to `AppKit`'s bottom left.
//! [`Viewport`] maps the endless canvas onto the screen: pan, zoom about a
//! point, and the canvas box that is visible. [`Tree`] tiles the window with
//! panes (`tree`).

pub mod tree;

pub use tree::{Axis, Direction, Divider, PaneId, Tree};

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A rectangle, by its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A point. On the canvas or on the screen, as the name of the value says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Rect {
    /// The smallest rectangle that holds every point; `None` for no points.
    pub fn around(points: impl IntoIterator<Item = Point>) -> Option<Self> {
        points.into_iter().fold(None, |acc, p| {
            let at = Self { x: p.x, y: p.y, width: 0.0, height: 0.0 };
            Some(acc.map_or(at, |r: Self| r.union(at)))
        })
    }

    /// The smallest rectangle that holds both.
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self { x, y, width: right - x, height: bottom - y }
    }

    /// The part both share, edges included; `None` when they do not touch.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right >= x && bottom >= y).then_some(Self { x, y, width: right - x, height: bottom - y })
    }

    /// This rectangle grown by `by` on every side.
    pub fn outset(self, by: f64) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            width: self.width + 2.0 * by,
            height: self.height + 2.0 * by,
        }
    }
}

/// The closest a canvas can zoom out: one screen point shows ten canvas points.
pub const MIN_ZOOM: f64 = 0.1;
/// The closest a canvas can zoom in: one canvas point fills eight screen points.
pub const MAX_ZOOM: f64 = 8.0;

/// Where the screen looks onto an endless canvas: the canvas point at the
/// screen's top left, and the zoom (screen points per canvas point, always in
/// `MIN_ZOOM..=MAX_ZOOM`). The canvas has no edge, so any finite origin is
/// valid. A value is the whole state, so a view keeps the last one and asks
/// for the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    origin: Point,
    zoom: f64,
}

impl Default for Viewport {
    /// The canvas origin at the screen's top left, at 100%.
    fn default() -> Self {
        Self { origin: Point { x: 0.0, y: 0.0 }, zoom: 1.0 }
    }
}

impl Viewport {
    /// Screen points per canvas point.
    pub fn zoom(self) -> f64 {
        self.zoom
    }

    /// Where a canvas point shows on the screen.
    pub fn to_screen(self, canvas: Point) -> Point {
        Point {
            x: (canvas.x - self.origin.x) * self.zoom,
            y: (canvas.y - self.origin.y) * self.zoom,
        }
    }

    /// The canvas point under a screen point.
    pub fn to_canvas(self, screen: Point) -> Point {
        Point { x: self.origin.x + screen.x / self.zoom, y: self.origin.y + screen.y / self.zoom }
    }

    /// A canvas rectangle as it shows on the screen.
    pub fn rect_to_screen(self, canvas: Rect) -> Rect {
        let at = self.to_screen(Point { x: canvas.x, y: canvas.y });
        Rect {
            x: at.x,
            y: at.y,
            width: canvas.width * self.zoom,
            height: canvas.height * self.zoom,
        }
    }

    /// The canvas box a screen of `size` shows.
    pub fn visible(self, size: Size) -> Rect {
        Rect {
            x: self.origin.x,
            y: self.origin.y,
            width: size.width / self.zoom,
            height: size.height / self.zoom,
        }
    }

    /// The canvas dragged by `dx`, `dy` screen points: what was under the
    /// pointer stays under it. A delta that is not finite moves nothing.
    pub fn pan(self, dx: f64, dy: f64) -> Self {
        if !(dx.is_finite() && dy.is_finite()) {
            return self;
        }
        let origin = Point { x: self.origin.x - dx / self.zoom, y: self.origin.y - dy / self.zoom };
        Self { origin, ..self }
    }

    /// The zoom times `factor`, held in `MIN_ZOOM..=MAX_ZOOM`, about a screen
    /// point: the canvas point under `about` stays under it. A factor of zero
    /// or less zooms all the way out. A factor that is not a number, or a
    /// point that is not finite, changes nothing.
    pub fn zoom_about(self, about: Point, factor: f64) -> Self {
        let wanted = self.zoom * factor;
        if wanted.is_nan() || !(about.x.is_finite() && about.y.is_finite()) {
            return self;
        }
        let zoom = wanted.clamp(MIN_ZOOM, MAX_ZOOM);
        let fixed = self.to_canvas(about);
        let origin = Point { x: fixed.x - about.x / zoom, y: fixed.y - about.y / zoom };
        Self { origin, zoom }
    }
}

/// A pop-up at the pen: its panel, and one cell per option, left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub panel: Rect,
    pub cells: Vec<Rect>,
}

/// A colour's cell in a pop-up.
pub const SWATCH: Size = Size { width: 24.0, height: 24.0 };
/// A word's cell in a pop-up.
pub const WORD: Size = Size { width: 64.0, height: 24.0 };

/// The space between a pop-up's panel and its cells, and between cells.
const POPUP_PAD: f64 = 6.0;
/// The closest a pop-up comes to an edge of the space it opens in.
const POPUP_EDGE: f64 = 8.0;

/// A pop-up of `count` cells of `cell` size in a row, centred on `at` (the
/// pen) so the shortest move picks, and moved inside `within` (the focused
/// pane) when it would cross an edge. A pop-up too big for `within` keeps
/// its top left corner in.
pub fn popup(at: Point, count: usize, cell: Size, within: Rect) -> Popup {
    #[expect(clippy::cast_precision_loss, reason = "a handful of cells")]
    let n = count as f64;
    let width = n * cell.width + (n + 1.0) * POPUP_PAD;
    let height = cell.height + 2.0 * POPUP_PAD;
    let place = |at: f64, size: f64, start: f64, room: f64| {
        (at - size / 2.0).min(start + room - size - POPUP_EDGE).max(start + POPUP_EDGE)
    };
    let panel = Rect {
        x: place(at.x, width, within.x, within.width),
        y: place(at.y, height, within.y, within.height),
        width,
        height,
    };
    let cells = (0..count)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "a handful of cells")]
            let i = i as f64;
            Rect {
                x: panel.x + POPUP_PAD + i * (cell.width + POPUP_PAD),
                y: panel.y + POPUP_PAD,
                width: cell.width,
                height: cell.height,
            }
        })
        .collect();
    Popup { panel, cells }
}

impl Rect {
    /// Whether `p` is inside, edges included.
    pub fn contains(self, p: Point) -> bool {
        (self.x..=self.x + self.width).contains(&p.x)
            && (self.y..=self.y + self.height).contains(&p.y)
    }
}

/// The content size the window opens at. The user resizes it from there.
pub const WINDOW: Size = Size { width: 960.0, height: 600.0 };

/// The smallest content size the window resizes to: room for the open
/// sidebar beside a pane.
pub const SMALLEST: Size = Size { width: 480.0, height: 320.0 };

impl Size {
    /// A rectangle of this size at the origin.
    pub fn rect(self) -> Rect {
        Rect { x: 0.0, y: 0.0, width: self.width, height: self.height }
    }
}

const MARGIN: f64 = 24.0;
const LINE_HEIGHT: f64 = 22.0;
const LINE_GAP: f64 = 6.0;

/// One rectangle per text line: full width inside the margins of `area`,
/// stacked, and centered vertically in it. A stack taller than the area
/// starts at its top margin and runs off the bottom.
pub fn lines(area: Rect, count: usize) -> Vec<Rect> {
    let width = (area.width - 2.0 * MARGIN).max(0.0);
    let step = LINE_HEIGHT + LINE_GAP;
    let offsets: Vec<f64> =
        std::iter::successors(Some(0.0), |y| Some(y + step)).take(count).collect();
    let stack = offsets.last().map_or(0.0, |last| last + LINE_HEIGHT);
    let top = area.y + ((area.height - stack) / 2.0).max(MARGIN);
    let x = area.x + MARGIN;
    offsets.iter().map(|y| Rect { x, y: top + y, width, height: LINE_HEIGHT }).collect()
}

/// The one line at the foot of `area`, full width inside its side margins,
/// a gap above its bottom edge; at its top in an area too short for that.
pub fn foot(area: Rect) -> Rect {
    let width = (area.width - 2.0 * MARGIN).max(0.0);
    let y = (area.y + area.height - LINE_HEIGHT - LINE_GAP).max(area.y);
    Rect { x: area.x + MARGIN, y, width, height: LINE_HEIGHT }
}

#[cfg(test)]
mod tests;
