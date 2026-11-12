use std::fs;

use slinger_control::UiOpen;

use super::*;
use crate::backend::testing::{user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_daemon};
use crate::config::{Config, Rgb};
use crate::layout::{CANVAS_MARGIN, Rect, WINDOW};
use crate::panes::{Kind, Pane, Panes};
use crate::scene::canvas::ItemId;
use crate::scene::ink::Pen;
use crate::scene::tools::Tool;
use crate::scene::{Notes, Uis, Web};

/// The live items' pages the window shows.
fn webs(canvases: &mut Canvases, panes: &Panes) -> Vec<Web> {
    let (theme, side) = (Config::default().theme, crate::scene::sidebar::Sidebar::new(false));
    canvases.poll();
    let drawn = canvases.show(panes, WINDOW.rect(), &theme);
    let mut scene =
        crate::scene::build(WINDOW, panes, &theme, &[], &side, drawn, &Uis::new(), &Notes::new());
    canvases.show_live(&mut scene);
    scene.webs
}

fn pen(x: f64, y: f64) -> Pen {
    Pen { at: Point { x, y }, when: 0.0, touch: false, eraser: false }
}

#[test]
fn an_agent_s_live_item_shows_its_page_which_takes_presses_until_it_is_selected() {
    with_daemon(|daemon| {
        let _notes = open_notes(daemon);
        user_stroke(daemon, "notes");
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pick.html"), "<p>pick</p>").unwrap();
        daemon.ui_put("pick", &dir.path().join("pick.html"), "scout").unwrap();
        let ask = UiOpen {
            ui: "pick".into(),
            agent: "scout".into(),
            host: "canvas".into(),
            canvas: Some("notes".into()),
            item: Some("i1".into()),
            ..UiOpen::default()
        };
        let opened = daemon.ui_open(ask).unwrap();
        let item = opened.item.unwrap();
        // In the pane: a new view has the canvas origin in by the margin.
        let unit = |v: i64| f64::from(i32::try_from(v).unwrap()) / 10.0;
        let (x, y) = (unit(item.x) + CANVAS_MARGIN, unit(item.y) + CANVAS_MARGIN);
        let frame = Rect { x, y, width: unit(item.w), height: unit(item.h) };

        let panes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let mut canvases = Canvases::new(daemon.clone());
        let mut shown = Vec::new();
        wait_until(|| {
            shown = webs(&mut canvases, &panes);
            !shown.is_empty()
        });
        let live = shown[0].live.unwrap();
        assert_eq!((shown.len(), shown[0].frame), (1, frame), "over its box");
        assert_eq!((live.item, live.zoom), (ItemId(item.item[1..].parse().unwrap()), 1.0));
        assert_eq!(shown[0].page.open.as_deref(), Some(opened.open.as_str()));
        assert!(shown[0].page.context.contains(r#""host":"canvas""#));

        let (area, lasso) = (WINDOW.rect(), (Tool::Lasso, Rgb { r: 0, g: 0, b: 0 }));
        let on = pen(frame.x + 10.0, frame.y + 10.0);
        assert!(!canvases.pen_down(&panes, (panes.focus(), area), on, lasso), "the page's");
        assert!(canvases.pen_down(&panes, (panes.focus(), area), pen(5.0, 5.0), lasso));
        for (x, y) in [(900.0, 5.0), (900.0, 600.0), (5.0, 600.0)] {
            canvases.pen_drag(&panes, pen(x, y));
        }
        canvases.pen_up(&panes, pen(5.0, 5.0), Some((panes.focus(), area)));
        assert_eq!(canvases.selection().map(|s| s.ids.len()), Some(2), "the stroke and the item");
        assert!(canvases.pen_down(&panes, (panes.focus(), area), on, lasso), "selected: a drag");
    });
}
