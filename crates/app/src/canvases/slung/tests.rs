use slinger_control::{Client, Put};

use super::*;
use crate::backend::testing::{delivered, fake_claude, user_stroke, wait_until, waiting_claude};
use crate::backend::tests::{open_notes, with_store};
use crate::commands::Command;
use crate::config::{Config, Rgb};
use crate::layout::{Point, WINDOW};
use crate::panes::Pane;
use crate::scene::ink::Pen;
use crate::scene::taps::Tap;
use crate::scene::tools::Tool;

const PROMPT: &str = "Read what I wrote and do what it asks.";

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

/// The workspace list the sidebar hands in: notes, with `agent` or none.
fn agents(agent: Option<Agent>) -> Vec<(String, Option<Agent>)> {
    vec![("notes".into(), agent)]
}

fn scout() -> Agent {
    Agent { name: "scout".into(), status: None, note: None, ..Agent::default() }
}

/// Waits for `notes` to arrive in `canvases`.
fn arrived(canvases: &mut Canvases, panes: &Panes) {
    let theme = Config::default().theme;
    wait_until(|| {
        canvases.poll();
        !canvases.show(panes, WINDOW.rect(), &theme).is_empty()
    });
}

/// Lassoes all of the page's one stroke.
fn select_the_stroke(canvases: &mut Canvases, panes: &Panes) {
    let area = WINDOW.rect();
    let pen =
        |x, y| Pen { at: Point { x, y }, when: 0.0, touch: false, eraser: false, shift: false };
    let lasso = (Tool::Lasso, Rgb { r: 0, g: 0, b: 0 });
    assert!(canvases.pen_down(panes, (panes.focus(), area), pen(50.0, 50.0), lasso));
    for (x, y) in [(200.0, 50.0), (200.0, 200.0), (50.0, 200.0)] {
        canvases.pen_drag(panes, pen(x, y), Some((panes.focus(), area)));
    }
    canvases.pen_up(panes, pen(50.0, 50.0), Some((panes.focus(), area)));
    assert!(canvases.selection().is_some());
}

/// The texts of the sling's marks, polling until one starts with `start`.
fn until_marked(canvases: &mut Canvases, panes: &Panes, start: &str) -> Vec<String> {
    let theme = Config::default().theme;
    let mut texts = Vec::new();
    wait_until(|| {
        canvases.poll();
        let labels = canvases.sling_marks(panes, WINDOW.rect(), &theme).all();
        texts = labels.into_iter().map(|l| l.text).collect();
        texts.iter().any(|t| t.starts_with(start))
    });
    texts
}

#[test]
fn one_tap_slings_the_selection_with_the_config_s_words_and_the_pill_follows_the_agent() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&waiting_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        daemon.session_start("notes").unwrap();
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        canvases.agents(Some(agents(Some(scout()))));
        arrived(&mut canvases, &panes);
        select_the_stroke(&mut canvases, &panes);

        let theme = Config::default().theme;
        let Marks { labels, targets, .. } = canvases.sling_marks(&panes, WINDOW.rect(), &theme);
        let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
        let beside = [buttons::SLING, buttons::PROMPT, buttons::DELETE];
        assert_eq!(texts, beside, "beside the selection");
        let taps: Vec<Tap> = targets.iter().map(|t| t.1).collect();
        let runs = [Command::SlingNow, Command::Sling, Command::DeleteSelection].map(Tap::Run);
        assert_eq!(taps, runs);

        assert!(canvases.sling_now_command(&panes, WINDOW.rect(), Profile::Running));
        let message = delivered(home.path(), PROMPT);
        assert!(message.contains("selected=c1"), "{message}");
        let working = until_marked(&mut canvases, &panes, "● scout working");
        assert_eq!(working[1..], [buttons::DELETE], "one sling at a time: {working:?}");
        assert_eq!(canvases.said().as_deref(), Some("Slung to scout"));

        daemon.canvas_put("notes", "scout", "c1", Put::Text("a reply".into())).unwrap();
        std::fs::write(home.path().join("go"), "").unwrap();
        let answered = until_marked(&mut canvases, &panes, "✓ scout answered ·");
        assert_eq!(answered[0], "✓ scout answered · 1 new item");
        assert_eq!(answered[1..], beside, "it answered: the buttons are back");
        assert!(canvases.tap_slung(&panes, WINDOW.rect()), "in sight: a tap clears it");
        let marks = canvases.sling_marks(&panes, WINDOW.rect(), &theme);
        assert_eq!((marks.labels.len(), marks.pill), (3, None), "the buttons");
        daemon.session_stop("notes").unwrap();
    });
}

#[test]
fn one_tap_with_nothing_selected_slings_what_is_new_on_the_page() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        daemon.session_start("notes").unwrap();
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        arrived(&mut canvases, &panes);
        assert!(
            canvases.sling_now_command(&panes, WINDOW.rect(), Profile::Running),
            "no list yet: it tries"
        );
        let message = delivered(home.path(), PROMPT);
        assert!(message.contains("canvas=notes\\n") && message.contains("c1 ink"), "{message}");
        let done = until_marked(&mut canvases, &panes, "✓ scout answered in");
        assert_eq!(done, ["✓ scout answered in text · tap to read"]);
        assert!(canvases.tap_slung(&panes, WINDOW.rect()));
        let marks = canvases.sling_marks(&panes, WINDOW.rect(), &Config::default().theme);
        let lines = marks.panel.expect("the answer").lines;
        assert!(lines.iter().any(|l| l.label.text == "ok"), "{lines:?}");
        // slinger-9fe67e: the next stroke takes the answer away.
        let at = Point { x: 300.0, y: 300.0 };
        let pen = Pen { at, when: 0.0, touch: false, eraser: false, shift: false };
        let ink = (Tool::Pen, Rgb { r: 0, g: 0, b: 0 });
        assert!(canvases.pen_down(&panes, (panes.focus(), WINDOW.rect()), pen, ink));
        let marks = canvases.sling_marks(&panes, WINDOW.rect(), &Config::default().theme);
        assert_eq!((marks.pill, marks.panel), (None, None));
        daemon.session_stop("notes").unwrap();
    });
}

#[test]
fn one_tap_on_a_canvas_with_no_agent_says_so_and_sends_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        arrived(&mut canvases, &panes);
        canvases.agents(Some(agents(None)));
        assert!(canvases.sling_now_command(&panes, WINDOW.rect(), Profile::Running));
        assert_eq!(canvases.said().as_deref(), Some(NO_AGENT));
        assert_eq!(canvases.sling.canvas(), None, "nothing on its way");
    });
}

#[test]
fn one_tap_with_no_canvas_page_does_nothing() {
    let mut canvases = Canvases::new(Client::new("/nonexistent/x.sock"));
    assert!(!canvases.sling_now_command(&notes(), WINDOW.rect(), Profile::Running));
}

#[test]
fn a_blank_prompt_takes_the_config_s_words() {
    let mut canvases = Canvases::new(Client::new("/nonexistent/x.sock"));
    let config = crate::config::parse("[sling]\nprompt = \"Answer in one line.\"\n").unwrap();
    canvases.set_prompt(&config.resolve().prompt);
    assert_eq!(canvases.prompt, "Answer in one line.");
}
