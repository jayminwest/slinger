use std::borrow::Cow;

use super::*;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Size, Viewport};
use crate::scene::canvas::{self, Item, ItemId, Mark};
use crate::scene::ink::Ink;

/// US Letter and A4, in points, as displayed.
const LETTER: (f64, f64) = (612.0, 792.0);
const A4: (f64, f64) = (595.0, 842.0);

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn pdf(frame: Rect, crop: Option<Rect>) -> Shape {
    let (hash, name) = ("sha256:9f".to_string(), Some("paper.pdf".to_string()));
    Shape::Pdf { frame, hash, pdf_page: 3, page_size: LETTER, crop, name }
}

fn item(id: u64, shape: Shape) -> Item {
    Item { id: ItemId(id), colour: Rgb { r: 0, g: 0, b: 0 }, shape }
}

#[test]
fn a_canvas_id_is_the_stem_kept_to_its_letters_then_a_try_then_dot_pdf() {
    let id = |name: &str, n| canvas_id(Path::new(name), n);
    assert_eq!(id("/Users/me/Paper (v2).pdf", 1), "paper--v2-.pdf");
    assert_eq!(id("Paper (v2).pdf", 2), "paper--v2--2.pdf");
    assert_eq!(id("notes_2024-1.PDF", 1), "notes_2024-1.pdf");
    assert_eq!(id("Café.pdf", 1), "caf-.pdf", "each character outside a-z 0-9 - _");
    assert_eq!(id(".pdf", 1), "-pdf.pdf", "a hidden file's stem is its whole name");
    assert_eq!(id("/", 3), "pdf-3.pdf", "a stem left empty is pdf");
    let long = format!("{}.pdf", "a".repeat(80));
    assert_eq!(id(&long, 1), format!("{}.pdf", "a".repeat(56)));
    assert!(id(&long, 999).len() <= 64, "a canvas id holds 64 bytes");
}

#[test]
fn pages_stack_centred_on_a_column_1600_wide_by_one_scale_with_a_gap() {
    let frames = stack(&[LETTER, A4, (1224.0, 792.0)]);
    // The widest page (a tabloid on its side) sets the scale: 1600 / 1224.
    let k = PAGE_WIDTH / 1224.0;
    let near = |a: Rect, b: Rect| {
        [(a.x, b.x), (a.y, b.y), (a.width, b.width), (a.height, b.height)]
            .iter()
            .all(|(p, q)| (p - q).abs() < 1e-9)
    };
    let letter = rect((PAGE_WIDTH - 612.0 * k) / 2.0, 0.0, 612.0 * k, 792.0 * k);
    assert!(near(frames[0], letter), "{:?}", frames[0]);
    let a4 = rect((PAGE_WIDTH - 595.0 * k) / 2.0, 792.0 * k + PAGE_GAP, 595.0 * k, 842.0 * k);
    assert!(near(frames[1], a4), "{:?}", frames[1]);
    let wide = rect(0.0, a4.y + a4.height + PAGE_GAP, PAGE_WIDTH, 792.0 * k);
    assert!(near(frames[2], wide), "{:?}", frames[2]);
    assert_eq!(stack(&[LETTER])[0].width, PAGE_WIDTH, "one page fills the column");
    assert_eq!(stack(&[]), Vec::<Rect>::new());
}

#[test]
fn each_page_is_a_whole_page_item_named_after_its_file_in_page_order() {
    let shapes = pages(&[LETTER, A4], "sha256:9f", Path::new("/tmp/Paper.pdf"));
    let Shape::Pdf { frame, hash, pdf_page, page_size, crop, name } = &shapes[1] else {
        panic!("{shapes:?}")
    };
    assert_eq!((hash.as_str(), *pdf_page, *page_size, *crop), ("sha256:9f", 2, A4, None));
    assert_eq!((name.as_deref(), *frame), (Some("Paper.pdf"), stack(&[LETTER, A4])[1]));
    let long = format!("{}.pdf", "é".repeat(130));
    let Shape::Pdf { name, .. } = &pages(&[LETTER], "sha256:9f", Path::new(&long))[0] else {
        panic!()
    };
    assert_eq!(*name, None, "a name over 255 bytes is not kept");
}

