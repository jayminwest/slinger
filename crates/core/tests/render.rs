//! Images of canvas items, through the public interface: a canvas in, PNGs
//! out. Golden images hold the image rules of `docs/design/packet.md`.
//!
//! After a deliberate change to how things are drawn, look at the new images
//! and store them with `cargo test -p slinger-core --test render -- --ignored`.
#![cfg(test)]

use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, ItemId, Op, PageId, Point, Rect, Shape, Time,
};
use slinger_core::render::{self, Image, MARGIN, MAX_EDGE, OVERLAP, SCALE};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/render");
const RED: Colour = Colour::rgb(220, 30, 30);
const BLUE: Colour = Colour::rgb(30, 60, 200);

struct Board {
    canvas: Canvas,
    page: PageId,
}

impl Board {
    fn new() -> Self {
        let canvas = Canvas::new();
        let page = canvas.pages().next().unwrap().id();
        Self { canvas, page }
    }

    fn add(&mut self, by: Author, shape: Shape) -> ItemId {
        let action = Action::Add { page: self.page, shape };
        let Applied::Added(id) = self.canvas.apply(Op { by, at: Time(0), action }).unwrap() else {
            panic!("not added")
        };
        id
    }

    fn stroke(&mut self, colour: Colour, points: &[(f64, f64)]) -> ItemId {
        let points = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        self.add(Author::User, Shape::Stroke { points, colour, width: 4.0 })
    }
}

/// A small page: a handwritten "hi" with a red underline, and beside it an
/// agent's box with text and an arrow from the ink to it.
fn small() -> (Board, Vec<ItemId>) {
    let mut b = Board::new();
    let ink = vec![
        b.stroke(Colour::BLACK, &[(20.0, 10.0), (20.0, 90.0)]),
        b.stroke(Colour::BLACK, &[(20.0, 60.0), (35.0, 45.0), (50.0, 50.0), (55.0, 90.0)]),
        b.stroke(Colour::BLACK, &[(75.0, 50.0), (75.0, 90.0)]),
        b.stroke(Colour::BLACK, &[(75.0, 30.0)]),
        b.stroke(RED, &[(10.0, 105.0), (60.0, 110.0), (95.0, 104.0)]),
    ];
    let agent = Author::Agent("claude".into());
    let frame = Rect::new(160.0, 20.0, 200.0, 80.0);
    b.add(agent.clone(), Shape::Box { frame, colour: BLUE });
    let text = "Hello, ink: 0.35 scale".into();
    let frame = Rect::new(170.0, 30.0, 180.0, 60.0);
    b.add(agent.clone(), Shape::Text { text, frame, colour: Colour::BLACK });
    let (from, to) = (End::free(Point::new(100.0, 60.0)), End::free(Point::new(155.0, 60.0)));
    b.add(agent, Shape::Arrow { from, to, colour: BLUE });
    (b, ink)
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(format!("{GOLDEN}/{name}")).unwrap_or_default()
}

/// The images the golden tests check, by file name.
fn goldens() -> Vec<Image> {
    let (b, ink) = small();
    let mut images = render::items(&b.canvas, &ink, "c1");
    images.extend(render::crop(&b.canvas, b.page, Rect::new(0.0, 0.0, 380.0, 120.0), "page"));
    images
}

#[test]
fn a_small_page_matches_its_golden_images() {
    for image in goldens() {
        assert!(image.png == golden(&image.name), "{} differs from its golden image", image.name);
    }
}

#[test]
#[ignore = "writes the golden images; run by hand after a deliberate change"]
fn bless_golden_images() {
    std::fs::create_dir_all(GOLDEN).unwrap();
    for image in goldens() {
        std::fs::write(format!("{GOLDEN}/{}", image.name), &image.png).unwrap();
    }
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    let ((a, ink_a), (b, ink_b)) = (small(), small());
    assert_eq!(render::items(&a.canvas, &ink_a, "c1"), render::items(&b.canvas, &ink_b, "c1"));
    let area = Rect::new(-50.0, -50.0, 500.0, 300.0);
    assert_eq!(
        render::crop(&a.canvas, a.page, area, "x"),
        render::crop(&b.canvas, b.page, area, "x")
    );
}

#[test]
fn an_image_is_the_items_box_plus_the_margin_at_the_scale() {
    let (b, ink) = small();
    let [image] = render::items(&b.canvas, &ink, "c1").try_into().unwrap();
    let bounds = b.canvas.bounds_of(ink).unwrap();
    assert_eq!((image.name.as_str(), image.x, image.y), ("c1.png", 0, 0));
    let framed = |units: f64| px(units + 2.0 * MARGIN);
    assert_eq!((image.width, image.height), (framed(bounds.w), framed(bounds.h)));
    let (w, h, _) = decode(&image.png);
    assert_eq!((w, h), (image.width, image.height));
}

