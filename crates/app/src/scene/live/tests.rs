use std::rc::Rc;

use super::*;
use crate::backend::Problem;
use crate::config::{Rgb, Theme};
use crate::layout::Viewport;
use crate::panes::{Kind, Pane, Panes};
use crate::scene::Notes;
use crate::scene::canvas::{self, Item, Shape};
use crate::scene::cursor::Cursor;
use crate::ui::{Code, Shown};

const SCREEN: Size = Size { width: 800.0, height: 600.0 };
/// Where the pane is in the window.
const PANE: Rect = Rect { x: 300.0, y: 40.0, width: 800.0, height: 600.0 };

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn live(id: u64, frame: Rect, open: &str) -> Item {
    let shape = Shape::Live { frame, name: "pick-slot".into(), open: open.into() };
    Item { id: ItemId(id), colour: Rgb { r: 0, g: 0, b: 0 }, shape }
}

fn page(open: &str) -> Arc<Page> {
    let hash = format!("sha256:{}", "a".repeat(64));
    let code = Code { name: "pick-slot".into(), version: 1, hash, html: Vec::new() };
    let shown = Shown { code, open: Some(open.into()), data: serde_json::Value::Null };
    Arc::new(Page::new(shown, "canvas").unwrap())
}

/// The canvas pane that shows `items` through `view`.
fn pane(items: &[Item], view: Viewport) -> (PaneId, Vec<CanvasPane>) {
    let id = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() }).focus();
    let scene = canvas::build(items, view, SCREEN, &Theme::default(), None);
    (id, vec![CanvasPane { pane: id, frame: PANE, scene: Rc::new(scene), cursor: Cursor::Arrow }])
}

#[test]
fn a_live_item_s_page_shows_over_its_box_and_follows_a_pan_and_a_zoom() {
    let items = [live(2, rect(100.0, 50.0, 320.0, 240.0), "pick-slot#12")];
    let shown_pages = HashMap::from([("pick-slot#12".to_string(), page("pick-slot#12"))]);
    let (id, placed) = pane(&items, Viewport::default());
    let shown = webs(&placed, &shown_pages);
    let want = Live { pane: Some(id), item: ItemId(2), clip: PANE, zoom: 1.0, shows: Shows::Page };
    assert_eq!(shown.len(), 1);
    assert_eq!((shown[0].frame, shown[0].live), (rect(400.0, 90.0, 320.0, 240.0), Some(want)));
    assert!(shown[0].page.context.contains(r#""host":"canvas""#));

    let view = Viewport::default().pan(-20.0, 10.0).zoom_about(Point { x: 0.0, y: 0.0 }, 2.0);
    let (_, placed) = pane(&items, view);
    let moved = webs(&placed, &shown_pages);
    let zoomed = Live { zoom: 2.0, ..want };
    assert_eq!(moved[0].frame, rect(300.0 + 160.0, 40.0 + 120.0, 640.0, 480.0));
    assert_eq!(moved[0].live, Some(zoomed), "its page is drawn at the canvas's zoom");
}

#[test]
fn a_box_and_its_name_show_until_the_page_comes_and_an_item_off_screen_has_none() {
    let items = [
        live(2, rect(0.0, 0.0, 320.0, 240.0), "pick-slot#12"),
        live(3, rect(5000.0, 0.0, 320.0, 240.0), "pick-slot#13"),
        live(4, rect(400.0, 0.0, 320.0, 240.0), "pick-slot#12"),
    ];
    let (_, placed) = pane(&items, Viewport::default());
    assert_eq!(showings(&placed), ["pick-slot#12"], "each showing on the screen once");
    assert_eq!(webs(&placed, &HashMap::new()), [], "no page yet: only its box and name");
    let Mark::Live { name, at, size, .. } = &placed[0].scene.draws[0].mark else {
        panic!("{:?}", placed[0].scene.draws[0].mark)
    };
    assert_eq!((name.as_str(), *at, *size), ("pick-slot", Point { x: 8.0, y: 8.0 }, 16.0));
    let shown_pages = HashMap::from([("pick-slot#12".to_string(), page("pick-slot#12"))]);
    let both = webs(&placed, &shown_pages);
    let items: Vec<_> = both.iter().map(|w| w.live.map(|l| l.item.0)).collect();
    assert_eq!(items, [Some(2), Some(4)], "two items of one showing each show it");
}

#[test]
fn alone_a_live_item_shows_its_default_size_in_the_middle() {
    let web = alone(page("pick-slot#12"), SCREEN, 1.5);
    assert_eq!(web.frame, rect(160.0, 120.0, 480.0, 360.0));
    assert_eq!(web.live.map(|l| (l.pane, l.clip, l.zoom)), Some((None, SCREEN.rect(), 1.5)));
}

#[test]
fn an_item_narrower_than_48_points_on_the_screen_takes_no_presses() {
    let items = [live(2, rect(0.0, 0.0, 320.0, 240.0), "pick-slot#12")];
    let shown_pages = HashMap::from([("pick-slot#12".to_string(), page("pick-slot#12"))]);
    let shows = |zoom: f64| {
        let view = Viewport::default().zoom_about(Point { x: 0.0, y: 0.0 }, zoom);
        let (_, placed) = pane(&items, view);
        webs(&placed, &shown_pages)[0].live.map(|l| l.shows)
    };
    assert_eq!(shows(0.15), Some(Shows::Page), "48 points wide");
    assert_eq!(shows(0.14), Some(Shows::Still));
    assert_eq!((Shows::at(48.0), Shows::at(47.9)), (Shows::Page, Shows::Still));
}

#[test]
fn a_reply_its_agent_did_not_take_shows_along_the_top_of_its_item_cut_off_at_the_pane() {
    let theme = Theme::default();
    let items = [
        live(2, rect(100.0, -10.0, 320.0, 240.0), "pick-slot#12"),
        live(3, rect(500.0, 50.0, 320.0, 240.0), "pick-slot#13"),
    ];
    let shown_pages = HashMap::from([
        ("pick-slot#12".to_string(), page("pick-slot#12")),
        ("pick-slot#13".to_string(), page("pick-slot#13")),
    ]);
    let (_, placed) = pane(&items, Viewport::default());
    let shown = webs(&placed, &shown_pages);
    let problem = Problem { message: "scout is not running".into(), hint: None };
    let notes = Notes::from([("pick-slot#12".to_string(), problem)]);
    let labels = super::notes(&shown, &notes, &theme);
    assert_eq!(labels.len(), 1, "only the item whose reply was not taken");
    assert_eq!(labels[0].text, "✗ scout is not running");
    assert_eq!((labels[0].colour, labels[0].fill), (theme.error, Some(theme.background)));
    assert_eq!(labels[0].frame, rect(400.0, 40.0, 320.0, 12.0), "cut off at the pane's top");
    assert_eq!(super::notes(&shown, &Notes::new(), &theme), []);
}
