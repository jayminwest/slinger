use super::*;
use crate::config::Rgb;

/// US Letter, in points, as displayed.
const LETTER: (f64, f64) = (612.0, 792.0);

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn pdf(frame: Rect, crop: Option<Rect>) -> Shape {
    let (hash, name) = ("sha256:9f86".to_string(), Some("paper.pdf".to_string()));
    Shape::Pdf { frame, hash, pdf_page: 3, page_size: LETTER, crop, name }
}

fn item(id: u64, shape: Shape) -> Cow<'static, Item> {
    Cow::Owned(Item { id: ItemId(id), colour: Rgb { r: 0, g: 0, b: 0 }, shape })
}

fn press(at: Point) -> Press {
    Press { at, when: 0.0, zoom: 1.0 }
}

/// The page whole, at half its size in points: 1224 by 1584 units.
fn page() -> Shape {
    pdf(rect(0.0, 0.0, 1224.0, 1584.0), None)
}

#[test]
fn a_box_on_a_whole_page_crops_to_the_box_in_page_points() {
    let snippet = cropped(&page(), rect(100.0, 200.0, 400.0, 300.0)).unwrap();
    // Two units a point: the box's corner and size, halved.
    assert_eq!(
        snippet,
        pdf(rect(100.0, 200.0, 400.0, 300.0), Some(rect(50.0, 100.0, 200.0, 150.0)))
    );
}

#[test]
fn a_page_not_at_the_origin_maps_from_its_frame() {
    // Letter in the design's 1600-unit column: 0.3825 points a unit.
    let source = pdf(rect(0.0, 1000.0, 1600.0, 2070.6), None);
    let Some(Shape::Pdf { frame, crop: Some(crop), .. }) =
        cropped(&source, rect(400.0, 1400.0, 800.0, 400.0))
    else {
        panic!()
    };
    assert_eq!(frame, rect(400.0, 1400.0, 800.0, 400.0));
    assert_eq!(crop.x, 153.0);
    assert_eq!(crop.width, 306.0);
    // 400 units down a page 2070.6 high is 153.0 of 792 points, to the tenth.
    assert_eq!(crop.y, 153.0);
    assert_eq!(crop.height, 153.0);
}

#[test]
fn a_crop_of_a_crop_maps_from_the_shown_region() {
    // The design's example snippet: 300 by 120 points in 600 by 240 units.
    let source = pdf(rect(2400.0, 300.0, 600.0, 240.0), Some(rect(72.0, 180.0, 300.0, 120.0)));
    let snippet = cropped(&source, rect(2500.0, 350.0, 200.0, 100.0)).unwrap();
    assert_eq!(
        snippet,
        pdf(rect(2500.0, 350.0, 200.0, 100.0), Some(rect(122.0, 205.0, 100.0, 50.0)))
    );
}

#[test]
fn a_box_past_the_frame_s_edge_keeps_only_what_lies_on_the_page() {
    let snippet = cropped(&page(), rect(-100.0, -100.0, 300.0, 300.0)).unwrap();
    assert_eq!(snippet, pdf(rect(0.0, 0.0, 200.0, 200.0), Some(rect(0.0, 0.0, 100.0, 100.0))));
    let corner = cropped(&page(), rect(1100.0, 1500.0, 500.0, 500.0)).unwrap();
    assert_eq!(
        corner,
        pdf(rect(1100.0, 1500.0, 124.0, 84.0), Some(rect(550.0, 750.0, 62.0, 42.0)))
    );
    let Some(Shape::Pdf { crop: Some(c), .. }) = cropped(&page(), rect(-5.0, -5.0, 2000.0, 2000.0))
    else {
        panic!()
    };
    assert_eq!(c, rect(0.0, 0.0, 612.0, 792.0), "never past the page");
}

#[test]
fn rounding_never_takes_a_crop_past_the_page() {
    // A third of a point a unit: edges fall between tenths.
    let source = pdf(rect(0.0, 0.0, 1836.0, 2376.0), None);
    for x in [1.0, 2.0, 1500.05, 1835.0] {
        let Some(Shape::Pdf { crop: Some(c), frame, .. }) =
            cropped(&source, rect(x, x, 1836.0 - x, 2376.0 - x))
        else {
            panic!("{x}")
        };
        let tenths = |v: f64| (v * 10.0).round();
        assert!(tenths(c.x) + tenths(c.width) <= 6120.0, "{c:?}");
        assert!(tenths(c.y) + tenths(c.height) <= 7920.0, "{c:?}");
        assert!(frame.width > 0.0 && frame.height > 0.0);
    }
}