#[test]
fn items_draw_only_themselves() {
    let (b, ink) = small();
    let [image] = render::items(&b.canvas, &ink, "c1").try_into().unwrap();
    let (_, _, rgb) = decode(&image.png);
    let blue = rgb.chunks(3).filter(|p| i32::from(p[2]) > i32::from(p[0]) + 50).count();
    assert_eq!(blue, 0, "the agent's blue box and arrow are not in the ink's image");
}

#[test]
fn nothing_to_draw_gives_no_images() {
    let (b, _) = small();
    assert_eq!(render::items(&b.canvas, &[], "c1"), Vec::<Image>::new());
    assert_eq!(render::items(&b.canvas, &[ItemId::new(999)], "c1"), Vec::<Image>::new());
}

#[test]
fn a_crop_of_an_empty_place_is_white() {
    let (b, _) = small();
    let [image] = render::crop(&b.canvas, b.page, Rect::new(1000.0, 1000.0, 100.0, 100.0), "e")
        .try_into()
        .unwrap();
    assert_eq!((image.name.as_str(), image.width, image.height), ("e.png", 35, 35));
    assert!(decode(&image.png).2.iter().all(|&v| v == 255));
}

#[test]
fn a_very_wide_cluster_is_cut_into_tiles_that_overlap_by_the_constant() {
    let mut b = Board::new();
    let points: Vec<(f64, f64)> =
        (0..=100).map(|i| (f64::from(i) * 100.0, if i % 2 == 0 { 0.0 } else { 200.0 })).collect();
    let zigzag = b.stroke(Colour::BLACK, &points);
    let tiles = render::items(&b.canvas, &[zigzag], "c7");
    let names: Vec<&str> = tiles.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["c7-1.png", "c7-2.png", "c7-3.png"]);
    assert!(tiles.iter().all(|t| t.width <= MAX_EDGE && t.height == tiles[0].height && t.y == 0));
    for pair in tiles.windows(2) {
        let (left, right) = (&pair[0], &pair[1]);
        assert_eq!(left.x + left.width - right.x, OVERLAP);
        // The shared pixels are the same in both tiles.
        let (lw, h, l) = decode(&left.png);
        let (rw, _, r) = decode(&right.png);
        for y in 0..h as usize {
            for dx in 0..OVERLAP as usize {
                let lx = (lw - OVERLAP) as usize + dx;
                let li = (y * lw as usize + lx) * 3;
                let ri = (y * rw as usize + dx) * 3;
                assert_eq!(l[li..li + 3], r[ri..ri + 3], "pixel {dx},{y} of the overlap");
            }
        }
    }
    let last = &tiles[2];
    let whole = px(10_000.0 + 4.0 + 2.0 * MARGIN);
    assert_eq!(last.x + last.width, whole, "the tiles end where the picture does");
}

#[test]
fn a_tall_and_wide_picture_is_tiled_by_row_then_column() {
    let mut b = Board::new();
    let line = b.stroke(Colour::BLACK, &[(0.0, 0.0), (5000.0, 5000.0)]);
    let tiles = render::items(&b.canvas, &[line], "c1");
    let at: Vec<(u32, u32)> = tiles.iter().map(|t| (t.x, t.y)).collect();
    let step = MAX_EDGE - OVERLAP;
    assert_eq!(at, [(0, 0), (step, 0), (0, step), (step, step)]);
    assert_eq!(tiles[3].name, "c1-4.png");
}

/// Canvas units as whole pixels, rounded up.
fn px(units: f64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "small and positive"
    )]
    let px = (units * SCALE).ceil() as u32;
    px
}

/// The size and RGB bytes of a PNG this crate wrote (no filters).
fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let (mut at, mut size, mut data) = (8, (0, 0), Vec::new());
    while at < png.len() {
        let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
        let body = &png[at + 8..at + 8 + len];
        match &png[at + 4..at + 8] {
            b"IHDR" => {
                let word = |i: usize| u32::from_be_bytes(body[i..i + 4].try_into().unwrap());
                size = (word(0), word(4));
                assert_eq!(body[8..], [8, 2, 0, 0, 0], "8-bit RGB");
            }
            b"IDAT" => data.extend_from_slice(body),
            _ => {}
        }
        at += 12 + len;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).unwrap();
    let row = size.0 as usize * 3 + 1;
    assert_eq!(raw.len(), row * size.1 as usize);
    let rgb = raw.chunks(row).flat_map(|r| {
        assert_eq!(r[0], 0, "no filter");
        r[1..].to_vec()
    });
    (size.0, size.1, rgb.collect())
}
