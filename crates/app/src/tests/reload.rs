//! `reload-ui` (slinger-c19fe7): the agent UI pane's view closes until the
//! watch fetches its showing again, and then a new view loads it; on a
//! canvas pane it asks for nothing. A real web view cannot be seen here: the
//! scene's `webs` are what `platform::web` opens and closes by.

use super::*;
use crate::commands::context::Subject;

fn board() -> ui::Shown {
    let hash = format!("sha256:{}", "b".repeat(64));
    let code = ui::Code { name: "board".into(), version: 1, hash, html: b"<p>hi</p>".to_vec() };
    ui::Shown { code, open: None, data: serde_json::Value::Null }
}

impl World {
    fn reload(&mut self) -> Routed {
        let w = self;
        let event = Event::Command(Command::ReloadUi);
        route(
            event,
            &mut w.shared,
            &mut w.canvases,
            &mut w.tools,
            &w.settings,
            &w.socket,
            &mut w.sidebar,
        )
    }

    fn webs(&mut self) -> usize {
        let w = self;
        view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases).webs.len()
    }
}

#[test]
fn reload_ui_closes_the_pane_s_view_until_the_watch_fetches_it_again() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert!(matches!(w.reload(), Routed::Answer(_)), "a canvas pane: no refetch");

        w.shared.panes = Panes::new(Pane { kind: PaneKind::Ui, name: "board".into() });
        assert!(w.shared.fetched(vec![("board".into(), Ok(board()))]));
        assert_eq!(w.webs(), 1, "the page shows");
        let offered = w.offers(&Subject::Pane { ui: true });
        assert!(offered.iter().any(|i| i.title == "Reload agent UI" && i.enabled), "offered");

        assert!(matches!(w.reload(), Routed::Refetch), "the watch fetches it again");
        assert_eq!(w.webs(), 0, "its view closes");
        assert!(!w.shared.uis.contains_key("board"));
        let still = w.offers(&Subject::Pane { ui: true });
        assert!(still.iter().any(|i| i.title == "Reload agent UI" && i.enabled), "it can retry");

        w.shared.fetched(vec![("board".into(), Ok(board()))]);
        assert_eq!(w.webs(), 1, "a new view loads it");
    });
}