#[test]
fn a_box_that_leaves_nothing_of_the_item_or_another_kind_crops_nothing() {
    assert_eq!(cropped(&page(), rect(2000.0, 0.0, 100.0, 100.0)), None);
    assert_eq!(cropped(&page(), rect(1224.0, 0.0, 100.0, 100.0)), None, "the edge alone");
    let boxed = Shape::Box { frame: rect(0.0, 0.0, 100.0, 100.0), width: 2.0 };
    assert_eq!(cropped(&boxed, rect(0.0, 0.0, 50.0, 50.0)), None);
}

#[test]
fn a_drag_crops_the_topmost_page_under_the_press_and_leaves_the_canvas() {
    let lower = pdf(rect(0.0, 0.0, 1224.0, 1584.0), None);
    let upper = pdf(rect(600.0, 0.0, 1224.0, 1584.0), Some(rect(0.0, 0.0, 612.0, 792.0)));
    let ink = Shape::Box { frame: rect(0.0, 0.0, 2000.0, 2000.0), width: 2.0 };
    let items = vec![item(1, lower), item(2, upper.clone()), item(3, ink)];
    let mut crop = Crop::default();
    assert!(!crop.down(Tool::Select, ("main", "p1"), press(pt(700.0, 100.0)), &items));
    assert!(crop.down(Tool::CropPdf, ("main", "p1"), press(pt(700.0, 100.0)), &items));
    assert!(crop.drag(pt(800.0, 150.0)));
    let theme = Theme::default();
    let shown = crop.shown(("main", "p1"), items.clone(), 2.0, &theme);
    let Shape::Panel { frame, width, dashed: true, .. } = shown[3].shape else { panic!() };
    assert_eq!((frame, width), (rect(700.0, 100.0, 100.0, 50.0), 0.5), "the box, dashed");
    assert_eq!(crop.shown(("main", "p2"), items.clone(), 1.0, &theme).len(), 3);
    assert!(crop.up(Some(pt(900.0, 200.0)), &items));
    let snippet = crop.take().unwrap();
    assert_eq!(snippet, cropped(&upper, rect(700.0, 100.0, 200.0, 100.0)).unwrap());
    assert_eq!(crop.take(), None, "taken once");
    assert_eq!(crop.shown(("main", "p1"), items.clone(), 1.0, &theme).len(), 3, "no box left");
}

#[test]
fn a_box_from_off_the_pages_takes_the_topmost_page_it_meets() {
    let items = vec![item(1, page()), item(2, pdf(rect(0.0, 2000.0, 1224.0, 1584.0), None))];
    let mut crop = Crop::default();
    crop.down(Tool::CropPdf, ("main", "p1"), press(pt(-200.0, 1500.0)), &items);
    assert!(crop.up(Some(pt(100.0, 2100.0)), &items));
    let Some(Shape::Pdf { frame, .. }) = crop.take() else { panic!() };
    assert_eq!(frame, rect(0.0, 2000.0, 100.0, 100.0), "the later page, drawn over");
}

#[test]
fn a_tap_or_a_box_on_no_page_makes_nothing() {
    let items = vec![item(1, page())];
    let mut crop = Crop::default();
    crop.down(Tool::CropPdf, ("main", "p1"), press(pt(100.0, 100.0)), &items);
    assert!(crop.up(Some(pt(102.0, 101.0)), &items), "a tap is the tool's");
    assert_eq!(crop.take(), None);
    crop.down(Tool::CropPdf, ("main", "p1"), press(pt(3000.0, 0.0)), &items);
    assert!(crop.up(Some(pt(3200.0, 200.0)), &items));
    assert_eq!(crop.take(), None);
    assert!(!crop.up(None, &items), "no box under the pen");
    assert!(!crop.drag(pt(0.0, 0.0)));
}
