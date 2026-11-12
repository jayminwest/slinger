use slinger_control::{Client, EventFilter, Put};

use super::*;
use crate::backend::testing::{fake_claude, user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::commands::{Chord, Command};
use crate::config::Config;
use crate::layout::WINDOW;
use crate::panes::{Kind, Pane};
use crate::scene::canvas::{Draw, Mark};
use crate::scene::ink::Pen;
use crate::scene::sidebar::{Agent, Status};
use crate::scene::tools::Tool;

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

/// What the pane draws now, after taking in what the daemon said.
fn draws(canvases: &mut Canvases, panes: &Panes) -> Vec<Draw> {
    canvases.poll();
    let shown = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
    shown.first().map(|p| p.scene.draws.clone()).unwrap_or_default()
}

/// The button the pane draws, once the daemon holds it: its box and label.
fn button(canvases: &mut Canvases, panes: &Panes, daemon: &Client) -> (Rect, String) {
    let mut found = None;
    wait_until(|| {
        let held = daemon.canvas_show("notes", None).unwrap().items;
        let stored = held.iter().any(|i| i["kind"] == "button");
        found = draws(canvases, panes).into_iter().find_map(|d| match d.mark {
            Mark::Button { frame, label, .. } => Some((frame, label)),
            _ => None,
        });
        stored && found.is_some()
    });
    found.unwrap()
}

/// What the agent's process has read, once it holds `n` whole messages.
/// It writes a message a line at a time and a long line in pieces: a line
/// counts once its newline is in.
fn read(home: &std::path::Path, n: usize) -> Vec<String> {
    let mut lines = Vec::new();
    wait_until(|| {
        let text = std::fs::read_to_string(home.join("in.txt")).unwrap_or_default();
        let whole = text.rfind('\n').map_or("", |end| &text[..end]);
        lines = whole.lines().map(str::to_string).collect();
        lines.len() >= n
    });
    lines
}

/// Polls `canvases` until the daemon has recorded `n` slings on `notes`
/// and the pane's sling line is gone: the last sling recorded is where what
/// is new starts for the next. The agent reads a sling before the daemon
/// records it, so its reading is not the sign.
fn recorded(canvases: &mut Canvases, daemon: &Client, n: usize) {
    let (kind, canvas) = (Some("sling_sent".into()), Some("notes".into()));
    let filter = EventFilter { kind, canvas, ..EventFilter::default() };
    let theme = Config::default().theme;
    wait_until(|| {
        canvases.poll();
        let slings = daemon.record_list(filter.clone()).unwrap();
        slings.len() >= n && canvases.sling.labels(&theme).is_empty()
    });
}

fn tap(canvases: &mut Canvases, panes: &Panes, on: Rect) -> bool {
    let at = Point { x: on.x + on.width / 2.0, y: on.y + on.height / 2.0 };
    let pen = Pen { at, when: 0.0, touch: false, eraser: false, shift: false };
    canvases.pen_down(
        panes,
        (panes.focus(), WINDOW.rect()),
        pen,
        (Tool::Pen, Rgb { r: 0, g: 0, b: 0 }),
    )
}

/// Places a button on the pane once its page has come; its box.
fn place(canvases: &mut Canvases, panes: &Panes, daemon: &Client) -> Rect {
    let area = WINDOW.rect();
    wait_until(|| {
        draws(canvases, panes);
        canvases.pages_of(panes, panes.focus()).is_some()
    });
    assert!(canvases.command(panes, (panes.focus(), area), Command::PlaceButton));
    let (frame, label) = button(canvases, panes, daemon);
    assert_eq!(label, "○ send to agent", "no status yet");
    let middle = (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0);
    assert_eq!(middle, (area.width / 2.0, area.height / 2.0), "in the middle of the pane");
    frame
}

/// The agent's answer, put beside the note `c2`, shows in the pane.
fn reply_lands_beside_the_note(canvases: &mut Canvases, panes: &Panes, daemon: &Client) {
    let put = daemon.canvas_put("notes", "scout", "c2", Put::Text("a reply".into())).unwrap();
    assert!(put.x >= 1100, "right of the note: {put:?}");
    wait_until(|| {
        let shown = draws(canvases, panes);
        shown.iter().any(|d| matches!(&d.mark, Mark::Text { text, .. } if text == "a reply"))
    });
}

/// With nothing selected `sling` opens the entry beside the button at
/// `frame`, and a tap sends what was typed there.
fn type_beside_and_tap(canvases: &mut Canvases, panes: &Panes, frame: Rect) {
    assert!(canvases.open_sling(panes, WINDOW.rect()));
    let entry = canvases.sling.entry(&Config::default().theme, None, None).unwrap();
    assert!(entry.frame.y > frame.y + frame.height, "under the button: {entry:?}");
    assert!(entry.placeholder.ends_with("about what is new (or nothing)"), "{entry:?}");
    assert!(!canvases.sling_typed("and now?"));
    assert!(!canvases.sling_key(Chord::new(0, 'w')), "the entry's key");
    assert!(tap(canvases, panes, frame));
    assert!(!canvases.typing(), "sent");
}

#[test]
fn a_placed_button_shows_its_agent_s_status_and_a_tap_slings_what_is_new() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        daemon.session_start("notes").unwrap();
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        let frame = place(&mut canvases, &panes, daemon);
        let scout = Agent { name: "scout".into(), status: Some(Status::Working), note: None };
        canvases.agents(Some(vec![("notes".into(), Some(scout))]));
        assert_eq!(button(&mut canvases, &panes, daemon).1, "● send to agent");

        // A note, then a tap: what is new goes, with no selection and no prompt.
        user_stroke(daemon, "notes");
        assert!(tap(&mut canvases, &panes, frame));
        recorded(&mut canvases, daemon, 1);
        let message = &read(home.path(), 1)[0];
        assert!(message.contains("slinger-sling 1 canvas=notes\\n"), "nothing selected: {message}");
        assert!(message.contains("# new\\nc2 ink"), "the note is new: {message}");
        reply_lands_beside_the_note(&mut canvases, &panes, daemon);

        type_beside_and_tap(&mut canvases, &panes, frame);
        recorded(&mut canvases, daemon, 2);
        let second = &read(home.path(), 2)[1];
        assert!(second.contains("canvas=notes\\n") && second.contains("and now?"), "{second}");
        daemon.session_stop("notes").unwrap();
    });
}

