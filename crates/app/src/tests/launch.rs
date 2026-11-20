//! A bundled app's launch line (slinger-5e8b60): while it starts slingerd,
//! or that failed, or slingerd is stopped, the line replaces the "does not
//! answer" problems and their shell hint, in the focused pane and the
//! sidebar; an app that starts no daemon says what it said before.

use super::*;
use crate::scene::daemon::{Daemon, STARTING};
use crate::scene::taps::Tap;

/// The window on a socket nobody serves, once its canvas said so.
fn lost(config: &Path) -> World {
    let socket = config.join("none.sock");
    let settings = Settings::load(ConfigFiles::at(config.join("config.toml")));
    let panes = Panes::new(Pane { kind: PaneKind::Canvas, name: "notes".into() });
    let mut w = World {
        tools: Tools::new(settings.config.palette.clone()),
        settings,
        shared: Shared { panes, ..Shared::default() },
        sidebar: Sidebar::new(true),
        canvases: Canvases::new(control::Client::new(&socket)),
        socket,
    };
    let problem = backend::Problem { message: "slingerd does not answer".into(), hint: None };
    w.sidebar.update(Err(problem));
    w.until(|w| w.canvases.daemon_problems().next().is_some());
    w
}

/// Lines of text, each with its colour.
type Lines = Vec<(String, config::Rgb)>;

impl World {
    /// The focused pane's and the sidebar's lines.
    fn said(&mut self) -> (Lines, Lines) {
        let w = self;
        let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
        let pane = scene.labels.iter().map(|l| (l.text.clone(), l.colour)).collect();
        let side = scene.sidebar.iter().map(|r| (r.label.text.clone(), r.label.colour)).collect();
        (pane, side)
    }
}

#[test]
fn starting_shows_one_muted_line_and_no_shell_hint_in_the_pane_and_the_sidebar() {
    let config = tempfile::tempdir().unwrap();
    let mut w = lost(config.path());
    let theme = w.settings.config.theme;
    let (pane, side) = w.said();
    assert!(pane.iter().any(|(t, c)| t.starts_with("✗ ") && *c == theme.error), "{pane:?}");
    assert!(side.iter().any(|(t, _)| t.starts_with("✗ ")), "not managed: as before");

    assert!(w.sidebar.set_daemon(Some(Daemon::Starting)));
    let (pane, side) = w.said();
    let starting = (STARTING.to_string(), theme.muted);
    assert!(pane.contains(&starting), "{pane:?}");
    assert!(side.contains(&starting), "{side:?}");
    let all = || pane.iter().chain(&side);
    assert!(!all().any(|(t, _)| t.contains('✗') || t.contains("--socket")), "{pane:?} {side:?}");
}

#[test]
fn a_failed_start_shows_why_and_the_log_and_a_stopped_daemon_offers_to_start() {
    let config = tempfile::tempdir().unwrap();
    let mut w = lost(config.path());
    let log = "/d/slingerd.log".to_string();
    w.sidebar.set_daemon(Some(Daemon::Failed { code: Some(1), said: None, log }));
    let (pane, _) = w.said();
    let texts: Vec<&str> = pane.iter().map(|(t, _)| t.as_str()).collect();
    assert!(texts.contains(&"✗ slingerd did not start (exit 1)"), "{texts:?}");
    assert!(texts.contains(&"see /d/slingerd.log"), "{texts:?}");

    w.sidebar.set_daemon(Some(Daemon::Stopped));
    let (pane, side) = w.said();
    assert!(pane.iter().any(|(t, _)| t == "slingerd is stopped"), "{pane:?}");
    assert!(side.iter().any(|(t, _)| t == "Start slingerd"), "{side:?}");
    let start = Tap::Run(Command::StartSlingerd);
    let theme = w.settings.config.theme;
    assert!(w.sidebar.targets(&theme, w.shared.size.rect()).iter().any(|t| t.1 == start));
    assert!(w.send(Event::Command(Command::StartSlingerd)), "the window starts it");
}

/// slinger-724433: the launch line hides only the daemon's problems; the
/// kept ink's file that cannot be read still shows.
#[test]
fn starting_still_shows_a_problem_of_the_app_s_own() {
    let config = tempfile::tempdir().unwrap();
    let mut w = lost(config.path());
    let path = config.path().join("unsent-ink.json");
    std::fs::write(&path, "{nope").unwrap();
    let keeping = crate::backend::Unsent::at(path.clone());
    w.canvases = Canvases::new(control::Client::new(&w.socket)).keeping(keeping);
    w.sidebar.set_daemon(Some(Daemon::Starting));
    let (pane, _) = w.said();
    let shown = |t: &String| t.starts_with(&format!("✗ {}", path.display()));
    assert!(pane.iter().any(|(t, _)| shown(t)), "{pane:?}");
    assert!(!pane.iter().any(|(t, _)| t.contains("does not answer")), "{pane:?}");
}
