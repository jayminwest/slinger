//! Draws items into one tile of RGB pixels. Shapes are covered by distance
//! (lines) or by area (glyph cells), so edges are smooth, and each item is
//! laid down once over what is under it, in its own colour.

use super::{Backdrop, SCALE, png};
use crate::canvas::{Colour, Item, Point, Rect, Shape};

mod glyph;
#[cfg(test)]
mod tests;

use glyph::{glyph, zero_width};

/// The pen width, in canvas units, of a box's edge and an arrow.
const LINE_WIDTH: f64 = 2.0;

/// The thinnest line, as a radius in pixels. Thinner ink would fade to grey.
const MIN_RADIUS: f64 = 0.5;

/// The length, in canvas units, of each side of an arrow's head.
const HEAD: f64 = 16.0;

/// What a crop writes inside a "send to agent" button.
const BUTTON_LABEL: &str = "send to agent";

/// The colour of a `pdf` item's frame and label when its page is not drawn.
const PDF_GREY: Colour = Colour::rgb(0x99, 0x99, 0x99);

/// The size, in canvas units, of one character cell of a label.
const TEXT_SIZE: f64 = crate::canvas::TEXT_SIZE;

/// One tile of the picture of `frame`, white until drawn on.
pub(super) struct Raster {
    /// The canvas area of the whole picture.
    frame: Rect,
    /// The tile's top-left pixel in the whole picture.
    left: f64,
    top: f64,
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

/// How much of each pixel in a part of a tile a shape covers, from 0 to 1.
struct Mask {
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    cover: Vec<f64>,
}

impl Raster {
    pub fn new(frame: Rect, x: u32, y: u32, width: u32, height: u32) -> Self {
        let (width, height) = (width as usize, height as usize);
        let (left, top) = (f64::from(x), f64::from(y));
        Self { frame, left, top, width, height, rgb: vec![255; width * height * 3] }
    }

    /// A canvas point in this tile's pixels.
    fn at(&self, p: Point) -> Point {
        Point::new(
            (p.x - self.frame.x) * SCALE - self.left,
            (p.y - self.frame.y) * SCALE - self.top,
        )
    }

    /// Draws one item over what is already drawn.
    pub fn item(&mut self, item: &Item) {
        let (lines, radius, colour) = match item.shape() {
            Shape::Stroke { points, colour, width } => {
                let points: Vec<Point> = points.iter().map(|&p| self.at(p)).collect();
                (vec![points], width * SCALE / 2.0, *colour)
            }
            Shape::Box { frame, colour } => {
                (vec![corners(self, *frame)], LINE_WIDTH * SCALE / 2.0, *colour)
            }
            Shape::Arrow { from, to, colour } => {
                let lines = arrow(from.at, to.at).into_iter();
                let lines = lines.map(|l| l.into_iter().map(|p| self.at(p)).collect()).collect();
                (lines, LINE_WIDTH * SCALE / 2.0, *colour)
            }
            Shape::Text { text, frame, colour, size } => {
                return self.text(text, *frame, *colour, *size);
            }
            // A live item's page is not drawn: its box, and its UI's name
            // inside. A button the same, with its label; a PDF's page in
            // grey, with its number (the core cannot read the PDF).
            Shape::Live { frame, .. } | Shape::Button { frame } | Shape::Pdf { frame, .. } => {
                let (label, colour) = match item.shape() {
                    Shape::Live { ui, .. } => (ui.clone(), Colour::BLACK),
                    Shape::Pdf { pdf_page, .. } => (format!("pdf p{pdf_page}"), PDF_GREY),
                    _ => (BUTTON_LABEL.to_owned(), Colour::BLACK),
                };
                let pad = TEXT_SIZE / 2.0;
                let inside = Rect::new(frame.x + pad, frame.y + pad, frame.w - 2.0 * pad, frame.h);
                self.text(&label, inside, colour, TEXT_SIZE);
                (vec![corners(self, *frame)], LINE_WIDTH * SCALE / 2.0, colour)
            }
        };
        let radius = radius.max(MIN_RADIUS);
        let points = lines.iter().flatten().copied();
        let Some(bounds) = Rect::around(points) else { return };
        let Some(mut mask) = self.mask(bounds, radius + 1.0) else { return };
        for line in &lines {
            mask.line(line, radius);
        }
        self.fill(&mask, colour);
    }

