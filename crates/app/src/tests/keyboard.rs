//! Where a paste goes: the window or a page (slinger-145162).

use control::{PaneKind as ControlKind, PaneSplit, PaneTree};

use super::*;
use crate::scene::paste::{Clip, Keyboard};

/// `AppKit` pastes (cmd+V, Flick's dictation) into the view with the
/// keyboard. A press that goes on to an agent UI's page gives it the
/// keyboard; a press the app takes, or a text of its own taking typing,
/// gives it back to the window, so the paste reaches the app.
#[test]
fn a_page_pressed_keeps_the_keyboard_until_the_app_takes_a_press_or_a_text_types() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let (mut w, on_canvas, on_page) = beside(daemon, config.path());
        assert_eq!(drawn(&mut w), Keyboard::Window);
        assert_eq!(press(&mut w, on_page, 1.0), Some(Keyboard::Pressed), "focused, it goes on");
        assert_eq!(drawn(&mut w), Keyboard::Pressed, "and the page keeps it");
        press(&mut w, on_page, 2.0);
        assert_eq!(drawn(&mut w), Keyboard::Pressed, "a press on the focused page goes on too");

        w.key(Chord::new(CMD, '4'), "4");
        assert_eq!(press(&mut w, on_canvas, 3.0), Some(Keyboard::Window), "the app took it");
        assert!(w.canvases.editing(), "a text is being typed");
        let clip = Clip::new(Some("hi".into()), None);
        assert!(w.send(Event::Edit(Edit::Paste(clip))), "and the paste reaches it");

        press(&mut w, on_page, 4.0);
        assert_eq!(drawn(&mut w), Keyboard::Window, "the app's text still takes typing");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.editing());
        assert_eq!(drawn(&mut w), Keyboard::Pressed, "once it ends, the page has it");
        w.key(Chord::new(CMD, 'k'), "k");
        assert!(w.shared.palette.is_open());
        assert_eq!(drawn(&mut w), Keyboard::Window, "the palette's search takes the paste");
    });
}

/// While an entry is open, a press on an agent UI's page still reaches
/// the page, but the entry keeps the keyboard: the scene drawn with the
/// press says so, and the platform takes it back once the press went on
/// (slinger-8d4191). Once the entry closes, the window has it, not the page.
#[test]
fn an_open_entry_keeps_the_keyboard_through_a_press_on_a_page() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let (mut w, _, on_page) = beside(daemon, config.path());
        assert!(w.send(Event::Command(Command::LeaveFeedback)));
        assert_eq!(drawn(&mut w), Keyboard::Entry);
        assert_eq!(press(&mut w, on_page, 1.0), Some(Keyboard::Entry), "focused, it goes on");
        assert!(w.canvases.typing(), "the entry stays open");
        assert_eq!(press(&mut w, on_page, 2.0), None, "a press on the focused page goes on");
        assert_eq!(drawn(&mut w), Keyboard::Entry, "and the entry keeps it");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.typing());
        assert_eq!(drawn(&mut w), Keyboard::Window, "the page never had it");
    });
}

/// A window onto `daemon`'s `notes` canvas (focused) beside an agent UI's
/// page, and a point across from the left of each.
fn beside(daemon: &control::Client, config: &Path) -> (World, f64, f64) {
    let mut w = world(daemon, config);
    let leaf = |pane, kind, target: &str| {
        PaneTree::Pane(control::Pane { pane, kind, target: target.into() })
    };
    let (canvas, ui) = (leaf(1, ControlKind::Canvas, "notes"), leaf(2, ControlKind::Ui, "b"));
    let (first, second) = (canvas.into(), ui.into());
    let tree = PaneTree::Split { split: PaneSplit::Row, ratio: 0.5, first, second };
    w.shared.panes = Panes::follow(&tree, 1);
    w.until(|w| w.canvases.page_names(&w.shared.panes, w.shared.panes.focus()).is_some());
    let area = w.sidebar.panes(w.shared.size.rect());
    let frames: Vec<_> = w.shared.panes.place(area).iter().map(|(_, _, f)| *f).collect();
    (w, frames[0].x + 100.0, frames[1].x + 100.0)
}

/// A press and its lift at `x`: the keyboard the scene drawn with the press
/// says, or none when it went on with no drawing.
fn press(w: &mut World, x: f64, when: f64) -> Option<Keyboard> {
    let (shared, canvases, tools) = (&mut w.shared, &mut w.canvases, &mut w.tools);
    let pen = Event::Press(at(x, 300.0, when));
    let routed = route(pen, shared, canvases, tools, &w.settings, &w.socket, &mut w.sidebar);
    w.send(Event::Lift(at(x, 300.0, when + 0.1)));
    match routed {
        Routed::Answer(Step::Draw(scene) | Step::Through(scene)) => Some(scene.keyboard),
        _ => None,
    }
}

/// The keyboard the window's scene says now.
fn drawn(w: &mut World) -> Keyboard {
    view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases).keyboard
}

/// A paste that goes nowhere flashes why, where macOS would only beep
/// (slinger-acc9ee): an empty clipboard pasted on a canvas, and paste's key
/// with the window holding the keyboard over an agent UI (its Edit menu
/// item greyed, so only the key comes). Paste's key the page or an open
/// entry has says nothing: theirs.
#[test]
fn a_paste_that_goes_nowhere_flashes_why() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let (mut w, _, on_page) = beside(daemon, config.path());
        let flashed = |w: &mut World| {
            let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
            scene.flash.map(|pill| pill.label.text)
        };
        let paste = || Event::Key(Chord::new(CMD, 'v'));
        assert!(w.send(Event::Edit(Edit::Paste(Clip::default()))));
        assert_eq!(flashed(&mut w).as_deref(), Some("Nothing on the clipboard to paste"));

        w.tools.flash().fade(std::time::Instant::now() + std::time::Duration::from_secs(60));
        assert_eq!(press(&mut w, on_page, 1.0), Some(Keyboard::Pressed));
        assert!(!w.send(paste()), "the page's own");
        assert_eq!(flashed(&mut w), None);
        w.shared.keyboard = Keyboard::Window;
        assert!(w.send(paste()), "nothing of the app's takes it");
        assert_eq!(flashed(&mut w).as_deref(), Some("Nowhere to paste here"));

        w.tools.flash().fade(std::time::Instant::now() + std::time::Duration::from_secs(60));
        assert!(w.send(Event::Command(Command::LeaveFeedback)));
        assert!(!w.send(paste()), "the entry's own");
        assert_eq!(flashed(&mut w), None);
    });
}