/// The button placed on a pane with no agent running, once the window has
/// heard of it: its box.
fn placed_with_no_agent(canvases: &mut Canvases, panes: &Panes, daemon: &Client) -> Rect {
    wait_until(|| {
        draws(canvases, panes);
        canvases.pages_of(panes, panes.focus()).is_some()
    });
    assert!(canvases.command(panes, (panes.focus(), WINDOW.rect()), Command::PlaceButton));
    let (frame, _) = button(canvases, panes, daemon);
    assert!(canvases.agent_of("notes").is_none());
    // The pill stands under the button the daemon holds: wait for it to be heard.
    wait_until(|| {
        canvases.poll();
        let mirrors = canvases.open.iter().filter_map(|o| o.mirror.as_ref());
        let mut items = mirrors.flat_map(crate::mirror::Mirror::items);
        items.any(|i| matches!(i.shape, crate::scene::canvas::Shape::Button { .. }))
    });
    frame
}

/// The sling's lines under the button once they show.
fn sling_lines(canvases: &mut Canvases, panes: &Panes) -> Vec<crate::scene::Label> {
    let theme = Config::default().theme;
    let mut lines = Vec::new();
    wait_until(|| {
        canvases.poll();
        lines = canvases.sling_marks(panes, WINDOW.rect(), &theme).all();
        !lines.is_empty()
    });
    lines
}

#[test]
fn a_tap_with_no_agent_running_says_why_under_the_button() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        let frame = placed_with_no_agent(&mut canvases, &panes, daemon);
        assert!(tap(&mut canvases, &panes, frame));
        let lines = sling_lines(&mut canvases, &panes);
        assert!(!canvases.typing(), "no entry: the pen cannot press escape");
        assert!(lines[0].text.starts_with("✗ "), "{lines:?}");
        assert!(lines[1].text.contains("session start"), "its hint: {lines:?}");
        assert!(lines[0].frame.y > frame.y + frame.height, "under the button: {lines:?}");
    });
}

/// slinger-6b4b6b: a tap on the button with the feedback entry or a page's
/// new name open closes it, as any press on the canvas does, and slings.
#[test]
fn a_tap_closes_the_feedback_or_rename_entry_and_slings() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        let frame = placed_with_no_agent(&mut canvases, &panes, daemon);
        let here = (panes.focus(), WINDOW.rect());
        for open in [Command::LeaveFeedback, Command::RenamePage] {
            if open == Command::LeaveFeedback {
                assert!(canvases.open_feedback(&panes, here, Some("notes"), Tool::Pen));
            } else {
                assert!(canvases.command(&panes, here, open));
            }
            assert!(canvases.typing(), "{open:?} is open");
            assert!(tap(&mut canvases, &panes, frame));
            assert!(!canvases.typing(), "{open:?} closed");
            let lines = sling_lines(&mut canvases, &panes);
            assert!(lines[0].text.starts_with("✗ "), "it slung: {lines:?}");
        }
    });
}