    /// Paints `backdrop` over the pixels whose centre lies in `clip`, a
    /// canvas rectangle inside its area: each takes the backdrop's pixel
    /// nearest that centre.
    pub fn backdrop(&mut self, backdrop: &Backdrop, clip: Rect) {
        let unit =
            |pixel: usize, start: f64, origin: f64| start + (origin + float(pixel) + 0.5) / SCALE;
        for row in 0..self.height {
            let y = unit(row, self.frame.y, self.top);
            if y < clip.y || y >= clip.y + clip.h {
                continue;
            }
            for col in 0..self.width {
                let x = unit(col, self.frame.x, self.left);
                if x >= clip.x && x < clip.x + clip.w {
                    let at = (row * self.width + col) * 3;
                    self.rgb[at..at + 3].copy_from_slice(&backdrop.pixel(Point::new(x, y)));
                }
            }
        }
    }

    /// Lays `text` out from the top left of `frame`, one character cell of
    /// `size` per character, wrapped at spaces to the frame's width when at
    /// least one cell fits, and at each `\n`.
    fn text(&mut self, text: &str, frame: Rect, colour: Colour, size: f64) {
        let rows = wrap(text, columns(frame.w, size));
        let longest = rows.iter().map(Vec::len).max().unwrap_or(0);
        let cell = size * SCALE;
        let origin = self.at(Point::new(frame.x, frame.y));
        let size = Point::new(float(longest) * cell, float(rows.len()) * cell);
        let bounds = Rect::new(origin.x, origin.y, size.x, size.y);
        let Some(mut mask) = self.mask(bounds, 1.0) else { return };
        let bit = cell / 8.0;
        for (r, row) in rows.iter().enumerate() {
            for (c, &ch) in row.iter().enumerate() {
                let (gx, gy) = (origin.x + float(c) * cell, origin.y + float(r) * cell);
                for (y, bits) in (0u8..).zip(glyph(ch)) {
                    for x in (0u8..8).filter(|x| bits & (1 << x) != 0) {
                        let (x, y) = (gx + f64::from(x) * bit, gy + f64::from(y) * bit);
                        mask.cell(x, y, x + bit, y + bit);
                    }
                }
            }
        }
        self.fill(&mask, colour);
    }

    /// An empty mask over the pixels of `bounds` grown by `pad`, cut to the
    /// tile, or `None` when that is outside the tile.
    fn mask(&self, bounds: Rect, pad: f64) -> Option<Mask> {
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped")]
        let clamp = |v: f64, max: usize| v.clamp(0.0, float(max)) as usize;
        let x0 = clamp((bounds.x - pad).floor(), self.width);
        let y0 = clamp((bounds.y - pad).floor(), self.height);
        let x1 = clamp((bounds.x + bounds.w + pad).ceil(), self.width);
        let y1 = clamp((bounds.y + bounds.h + pad).ceil(), self.height);
        let (width, height) = (x1.checked_sub(x0)?, y1.checked_sub(y0)?);
        (width > 0 && height > 0).then(|| Mask {
            x0,
            y0,
            width,
            height,
            cover: vec![0.0; width * height],
        })
    }

