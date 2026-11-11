//! Layout math. Pure: sizes in, rectangles out, no `AppKit`, so every branch is
//! unit-tested (this file has a 100% coverage floor). Units are points. The
//! origin is the top left; `platform` converts to `AppKit`'s bottom left.
//! [`Viewport`] maps the endless canvas onto the screen: pan, zoom about a
//! point, and the canvas box that is visible.

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

#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane uses it from slinger-c94c"))]
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
#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane uses it from slinger-c94c"))]
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

#[cfg_attr(not(test), expect(dead_code, reason = "the canvas pane uses it from slinger-c94c"))]
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
/// pen) so the shortest move picks, and moved inside `within` when it would
/// cross an edge. A pop-up too big for `within` keeps its top left corner in.
pub fn popup(at: Point, count: usize, cell: Size, within: Size) -> Popup {
    #[expect(clippy::cast_precision_loss, reason = "a handful of cells")]
    let n = count as f64;
    let width = n * cell.width + (n + 1.0) * POPUP_PAD;
    let height = cell.height + 2.0 * POPUP_PAD;
    let place = |at: f64, size: f64, room: f64| {
        (at - size / 2.0).min(room - size - POPUP_EDGE).max(POPUP_EDGE)
    };
    let panel = Rect {
        x: place(at.x, width, within.width),
        y: place(at.y, height, within.height),
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

/// The content size of the window.
pub const WINDOW: Size = Size { width: 440.0, height: 160.0 };

const MARGIN: f64 = 24.0;
const LINE_HEIGHT: f64 = 22.0;
const LINE_GAP: f64 = 6.0;

/// One rectangle per text line: full width inside the margins, stacked, and
/// centered vertically in `window`. A stack taller than the window starts at
/// the top margin and runs off the bottom.
pub fn lines(window: Size, count: usize) -> Vec<Rect> {
    let width = (window.width - 2.0 * MARGIN).max(0.0);
    let step = LINE_HEIGHT + LINE_GAP;
    let offsets: Vec<f64> =
        std::iter::successors(Some(0.0), |y| Some(y + step)).take(count).collect();
    let stack = offsets.last().map_or(0.0, |last| last + LINE_HEIGHT);
    let top = ((window.height - stack) / 2.0).max(MARGIN);
    offsets.iter().map(|y| Rect { x: MARGIN, y: top + y, width, height: LINE_HEIGHT }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: Size = Size { width: 400.0, height: 200.0 };

    #[test]
    fn centers_one_line() {
        let expected = Rect { x: 24.0, y: 89.0, width: 352.0, height: 22.0 };
        assert_eq!(lines(WIDE, 1), [expected]);
    }

    #[test]
    fn stacks_lines_with_a_gap_and_centers_the_stack() {
        let at = |y| Rect { x: 24.0, y, width: 352.0, height: 22.0 };
        assert_eq!(lines(WIDE, 2), [at(75.0), at(103.0)]);
    }

    #[test]
    fn no_lines_is_no_rectangles() {
        assert_eq!(lines(WIDE, 0), []);
    }

    #[test]
    fn a_window_too_small_keeps_the_top_margin_and_a_zero_width() {
        let rects = lines(Size { width: 30.0, height: 40.0 }, 2);
        assert_eq!(rects[0], Rect { x: 24.0, y: 24.0, width: 0.0, height: 22.0 });
        assert_eq!(rects[1].y, 52.0);
    }

    const SCREEN: Size = Size { width: 800.0, height: 600.0 };

    fn pt(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect { x, y, width, height }
    }

    fn near(a: Point, b: Point) -> bool {
        let scale = 1.0_f64.max(a.x.abs()).max(a.y.abs());
        (a.x - b.x).abs() <= 1e-9 * scale && (a.y - b.y).abs() <= 1e-9 * scale
    }

    /// A viewport that is neither at the origin nor at 100%.
    fn moved() -> Viewport {
        Viewport::default().pan(-130.0, 45.0).zoom_about(pt(300.0, 200.0), 2.5)
    }

    #[test]
    fn around_no_points_is_none_and_around_points_is_their_box() {
        assert_eq!(Rect::around([]), None);
        assert_eq!(Rect::around([pt(3.0, 4.0)]), Some(rect(3.0, 4.0, 0.0, 0.0)));
        assert_eq!(
            Rect::around([pt(3.0, 4.0), pt(-1.0, 9.0), pt(5.0, 0.0)]),
            Some(rect(-1.0, 0.0, 6.0, 9.0))
        );
    }

    #[test]
    fn union_holds_both_rectangles() {
        let union = rect(0.0, 0.0, 10.0, 10.0).union(rect(20.0, -5.0, 5.0, 5.0));
        assert_eq!(union, rect(0.0, -5.0, 25.0, 15.0));
    }

    #[test]
    fn intersection_is_the_shared_part_or_none() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection(rect(5.0, 5.0, 10.0, 10.0)), Some(rect(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.intersection(rect(10.0, 0.0, 5.0, 5.0)), Some(rect(10.0, 0.0, 0.0, 5.0)));
        assert_eq!(a.intersection(rect(11.0, 0.0, 5.0, 5.0)), None);
        assert_eq!(a.intersection(rect(0.0, 11.0, 5.0, 5.0)), None);
    }

    #[test]
    fn outset_grows_every_side() {
        assert_eq!(rect(0.0, 0.0, 10.0, 4.0).outset(2.0), rect(-2.0, -2.0, 14.0, 8.0));
    }

    #[test]
    fn the_default_viewport_is_the_identity() {
        let view = Viewport::default();
        assert_eq!(view.zoom(), 1.0);
        assert_eq!(view.to_screen(pt(12.0, -7.0)), pt(12.0, -7.0));
        assert_eq!(view.visible(SCREEN), rect(0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn screen_to_canvas_and_back_is_the_identity() {
        for view in [Viewport::default(), moved(), moved().zoom_about(pt(0.0, 0.0), 0.01)] {
            for p in [pt(0.0, 0.0), pt(799.0, 1.0), pt(-50.5, 1234.25)] {
                assert!(near(view.to_screen(view.to_canvas(p)), p), "{view:?} {p:?}");
                assert!(near(view.to_canvas(view.to_screen(p)), p), "{view:?} {p:?}");
            }
        }
    }

    #[test]
    fn a_canvas_rectangle_on_screen_has_its_corners_where_the_points_go() {
        let view = moved();
        let canvas = rect(10.0, 20.0, 30.0, 40.0);
        let screen = view.rect_to_screen(canvas);
        assert!(near(pt(screen.x, screen.y), view.to_screen(pt(10.0, 20.0))));
        let far = view.to_screen(pt(40.0, 60.0));
        assert!(near(pt(screen.x + screen.width, screen.y + screen.height), far));
    }

    #[test]
    fn pan_moves_the_canvas_with_the_pointer() {
        let view = moved();
        let under = view.to_canvas(pt(100.0, 100.0));
        let panned = view.pan(30.0, -20.0);
        assert!(near(panned.to_screen(under), pt(130.0, 80.0)));
        assert_eq!(panned.zoom(), view.zoom());
    }

    #[test]
    fn a_pan_that_is_not_finite_moves_nothing() {
        let view = moved();
        assert_eq!(view.pan(f64::NAN, 0.0), view);
        assert_eq!(view.pan(0.0, f64::INFINITY), view);
    }

    #[test]
    fn the_canvas_has_no_edge() {
        let far = Viewport::default().pan(-1e12, 1e12);
        assert_eq!(far.visible(SCREEN), rect(1e12, -1e12, 800.0, 600.0));
        let p = pt(1e12 + 5.0, -1e12 + 7.0);
        assert!(near(far.to_canvas(far.to_screen(p)), p));
    }

    #[test]
    fn zoom_about_a_point_keeps_that_point_still() {
        let view = moved();
        for (about, factor) in [(pt(250.0, 125.0), 1.7), (pt(0.0, 600.0), 0.4), (pt(9.0, 9.0), 1e9)]
        {
            let under = view.to_canvas(about);
            let zoomed = view.zoom_about(about, factor);
            assert!(near(zoomed.to_screen(under), about), "{about:?} x{factor}");
        }
    }

    #[test]
    fn zoom_changes_what_is_visible() {
        let view = Viewport::default().zoom_about(pt(0.0, 0.0), 2.0);
        assert_eq!(view.zoom(), 2.0);
        assert_eq!(view.visible(SCREEN), rect(0.0, 0.0, 400.0, 300.0));
    }

    #[test]
    fn zoom_has_limits() {
        let view = moved();
        let about = pt(10.0, 10.0);
        assert_eq!(view.zoom_about(about, 1e9).zoom(), MAX_ZOOM);
        assert_eq!(view.zoom_about(about, 1e-9).zoom(), MIN_ZOOM);
        assert_eq!(view.zoom_about(about, 0.0).zoom(), MIN_ZOOM);
        assert_eq!(view.zoom_about(about, -3.0).zoom(), MIN_ZOOM);
        assert_eq!(view.zoom_about(about, f64::INFINITY).zoom(), MAX_ZOOM);
    }

    #[test]
    fn a_zoom_that_is_not_a_number_or_about_no_point_changes_nothing() {
        let view = moved();
        assert_eq!(view.zoom_about(pt(10.0, 10.0), f64::NAN), view);
        assert_eq!(view.zoom_about(pt(f64::NAN, 10.0), 2.0), view);
        assert_eq!(view.zoom_about(pt(10.0, f64::NEG_INFINITY), 2.0), view);
    }

    #[test]
    fn a_popup_is_centred_on_the_pen_with_its_cells_in_a_row() {
        let pop = popup(pt(200.0, 100.0), 3, SWATCH, SCREEN);
        // 3 cells of 24 and 4 pads of 6 wide; 24 and 2 pads high.
        assert_eq!(pop.panel, rect(200.0 - 48.0, 100.0 - 18.0, 96.0, 36.0));
        let xs: Vec<f64> = pop.cells.iter().map(|c| c.x).collect();
        assert_eq!(xs, [158.0, 188.0, 218.0]);
        for cell in &pop.cells {
            assert_eq!((cell.y, cell.width, cell.height), (88.0, 24.0, 24.0));
            assert!(pop.panel.contains(pt(cell.x, cell.y)));
            assert!(pop.panel.contains(pt(cell.x + cell.width, cell.y + cell.height)));
        }
    }

    #[test]
    fn a_popup_near_an_edge_moves_inside_it() {
        let size = |p: &Popup| (p.panel.width, p.panel.height);
        let top_left = popup(pt(3.0, 2.0), 4, WORD, SCREEN);
        assert_eq!((top_left.panel.x, top_left.panel.y), (8.0, 8.0));
        let bottom_right = popup(pt(799.0, 599.0), 4, WORD, SCREEN);
        let (w, h) = size(&bottom_right);
        assert_eq!(
            (bottom_right.panel.x, bottom_right.panel.y),
            (800.0 - 8.0 - w, 600.0 - 8.0 - h)
        );
        let off_screen = popup(pt(-500.0, 9000.0), 4, WORD, SCREEN);
        assert_eq!((off_screen.panel.x, off_screen.panel.y), (8.0, 600.0 - 8.0 - h));
        assert_eq!(off_screen.cells[0].x, 8.0 + 6.0);
    }

    #[test]
    fn a_popup_too_big_for_its_space_keeps_its_top_left_in() {
        let pop = popup(pt(20.0, 20.0), 9, WORD, Size { width: 100.0, height: 30.0 });
        assert_eq!((pop.panel.x, pop.panel.y), (8.0, 8.0));
        assert!(pop.panel.width > 100.0);
    }

    #[test]
    fn a_popup_of_no_cells_is_an_empty_panel() {
        let pop = popup(pt(100.0, 100.0), 0, SWATCH, SCREEN);
        assert_eq!(pop.cells, []);
        assert_eq!(pop.panel, rect(97.0, 82.0, 6.0, 36.0));
    }

    #[test]
    fn contains_includes_the_edges() {
        let r = rect(10.0, 10.0, 5.0, 5.0);
        assert!(r.contains(pt(10.0, 10.0)) && r.contains(pt(15.0, 15.0)));
        assert!(!r.contains(pt(9.9, 12.0)) && !r.contains(pt(12.0, 15.1)));
    }

    #[test]
    fn the_window_fits_two_lines_inside_its_margins() {
        let rects = lines(WINDOW, 2);
        assert!(rects[0].y >= MARGIN);
        assert!(rects[1].y + rects[1].height <= WINDOW.height - MARGIN);
    }
}
