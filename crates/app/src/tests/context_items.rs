//! A context click on a canvas item (slinger-4185a6): Finder's rule
//! selects the clicked item alone unless the click is on the selection,
//! the menu is the item's kind's or the selection's, and its pick acts on
//! the selection; a click on a text being typed offers its clipboard and
//! leaves it typed, and write focus.

use super::step::{STEP_IN, apply};
use super::*;
use crate::commands::context::{Kind, Subject};
use crate::layout::Rect;
use crate::scene::canvas::ItemId;

/// Open box `i1`, box `i2` (folded), text `i3` and arrow `i4`.
fn page(daemon: &control::Client) {
    let item = |item: serde_json::Value| apply(daemon, &item);
    item(serde_json::json!({"page": "p1", "kind": "box", "x": 1000, "y": 1000,
        "w": 4000, "h": 3000}));
    item(serde_json::json!({"page": "p1", "kind": "box", "x": 6000, "y": 1000,
        "w": 2000, "h": 2000}));
    item(serde_json::json!({"page": "p1", "kind": "text", "x": 1000, "y": 5000,
        "text": "note"}));
    item(serde_json::json!({"page": "p1", "kind": "arrow", "pts": [1000, 6500, 3000, 0]}));
    let fold = control::CanvasFold {
        canvas: "notes".into(),
        item: "i2".into(),
        by: "user".into(),
        once: None,
    };
    daemon.canvas_fold(&control::FoldAsk::CanvasFold(fold)).unwrap();
}

/// Where the one pane draws item `id`, once it does.
fn bounds(w: &mut World, id: u64) -> Rect {
    let mut found = None;
    wait_until(|| {
        w.send(Event::Tick);
        let area = w.sidebar.panes(w.shared.size.rect());
        let theme = w.settings.config.theme;
        let panes = w.canvases.show(&w.shared.panes, area, &theme);
        found = panes[0].scene.draws.iter().find(|d| d.id == ItemId(id)).map(|d| d.bounds);
        found.is_some()
    });
    found.unwrap()
}

/// A context click in the middle of `frame`, or on its left edge: the
/// subject the menu is for, checked against what that subject offers.
fn click(w: &mut World, frame: Rect, edge: bool) -> Vec<&'static str> {
    let x = if edge { frame.x } else { frame.x + frame.width / 2.0 };
    let popup = w.context(x, frame.y + frame.height / 2.0).expect("a menu");
    super::context::titles(&popup)
}

fn selected(w: &World) -> Vec<u64> {
    w.canvases.selection().map(|s| s.ids.iter().map(|i| i.0).collect()).unwrap_or_default()
}

fn offered(w: &World, subject: &Subject) -> Vec<&'static str> {
    let items = w.offers(subject);
    let shown = items.iter().filter(|i| i.action != crate::commands::menu::Action::Separator);
    shown.map(|i| i.title).collect()
}

fn is(w: &World, subject: &Subject, menu: &[&'static str]) {
    assert_eq!(menu, offered(w, subject), "{subject:?}");
}

#[test]
fn a_context_click_selects_by_finder_s_rule_and_its_pick_acts_on_the_selection() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let (open, folded) = (bounds(&mut w, 1), bounds(&mut w, 2));
        let (text, arrow) = (bounds(&mut w, 3), bounds(&mut w, 4));

        let menu = click(&mut w, open, true);
        assert_eq!(selected(&w), [1], "the box alone");
        is(&w, &Subject::Item { kind: Kind::Box }, &menu);
        assert!(w.offers(&Subject::Item { kind: Kind::Box }).iter().all(|i| i.enabled));
        let menu = click(&mut w, folded, true);
        assert_eq!(selected(&w), [2]);
        is(&w, &Subject::Item { kind: Kind::Box }, &menu);

        super::step::select_all(&mut w, 4);
        let menu = click(&mut w, text, false);
        assert_eq!(selected(&w).len(), 4, "a click on a selected item keeps the selection");
        is(&w, &Subject::Selection { boxed: true }, &menu);

        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        click(&mut w, open, true);
        assert_eq!(selected(&w), [1]);
        let menu = click(&mut w, arrow, false);
        assert_eq!(selected(&w), [4], "the clicked item, not the one selected before");
        is(&w, &Subject::Item { kind: Kind::Arrow }, &menu);
        assert!(w.send(Event::Command(Command::DeleteSelection)), "the pick");
        w.until(|_| count(daemon, "arrow") == 0);
        assert_eq!((count(daemon, "box"), count(daemon, "text")), (2, 1), "only the arrow");

        let menu = click(&mut w, text, false);
        is(&w, &Subject::Item { kind: Kind::Text }, &menu);
        click(&mut w, open, true);
        assert!(w.send(Event::Command(Command::Fold)), "the pick");
        let fold = |d: &control::Client| {
            let items = d.canvas_show("notes", None).unwrap().items;
            items.iter().find(|i| i["id"] == "i1").map(|b| b["fold"].clone())
        };
        w.until(|_| fold(daemon).is_some_and(|f| !f.is_null()));
    });
}

#[test]
fn in_a_pane_stepped_into_a_box_a_click_hits_what_the_pane_shows() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        super::step::page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let open = bounds(&mut w, 1);
        click(&mut w, open, true);
        assert!(w.send(Event::Key(STEP_IN)));
        let inside = bounds(&mut w, 2);
        let menu = click(&mut w, inside, false);
        assert_eq!(selected(&w), [2], "the stroke the box holds");
        is(&w, &Subject::Item { kind: Kind::Stroke }, &menu);
        let menu = click(&mut w, Rect { x: inside.x + 120.0, ..inside }, false);
        is(&w, &Subject::Canvas, &menu);
        // The box itself is not shown inside it: its edge is empty canvas.
        assert_eq!(selected(&w), [2], "the empty canvas keeps the selection");
    });
}

#[test]
fn a_click_on_a_text_being_typed_offers_its_clipboard_and_it_is_still_typed() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let open = bounds(&mut w, 1);
        w.type_word(700.0, 1.0);
        let popup = w.context(710.0, 310.0);
        let menu = super::context::titles(&popup.expect("a menu"));
        assert_eq!(menu, ["Cut", "Copy", "Paste", "Focus on text"]);
        assert!(w.canvases.editing(), "the text is still typed");

        click(&mut w, open, true);
        assert!(!w.canvases.editing(), "a click elsewhere ends it, as a press does");
        assert_eq!(selected(&w), [1]);
        w.until(|_| count(daemon, "text") == 2);
    });
}