    /// Lays `colour` over the tile as much as `mask` covers each pixel.
    fn fill(&mut self, mask: &Mask, colour: Colour) {
        let alpha = f64::from(colour.a) / 255.0;
        for (n, &cover) in mask.cover.iter().enumerate().filter(|(_, c)| **c > 0.0) {
            let (col, row) = (mask.x0 + n % mask.width, mask.y0 + n / mask.width);
            let at = (row * self.width + col) * 3;
            let weight = cover * alpha;
            for (dst, src) in self.rgb[at..at + 3].iter_mut().zip([colour.r, colour.g, colour.b]) {
                let (old, ink) = (f64::from(*dst), f64::from(src));
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "0..=255"
                )]
                let blended = (old + (ink - old) * weight).round() as u8;
                *dst = blended;
            }
        }
    }

    #[cfg(test)]
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let at = (y * self.width + x) * 3;
        [self.rgb[at], self.rgb[at + 1], self.rgb[at + 2]]
    }

    /// The box `[x, y, w, h]` of the pixels drawn on, in the tile: those
    /// not white. `None` when the tile is all white.
    pub fn inked(&self) -> Option<[u32; 4]> {
        let (mut lo, mut hi) = ((usize::MAX, usize::MAX), (0, 0));
        for (n, px) in self.rgb.as_chunks::<3>().0.iter().enumerate() {
            if *px != [255, 255, 255] {
                let (x, y) = (n % self.width, n / self.width);
                lo = (lo.0.min(x), lo.1.min(y));
                hi = (hi.0.max(x), hi.1.max(y));
            }
        }
        #[expect(clippy::cast_possible_truncation, reason = "within a tile built from u32")]
        let n = |v: usize| v as u32;
        (lo.0 <= hi.0).then(|| [n(lo.0), n(lo.1), n(hi.0 - lo.0 + 1), n(hi.1 - lo.1 + 1)])
    }

    /// Whether every pixel drawn on lies in one of `rects`, boxes `[x, y,
    /// w, h]` of the whole picture.
    pub fn inked_within(&self, rects: &[[u32; 4]]) -> bool {
        // The tile's place in the whole picture, whole pixels by `new`.
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "from u32")]
        let (left, top) = (self.left as usize, self.top as usize);
        let held = |x: usize, y: usize| {
            rects.iter().any(|&[rx, ry, rw, rh]| {
                let (rx, ry, rw, rh) = (rx as usize, ry as usize, rw as usize, rh as usize);
                (rx..rx + rw).contains(&x) && (ry..ry + rh).contains(&y)
            })
        };
        self.rgb.as_chunks::<3>().0.iter().enumerate().all(|(n, px)| {
            *px == [255, 255, 255] || held(left + n % self.width, top + n / self.width)
        })
    }

    /// The PNG of the part `[x, y, w, h]` of the tile, which lies inside it.
    pub fn png(&self, [x, y, w, h]: [u32; 4]) -> Vec<u8> {
        let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for row in y..y + h {
            let at = (row * self.width + x) * 3;
            rgb.extend_from_slice(&self.rgb[at..at + w * 3]);
        }
        #[expect(clippy::cast_possible_truncation, reason = "built from u32")]
        png::encode(w as u32, h as u32, &rgb)
    }
}

impl Mask {
    /// Covers a polyline thickened by `radius` pixels on each side. One point
    /// is a dot.
    fn line(&mut self, points: &[Point], radius: f64) {
        let dot = points.first().filter(|_| points.len() == 1).map(|&p| (p, p));
        for (a, b) in dot.into_iter().chain(points.windows(2).map(|w| (w[0], w[1]))) {
            let span = Rect::around([a, b]).unwrap_or_default();
            self.each(
                span,
                radius + 1.0,
                |p| (radius + 0.5 - distance(p, a, b)).clamp(0.0, 1.0),
                f64::max,
            );
        }
    }

    /// Covers the rectangle from `(x0, y0)` to `(x1, y1)` by area. Cells that
    /// meet add up, so a run of them has no seam.
    fn cell(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let span = Rect::new(x0, y0, x1 - x0, y1 - y0);
        let overlap = |lo: f64, hi: f64, p: f64| (hi.min(p + 0.5) - lo.max(p - 0.5)).max(0.0);
        let cover = |p: Point| overlap(x0, x1, p.x) * overlap(y0, y1, p.y);
        self.each(span, 1.0, cover, |a, b| (a + b).min(1.0));
    }

