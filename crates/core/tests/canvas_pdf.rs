//! `pdf` items (docs/design/pdf.md): one page of a PDF, or part of one, on a
//! canvas. The model's rules, the canvas file's bytes, the geometry (paper
//! under everything, never erased), the packet's line and `over`, and a crop.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Edit, Log, backdrop, read, rect_tenths};
use slinger_core::canvas::{Action, Author, Canvas, ItemId, Op, Point, Rect, Shape, Time};
use slinger_core::packet::Packet;

const AT: Time = Time(1_762_790_400_000);

fn hash() -> String {
    format!("sha256:{}", "9f".repeat(32))
}

fn pdf(frame: Rect, crop: Option<Rect>) -> Shape {
    let name = Some("paper.pdf".to_string());
    Shape::Pdf { frame, hash: hash(), pdf_page: 1, page_size: (612.0, 792.0), crop, name }
}

/// The add of the user's page 1 of `paper.pdf`, 1600 by 2070.6 units at 0,0.
fn page_add() -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 0, "y": 0, "w": 16000,
        "h": 20706, "hash": hash(), "pdfpage": 1, "pagesize": [6120, 7920], "name": "paper.pdf" } })
}

fn stroke(x: i64, y: i64) -> Value {
    json!({ "op": "add", "item": { "page": "p1", "kind": "stroke", "color": "#ff0000",
        "width": 30, "pts": [x, y, 400, 0] } })
}

/// A log holding the PDF page `i1`, and its file so far.
fn log() -> (Log, String) {
    let (mut log, mut file) = Log::create("paper.pdf").unwrap();
    file += &log.apply(&Author::User, AT, &page_add()).unwrap().bytes;
    (log, file)
}

#[test]
fn a_pdf_item_needs_an_area_a_blob_id_a_page_and_a_crop_on_the_page() {
    let mut c = Canvas::new();
    let page = c.pages().next().unwrap().id();
    let mut add =
        |shape| c.apply(Op { by: Author::User, at: AT, action: Action::Add { page, shape } });
    let frame = Rect::new(0.0, 0.0, 160.0, 207.0);
    assert!(add(pdf(frame, None)).is_ok());
    assert!(add(pdf(frame, Some(Rect::new(72.0, 180.0, 300.0, 120.0)))).is_ok());
    let whole = |hash: &str, pdf_page, page_size, name: &str| Shape::Pdf {
        frame,
        hash: hash.into(),
        pdf_page,
        page_size,
        crop: None,
        name: Some(name.into()),
    };
    let size = (612.0, 792.0);
    let bad = [
        pdf(Rect::new(0.0, 0.0, 0.0, 10.0), None),
        pdf(frame, Some(Rect::new(600.0, 0.0, 20.0, 10.0))),
        pdf(frame, Some(Rect::new(0.0, 0.0, 0.0, 10.0))),
        whole("sha256:00", 1, size, "paper.pdf"),
        whole(&hash(), 0, size, "paper.pdf"),
        whole(&hash(), 1, (612.0, 0.0), "paper.pdf"),
        whole(&hash(), 1, size, "a/b.pdf"),
        whole(&hash(), 1, size, ""),
    ];
    for shape in bad {
        let err = add(shape.clone()).unwrap_err();
        assert_eq!(err.code(), "invalid", "{shape:?}");
        assert!(err.hint().is_some());
    }
}

#[test]
fn the_shown_region_is_the_crop_or_the_whole_page() {
    let frame = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert_eq!(pdf(frame, None).shown(), Some(Rect::new(0.0, 0.0, 612.0, 792.0)));
    let crop = Rect::new(72.0, 180.0, 300.0, 120.0);
    assert_eq!(pdf(frame, Some(crop)).shown(), Some(crop));
    assert_eq!(Shape::Button { frame }.shown(), None);
}