#[test]
fn an_item_shows_its_crop_or_its_whole_page() {
    let crop = rect(72.0, 180.0, 300.0, 120.0);
    assert_eq!(pdf(rect(0.0, 0.0, 1.0, 1.0), None).shown(), Some(rect(0.0, 0.0, 612.0, 792.0)));
    assert_eq!(pdf(rect(0.0, 0.0, 1.0, 1.0), Some(crop)).shown(), Some(crop));
    assert_eq!(Shape::Box { frame: crop, width: 1.0 }.shown(), None);
    let moved = pdf(rect(0.0, 0.0, 1.0, 1.0), Some(crop)).moved(pt(5.0, 6.0));
    assert_eq!(moved, pdf(rect(5.0, 6.0, 1.0, 1.0), Some(crop)));
    assert_eq!(moved.bounds(), Some(rect(5.0, 6.0, 1.0, 1.0)));
}

#[test]
fn an_item_draws_from_its_file_once_it_has_come_and_its_coming_repaints_it() {
    let items = [item(1, pdf(rect(100.0, 100.0, 200.0, 100.0), None))];
    let (view, screen) = (Viewport::default(), Size { width: 800.0, height: 600.0 });
    let theme = Theme::default();
    let first = canvas::build_with(&items, (view, screen), &theme, None, &|_| None);
    let Mark::Pdf { file: None, page: 3, shown, ref hash, .. } = first.draws[0].mark else {
        panic!("{:?}", first.draws[0])
    };
    assert_eq!((shown, hash.as_str()), (rect(0.0, 0.0, 612.0, 792.0), "sha256:9f"));
    let same = canvas::build_with(&items, (view, screen), &theme, Some(&first), &|_| None);
    assert_eq!(same.changed, None);
    let file = |hash: &str| (hash == "sha256:9f").then(|| "/tmp/9f.pdf".into());
    let came = canvas::build_with(&items, (view, screen), &theme, Some(&first), &file);
    let Mark::Pdf { file: Some(ref file), .. } = came.draws[0].mark else { panic!() };
    assert_eq!(file.to_str(), Some("/tmp/9f.pdf"));
    assert_eq!(came.changed, Some(rect(100.0, 100.0, 200.0, 100.0)), "only the page repaints");
}

#[test]
fn a_page_is_the_paper_under_every_other_item() {
    let page = pdf(rect(0.0, 0.0, 100.0, 100.0), None);
    let ink = Shape::Stroke { points: vec![pt(10.0, 10.0), pt(20.0, 20.0)], width: 2.0 };
    let items = [item(1, ink.clone()), item(2, page.clone()), item(3, ink.clone())];
    let shown = Ink::default().shown("main", "p1", &items);
    let order: Vec<u64> = shown.iter().map(|i: &Cow<'_, Item>| i.id.0).collect();
    assert_eq!(order, [2, 1, 3], "pdf items first, the rest in their order");
}

#[test]
fn reader_mode_is_for_a_canvas_whose_id_ends_in_dot_pdf_as_an_open_makes_it() {
    assert!(reader(&canvas_id(std::path::Path::new("/Users/me/Paper (v2).pdf"), 2)));
    assert!(reader("pdf.pdf"));
    assert!(!reader("main"));
    assert!(!reader("pdf"));
    assert!(!reader("notes.pdf.old"));
}

#[test]
fn the_column_is_the_box_around_the_pdf_items_alone() {
    let ink = Shape::Stroke { points: vec![pt(-500.0, 0.0), pt(5000.0, 9000.0)], width: 2.0 };
    let items = [
        item(1, pdf(rect(100.0, 0.0, 1400.0, 1800.0), None)),
        item(2, ink),
        item(3, pdf(rect(0.0, 1840.0, 1600.0, 2000.0), None)),
    ];
    assert_eq!(column(&items), Some(rect(0.0, 0.0, 1600.0, 3840.0)));
    assert_eq!(column(&items[1..2]), None, "no pages: no column");
}