    /// Sets each pixel near `span` to `join(old, cover(its centre))`.
    fn each(
        &mut self,
        span: Rect,
        pad: f64,
        cover: impl Fn(Point) -> f64,
        join: impl Fn(f64, f64) -> f64,
    ) {
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped")]
        let index =
            |v: f64, origin: usize, len: usize| (v - float(origin)).clamp(0.0, float(len)) as usize;
        let (x0, x1) = (
            index((span.x - pad).floor(), self.x0, self.width),
            index((span.x + span.w + pad).ceil(), self.x0, self.width),
        );
        let (y0, y1) = (
            index((span.y - pad).floor(), self.y0, self.height),
            index((span.y + span.h + pad).ceil(), self.y0, self.height),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let centre = Point::new(float(self.x0 + x) + 0.5, float(self.y0 + y) + 0.5);
                let c = cover(centre);
                if c > 0.0 {
                    let old = &mut self.cover[y * self.width + x];
                    *old = join(*old, c);
                }
            }
        }
    }
}

/// An arrow as lines: the shaft, then the head at `to`.
/// The outline of `frame`, closed, in `tile`'s pixels.
fn corners(tile: &Raster, frame: Rect) -> Vec<Point> {
    let [a, b, c, d] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        .map(|(u, v)| tile.at(Point::new(frame.x + u * frame.w, frame.y + v * frame.h)));
    vec![a, b, c, d, a]
}

fn arrow(from: Point, to: Point) -> Vec<Vec<Point>> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return vec![vec![to]];
    }
    let (ux, uy) = (dx / len, dy / len);
    // Each side of the head is the shaft's direction turned back by 30 degrees.
    let (cos, sin) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let side = |s: f64| {
        let (rx, ry) = (ux * cos - uy * sin * s, uy * cos + ux * sin * s);
        Point::new(to.x - rx * HEAD, to.y - ry * HEAD)
    };
    vec![vec![from, to], vec![side(1.0), to, side(-1.0)]]
}

/// How many characters of `size` a line of text holds in a frame `w` wide:
/// as many cells as fit, or no limit when not one does (the text is drawn
/// unwrapped).
#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "floored, >= 1")]
pub(super) fn columns(w: f64, size: f64) -> usize {
    if w >= size { (w / size) as usize } else { usize::MAX }
}

/// Lines of characters: `text` cut at each `\n`, and wrapped at spaces to at
/// most `columns` characters. A word longer than a line is cut. Zero-width
/// characters draw nothing, so they are left out and take no cell.
pub(super) fn wrap(text: &str, columns: usize) -> Vec<Vec<char>> {
    let mut rows = Vec::new();
    for paragraph in text.split('\n') {
        let mut row: Vec<char> = Vec::new();
        for (i, word) in paragraph.split(' ').enumerate() {
            let word: Vec<char> = word.chars().filter(|&c| !zero_width(c)).collect();
            if i > 0 {
                // The space before a word that does not fit becomes the line break.
                if !row.is_empty() && row.len() + 1 + word.len() > columns {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.push(' ');
                }
            }
            row.extend(word);
            while row.len() > columns {
                rows.push(row.drain(..columns).collect());
            }
        }
        rows.push(row);
    }
    rows
}

#[expect(clippy::cast_precision_loss, reason = "pixel and character counts are far below 2^52")]
fn float(n: usize) -> f64 {
    n as f64
}

/// The distance from `p` to the segment `a`-`b`.
fn distance(p: Point, a: Point, b: Point) -> f64 {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let len2 = abx * abx + aby * aby;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - (a.x + t * abx)).hypot(p.y - (a.y + t * aby))
}