#[test]
fn a_pdf_item_is_one_add_line_and_reads_back() {
    let (mut log, mut bytes) = log();
    let line = bytes.lines().nth(1).unwrap();
    assert_eq!(
        line,
        format!(
            r#"{{"n":1,"t":"2025-11-10T16:00:00.000Z","by":"user","op":"add","item":{{"id":"i1","page":"p1","kind":"pdf","x":0,"y":0,"w":16000,"h":20706,"hash":"{}","pdfpage":1,"pagesize":[6120,7920],"name":"paper.pdf"}},"step":1}}"#,
            hash()
        )
    );
    let snippet = json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 24000,
        "y": 3000, "w": 6000, "h": 2400, "hash": hash(), "pdfpage": 3, "pagesize": [6120, 7920],
        "crop": [720, 1800, 3000, 1200] } });
    bytes += &log.apply(&Author::User, AT, &snippet).unwrap().bytes;
    let tail = r#""pdfpage":3,"pagesize":[6120,7920],"crop":[720,1800,3000,1200]},"step":2}"#;
    assert!(bytes.ends_with(&format!("{tail}\n")), "{bytes}");
    let file = read(bytes.as_bytes()).unwrap();
    assert_eq!(file.canvas().unwrap(), *log.canvas(), "the file replays to the same canvas");
    let moved = json!({"op": "move", "ids": ["i1"], "dx": 100, "dy": -50});
    log.apply(&Author::User, AT, &moved).unwrap();
    let size = Edit { size: Some([8000, 10353]), ..Default::default() };
    assert_eq!(log.edit(&Author::User, AT, "d1", &size).unwrap().frame, [100, -50, 8000, 10353]);
    let item = log.canvas().item(ItemId::new(1)).unwrap();
    assert!(matches!(item.shape(), Shape::Pdf { pdf_page: 1, crop: None, .. }));
    let bad = json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 0, "y": 0,
        "w": 10, "h": 10, "hash": hash(), "pdfpage": 1, "pagesize": [6120] } });
    assert_eq!(log.apply(&Author::User, AT, &bad).unwrap_err().code(), "invalid");
}

#[test]
fn the_paper_draws_under_ink_and_the_eraser_never_takes_it() {
    let (mut log, _) = log();
    log.apply(&Author::User, AT, &stroke(1000, 1000)).unwrap();
    // A snippet pasted after the ink still lies under it.
    let snippet = json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 0, "y": 0,
        "w": 3000, "h": 3000, "hash": hash(), "pdfpage": 2, "pagesize": [6120, 7920] } });
    log.apply(&Author::User, AT, &snippet).unwrap();
    let c = log.canvas();
    let page = c.pages().next().unwrap().id();
    let drawn: Vec<u64> = c.drawn(page).map(|i| i.id().get()).collect();
    assert_eq!(drawn, [1, 3, 2]);
    let on_ink = Point::new(110.0, 100.0);
    assert_eq!(c.item_at(page, on_ink, 0.0), Some(ItemId::new(2)), "the ink over the page wins");
    assert_eq!(c.item_at(page, Point::new(250.0, 250.0), 0.0), Some(ItemId::new(3)));
    assert_eq!(c.item_at(page, Point::new(1000.0, 1000.0), 0.0), Some(ItemId::new(1)));
    let erased = c.items_erased(page, &[Point::new(0.0, 100.0), Point::new(300.0, 100.0)], 5.0);
    assert_eq!(erased, [ItemId::new(2)], "the eraser takes the ink and not the paper");
    let around = |w: f64, h: f64| {
        vec![Point::new(-1.0, -1.0), Point::new(w, -1.0), Point::new(w, h), Point::new(-1.0, h)]
    };
    assert_eq!(c.items_in_lasso(page, &around(400.0, 400.0)), [ItemId::new(3), ItemId::new(2)]);
    assert_eq!(
        c.items_in_lasso(page, &around(250.0, 250.0)),
        [ItemId::new(2)],
        "not the whole page"
    );
}

#[test]
fn the_packet_names_the_page_under_new_ink_and_leaves_the_rest_out() {
    let (mut log, _) = log();
    let below = json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 0, "y": 21106,
        "w": 16000, "h": 20706, "hash": hash(), "pdfpage": 2, "pagesize": [6120, 7920],
        "name": "paper.pdf" } });
    log.apply(&Author::User, AT, &below).unwrap();
    log.apply(&Author::User, AT, &stroke(1000, 1000)).unwrap();
    let packet = log.packet("paper.pdf", None, 0, &[], &[]).unwrap();
    let text = packet.text();
    assert!(text.contains("c3 ink 99,99 43x3 colour=red strokes=1 over=d1 img=c3.png\n"), "{text}");
    let page = format!("d1 pdf 0,0 1600x2071 pdfpage=1 name=paper.pdf blob={}\n", hash());
    assert!(text.contains(&format!("# page\n{page}")), "{text}");
    assert!(!text.contains("d2 pdf"), "no ink on page 2, so no line: {text}");
    assert_eq!(packet.images.len(), 1, "a PDF page is never an image");
    let picked = log.packet("paper.pdf", None, 0, &["d2".into()], &[]).unwrap();
    let text = picked.text();
    assert!(text.contains("d1 pdf") && text.contains("d2 pdf"), "{text}");
    assert_eq!(picked.selected, ["d2"]);
    let read = slinger_core::packet::Packet::read(&text).unwrap();
    let line = read.page.iter().find(|e| e.id == "d2").unwrap();
    assert_eq!((line.kind.as_str(), line.field("pdfpage")), ("pdf", Some("2")));
}

