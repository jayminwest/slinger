use serde_json::json;
use slinger_control::{CanvasOps, Client};

use crate::backend::testing::wait_until;
use crate::backend::tests::with_daemon;
use crate::canvases::Canvases;
use crate::commands::{Chord, Command};
use crate::config::Config;
use crate::layout::{Point, Rect, WINDOW};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::status;

/// A canvas `main` with a second page, `Two`: "plan A" and "other" on the
/// first, "the PLAN" far down the second.
fn written(daemon: &Client) {
    daemon.canvas_create("main").unwrap();
    let text = |page: &str, x: i64, y: i64, words: &str| json!({ "op": "add", "item": { "page": page, "kind": "text", "x": x, "y": y, "text": words } });
    let ops = json!([
        { "op": "page.add", "name": "Two" },
        text("p1", 0, 0, "plan A"),
        text("p1", 0, 600, "other"),
        text("p2", 30_000, 40_000, "the PLAN"),
    ]);
    daemon
        .canvas_apply(CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: None })
        .unwrap();
}

/// `canvases` shows `panes` once the page asked for has arrived.
fn shown(canvases: &mut Canvases, panes: &Panes) {
    wait_until(|| {
        canvases.poll();
        canvases.show(panes, WINDOW.rect(), &Config::default().theme);
        canvases.pages_of(panes, panes.focus()).is_some()
    });
}

/// Types `words` into the open entry and presses return.
fn answer(canvases: &mut Canvases, words: &str) {
    assert!(canvases.typing(), "the find's entry is open");
    canvases.sling_typed(words);
    assert!(canvases.sling_key(Chord::new(0, '\r')));
    assert!(!canvases.typing());
}

#[test]
fn find_goes_to_each_text_that_holds_the_words_page_by_page_and_centres_it() {
    with_daemon(|daemon| {
        written(daemon);
        let panes = Panes::new(Pane { kind: Kind::Canvas, name: "main".into() });
        let (pane, frame) = (panes.focus(), WINDOW.rect());
        let mut canvases = Canvases::new(daemon.clone());
        shown(&mut canvases, &panes);

        assert!(canvases.command(&panes, (pane, frame), Command::Find));
        answer(&mut canvases, "plan");
        assert_eq!(canvases.said().as_deref(), Some("“plan” 1/2 · Page 1"));

        // Find next goes to the second page and centres the text there.
        assert!(canvases.command(&panes, (pane, frame), Command::FindNext));
        assert_eq!(canvases.said().as_deref(), Some("“plan” 2/2 · Two"));
        let view = canvases.view(pane);
        assert_eq!(view.page, Some(("main".into(), "p2".into())));
        let content = status::content(frame);
        let middle =
            view.viewport.to_canvas(Point { x: content.width / 2.0, y: content.height / 2.0 });
        let text = Rect { x: 3000.0, y: 4000.0, width: 0.0, height: 0.0 };
        assert!(middle.x > text.x && middle.y > text.y && middle.y < text.y + 24.0, "{middle:?}");
        shown(&mut canvases, &panes);

        // After the last, the first; the entry starts from the last words.
        canvases.command(&panes, (pane, frame), Command::FindNext);
        assert_eq!(canvases.said().as_deref(), Some("“plan” 1/2 · Page 1"));
        canvases.command(&panes, (pane, frame), Command::Find);
        assert_eq!(canvases.asks.entry(&Config::default().theme).unwrap().text, "plan");
        answer(&mut canvases, "nowhere");
        assert_eq!(canvases.said().as_deref(), Some("No text holds “nowhere”"));
        assert_eq!(canvases.view(pane).page, Some(("main".into(), "p1".into())), "it stays");
    });
}

#[test]
fn find_next_with_no_find_yet_asks_and_a_pane_with_no_canvas_says_so() {
    with_daemon(|daemon| {
        written(daemon);
        let panes = Panes::new(Pane { kind: Kind::Canvas, name: "main".into() });
        let mut canvases = Canvases::new(daemon.clone());
        shown(&mut canvases, &panes);
        canvases.command(&panes, (panes.focus(), WINDOW.rect()), Command::FindNext);
        answer(&mut canvases, "OTHER");
        assert_eq!(canvases.said().as_deref(), Some("“OTHER” 1/1 · Page 1"));

        let ui = Panes::new(Pane { kind: Kind::Ui, name: "pick".into() });
        canvases.command(&ui, (ui.focus(), WINDOW.rect()), Command::Find);
        assert!(!canvases.typing());
        assert_eq!(canvases.said().as_deref(), Some("No canvas in this pane to find on"));
    });
}
