//! A workspace whose file the daemon cannot use, as the window shows it.

use super::*;
use crate::backend::tests::with_daemon;

/// The labels the window's panes show.
fn labels(w: &mut World) -> Vec<String> {
    let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
    scene.labels.iter().map(|l| l.text.clone()).collect()
}

#[test]
fn a_shown_broken_workspace_says_why_and_nothing_of_the_old_panes_canvas() {
    with_daemon(|d| {
        let notes = open_notes(d);
        let config = tempfile::tempdir().unwrap();
        let settings = Settings::load(ConfigFiles::at(config.path().join("config.toml")));
        // The panes a first launch has before any layout: canvas `main`,
        // which this daemon does not have.
        let mut w = World {
            tools: Tools::new(settings.config.palette.clone()),
            settings,
            shared: Shared::default(),
            sidebar: Sidebar::new(true),
            canvases: Canvases::new(d.clone()),
            socket: "/nonexistent/slinger.sock".into(),
        };
        w.until(|w| w.canvases.daemon_problems().next().is_some());
        let gone = "✗ canvas main not found".to_string();
        assert!(labels(&mut w).contains(&gone), "its own problem while it is the window's pane");

        std::fs::write(notes.path().join("slinger-workspace.toml"), "name = = \"notes\"\n")
            .unwrap();
        let dir = d.workspace_list().unwrap().remove(0).dir;
        w.shared.follow(backend::panes::layout(d, dir.to_str(), None));
        let said = labels(&mut w);
        assert!(said.iter().any(|l| l.starts_with("✗ ") && l.contains("extra `=`")), "{said:?}");
        assert!(!said.contains(&gone), "not the old pane's: {said:?}");
    });
}