#[test]
fn a_snippet_says_its_crop_and_an_agent_s_goes_in_made() {
    let (mut log, _) = log();
    let snippet = json!({ "op": "add", "item": { "page": "p1", "kind": "pdf", "x": 24000,
        "y": 3000, "w": 6000, "h": 2400, "hash": hash(), "pdfpage": 3, "pagesize": [6120, 7920],
        "crop": [720, 1800, 3000, 1200], "name": "paper.pdf" } });
    let scout = Author::Agent("scout".into());
    log.apply(&scout, AT, &snippet).unwrap();
    let text = log.packet("paper.pdf", None, 0, &[], &[]).unwrap().text();
    let line = format!(
        "# made\nd2 pdf 2400,300 600x240 by=scout pdfpage=3 crop=72,180,300,120 name=paper.pdf blob={}\n",
        hash()
    );
    assert!(text.contains(&line), "{text}");
    let removed = log.remove(&scout, AT, "d1");
    assert_eq!(removed.unwrap_err().code(), "invalid", "an agent cannot take the user's page");
}

#[test]
fn a_crop_draws_the_page_as_a_grey_frame() {
    let (log, _) = log();
    let images = log.crop("p1:0,0,1000,1000", "crop").unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(log.crop("d1", "d1").unwrap().len(), 1, "an item crop draws its frame and label");
}

/// Words in canvas tenths, as the app writes them: `(text, x, y)`, each 40
/// units wide and 10 high.
fn words(list: &[(&str, i64, i64)]) -> Vec<u8> {
    let list =
        list.iter().map(|(t, x, y)| json!({ "text": t, "x": x, "y": y, "w": 400, "h": 100 }));
    serde_json::to_vec(&list.collect::<Vec<_>>()).unwrap()
}

#[test]
fn a_sling_asks_for_the_area_of_the_page_under_each_new_cluster() {
    let (mut log, _) = log();
    assert_eq!(log.backdrops(None, 0).unwrap(), [], "no ink, nothing to render");
    log.apply(&Author::User, AT, &stroke(1000, 1000)).unwrap();
    let [area] = log.backdrops(None, 0).unwrap().try_into().unwrap();
    assert_eq!((area.item, area.pdf_page, area.crop), (ItemId::new(1), 1, None));
    assert_eq!(rect_tenths(area.area), [825, 825, 750, 350], "the image frame ∩ the page");
    assert_eq!(rect_tenths(area.frame), [0, 0, 16000, 20706]);
    assert_eq!((area.width, area.height), (27, 13));
    assert_eq!(log.backdrops(None, 2).unwrap(), [], "nothing new");
    assert_eq!(log.backdrops(Some("p9"), 0).unwrap_err().code(), "not_found");
}

#[test]
fn the_words_under_new_ink_end_its_line() {
    let (mut log, _) = log();
    log.apply(&Author::User, AT, &stroke(1000, 1000)).unwrap();
    let [area] = log.backdrops(None, 0).unwrap().try_into().unwrap();
    let rgb = vec![200; (area.width * area.height * 3) as usize];
    let said = words(&[("under", 1000, 950), ("the line below", 1000, 1200), ("x\"y", 1100, 950)]);
    let area = rect_tenths(area.area);
    let under = backdrop("i1", area, rgb.clone(), Some(&said)).unwrap();
    let text = log.packet("paper.pdf", None, 0, &[], std::slice::from_ref(&under)).unwrap().text();
    let line = "c2 ink 99,99 43x3 colour=red strokes=1 over=d1 img=c2.png \"under x\\\"y\"\n";
    assert!(text.contains(line), "{text}");
    let read = Packet::read(&text).unwrap();
    assert_eq!(read.new[0].text.as_deref(), Some("under x\"y"));
    let again = log.packet("paper.pdf", None, 2, &[], &[under]).unwrap().text();
    assert!(!again.contains("under"), "a page line never carries words: {again}");

    let short = backdrop("d1", area, rgb[1..].to_vec(), None).unwrap_err();
    assert_eq!(short.code(), "invalid");
    assert_eq!(backdrop("i1", area, rgb.clone(), Some(b"nope")).unwrap_err().code(), "invalid");
    assert_eq!(backdrop("x", area, rgb, None).unwrap_err().code(), "invalid");
}

#[test]
fn ink_drawn_after_the_areas_were_asked_shows_on_white() {
    let (mut log, _) = log();
    log.apply(&Author::User, AT, &stroke(1000, 1000)).unwrap();
    let [area] = log.backdrops(None, 0).unwrap().try_into().unwrap();
    let rgb = vec![0; (area.width * area.height * 3) as usize];
    let under = backdrop("i1", rect_tenths(area.area), rgb, None).unwrap();
    log.apply(&Author::User, AT, &stroke(9000, 9000)).unwrap();
    let packet = log.packet("paper.pdf", None, 0, &[], &[under]).unwrap();
    assert_eq!(packet.images.len(), 2, "both clusters have an image, the late one on white");
}
