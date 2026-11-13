//! The window's routing, event by event as `platform` hands them in: a text
//! ended with escape, then a tool switch by its key, then a box drag
//! (slinger-a77a12).

use super::*;
use crate::backend::ConfigFiles;
use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_agents, with_socket, with_store};
use crate::commands::{CMD, Chord};
use crate::panes::{Kind as PaneKind, Pane};
use crate::scene::ink::Pen;
use crate::scene::paste::Edit;
use crate::scene::tools::Tool;

/// The window's state, as `window` keeps it, with no watch.
struct World {
    settings: Settings,
    shared: Shared,
    tools: Tools,
    sidebar: Sidebar,
    canvases: Canvases,
    socket: std::path::PathBuf,
}

impl World {
    /// Hands `event` in as the window's answer does ([`route`]); false for
    /// a pass. A tick takes only what the canvases' followers heard.
    fn send(&mut self, event: Event) -> bool {
        let w = self;
        let routed = route(
            event,
            &mut w.shared,
            &mut w.canvases,
            &mut w.tools,
            &w.settings,
            &w.socket,
            &mut w.sidebar,
        );
        match routed {
            Routed::Answer(step) | Routed::Watch(_, step) => !matches!(step, Step::Pass),
            Routed::News(_) => {
                w.canvases.poll();
                view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
                true
            }
        }
    }

    /// A key press as `platform` sends it: its chord, and when that passes,
    /// the text it types.
    fn key(&mut self, chord: Chord, text: &str) {
        if !self.send(Event::Key(chord)) && !text.is_empty() {
            assert!(self.send(Event::Text(text.into())), "{text:?} is typed");
        }
    }
}

fn at(x: f64, y: f64, when: f64) -> Pen {
    Pen { at: Point { x, y }, when, touch: false, eraser: false, shift: false }
}

fn count(daemon: &control::Client, kind: &str) -> usize {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().filter(|i| i["kind"] == kind).count()
}

/// A window onto `daemon`'s `notes` canvas, its page arrived, with the
/// default config.
fn world(daemon: &control::Client, config: &Path) -> World {
    let settings = Settings::load(ConfigFiles::at(config.join("config.toml")));
    let panes = Panes::new(Pane { kind: PaneKind::Canvas, name: "notes".into() });
    let mut w = World {
        tools: Tools::new(settings.config.palette.clone()),
        settings,
        shared: Shared { panes, ..Shared::default() },
        sidebar: Sidebar::new(false),
        canvases: Canvases::new(daemon.clone()),
        socket: "/nonexistent/slinger.sock".into(),
    };
    w.until(|w| w.canvases.page_names(&w.shared.panes, w.shared.panes.focus()).is_some());
    w
}

impl World {
    /// Ticks until `met`, as the window's timer does.
    fn until(&mut self, met: impl Fn(&World) -> bool) {
        wait_until(|| {
            self.send(Event::Tick);
            met(self)
        });
    }

    /// The text tool by its key (cmd+4), a click at `x`, then "word" typed.
    fn type_word(&mut self, x: f64, when: f64) {
        self.key(Chord::new(CMD, '4'), "4");
        assert_eq!(self.tools.tool(), Tool::Text);
        assert!(self.send(Event::Press(at(x, 300.0, when))));
        self.send(Event::Lift(at(x, 300.0, when + 0.1)));
        for c in ['w', 'o', 'r', 'd'] {
            self.key(Chord::new(0, c), &c.to_string());
        }
        assert!(self.canvases.editing());
    }

    /// The box tool by its key (cmd+5), then a drag from `x`: each event used.
    fn drag_box(&mut self, x: f64, when: f64) {
        self.key(Chord::new(CMD, '5'), "5");
        assert_eq!(self.tools.tool(), Tool::Box);
        assert!(self.send(Event::Press(at(x, 400.0, when))));
        assert!(self.send(Event::Drag(at(x + 50.0, 440.0, when + 0.1))));
        assert!(self.send(Event::Drag(at(x + 100.0, 480.0, when + 0.2))));
        assert!(self.send(Event::Lift(at(x + 100.0, 480.0, when + 0.3))));
    }
}

#[test]
fn escape_ends_a_text_and_the_first_box_drag_after_cmd_5_reaches_the_daemon() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.editing(), "escape ended the text");
        w.until(|_| count(daemon, "text") == 1);
        w.drag_box(400.0, 14.0);
        w.until(|_| count(daemon, "box") == 1);
    });
}

#[test]
fn a_text_left_open_ends_and_the_first_box_drag_after_cmd_5_reaches_the_daemon() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        w.drag_box(400.0, 14.0);
        assert!(!w.canvases.editing(), "the press ended the text");
        w.until(|_| count(daemon, "text") == 1 && count(daemon, "box") == 1);
    });
}

/// slinger-7d8ba2: a colour picked from the pop-up, by its digit, with a
/// box selected, recolours the box on the daemon.
#[test]
fn a_colour_picked_with_a_box_selected_recolours_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.drag_box(400.0, 10.0);
        w.until(|_| count(daemon, "box") == 1);
        assert!(w.send(Event::Command(Command::ToolSelect)));
        assert!(w.send(Event::Press(at(350.0, 350.0, 11.0))));
        assert!(w.send(Event::Drag(at(550.0, 500.0, 11.1))));
        assert!(w.send(Event::Lift(at(550.0, 500.0, 11.2))));
        assert!(w.canvases.selection().is_some(), "the select tool's box took it");
        let crate::config::Rgb { r, g, b } = w.settings.config.palette.colours()[1];
        let want = format!("#{r:02x}{g:02x}{b:02x}");
        assert!(w.send(Event::Command(Command::PickColour)));
        w.key(Chord::new(0, '2'), "2");
        w.until(|_| {
            let items = daemon.canvas_show("notes", None).unwrap().items;
            items.iter().any(|i| i["kind"] == "box" && i["color"] == want.as_str())
        });
    });
}

/// slinger-e8d72a: Edit > Undo or Redo chosen with the pointer while an
/// entry is open passes on to its field (as cmd+Z does), not to the canvas.
#[test]
fn undo_by_the_pointer_with_an_entry_open_is_the_field_s() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.drag_box(400.0, 10.0);
        w.until(|_| count(daemon, "box") == 1);
        assert!(w.send(Event::Command(Command::LeaveFeedback)));
        assert!(w.canvases.typing());
        assert!(!w.send(Event::Command(Command::Undo)), "the field's");
        assert!(!w.send(Event::Command(Command::Redo)), "the field's");
        assert!(!w.send(Event::Key(Chord::new(CMD, 'z'))), "as the key");
        assert!(w.canvases.typing(), "still open");
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.typing());
        assert!(w.send(Event::Command(Command::Undo)), "the canvas's");
        w.until(|_| count(daemon, "box") == 0);
    });
}

/// docs/design/pdf.md, "Snippets": the crop tool's lift answers the window
/// with the snippet to put on the clipboard and leaves the canvas as it
/// was; pasted, the snippet is one more `pdf` item, cropped.
#[test]
fn a_crop_s_lift_copies_a_snippet_that_pastes_as_a_cropped_pdf_item() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let hash = format!("sha256:{}", "9f".repeat(32));
        let item = serde_json::json!({"page": "p1", "kind": "pdf", "x": 0, "y": 0,
            "w": 12240, "h": 15840, "hash": hash, "pdfpage": 1, "pagesize": [6120, 7920]});
        let ops = serde_json::json!({"op": "add", "item": item});
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert_eq!(count(daemon, "pdf"), 1);
        w.key(Chord::new(CMD, '8'), "8");
        assert_eq!(w.tools.tool(), Tool::CropPdf);
        let mut route = |event| {
            let w = &mut w;
            let (shared, canvases, tools) = (&mut w.shared, &mut w.canvases, &mut w.tools);
            route(event, shared, canvases, tools, &w.settings, &w.socket, &mut w.sidebar)
        };
        assert!(matches!(
            route(Event::Press(at(300.0, 300.0, 1.0))),
            Routed::Answer(Step::Draw(_))
        ));
        route(Event::Drag(at(400.0, 350.0, 1.1)));
        let Routed::Answer(Step::Copy(clip, _)) = route(Event::Lift(at(500.0, 400.0, 1.2))) else {
            panic!("the lift copies")
        };
        let items: serde_json::Value =
            serde_json::from_str(clip.items.as_deref().unwrap()).unwrap();
        assert_eq!(items[0]["kind"], "pdf");
        assert_eq!((items[0]["w"].clone(), items[0]["h"].clone()), (2000.into(), 1000.into()));
        assert_eq!(items[0]["crop"][2], 1000, "two units a point: 200 units are 100 points");
        assert_eq!(count(daemon, "pdf"), 1, "the canvas does not change");
        assert!(w.send(Event::Edit(Edit::Paste(clip))));
        w.until(|_| count(daemon, "pdf") == 2);
    });
}

#[test]
fn open_workspace_opens_a_folder_and_shows_it_and_flashes_why_it_cannot() {
    with_socket(|socket| {
        let settings = Settings::load(ConfigFiles::at("/nonexistent/config.toml".into()));
        let mut tools = Tools::new(settings.config.palette.clone());
        let mut sidebar = Sidebar::new(true);
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("ideas");
        std::fs::create_dir(&dir).unwrap();
        let name = window::open_workspace(&dir, socket, &mut sidebar, &mut tools);
        assert_eq!(name.as_deref(), Some("ideas"), "named for its folder");
        assert!(dir.join("slinger-workspace.toml").is_file(), "its file is made");
        let daemon = control::Client::new(socket);
        let _notes = open_notes(&daemon);
        sidebar.update(backend::workspaces(&daemon));
        let rows = sidebar.rows(&settings.config.theme, layout::WINDOW.rect());
        let bold: Vec<_> = rows.iter().filter(|r| r.font.bold).map(|r| &r.label.text).collect();
        assert_eq!(bold, ["ideas"], "the window shows it, not the first");
        let gone = home.path().join("gone");
        assert_eq!(window::open_workspace(&gone, socket, &mut sidebar, &mut tools), None);
        let pill =
            tools.flash().pill(layout::WINDOW.rect(), &settings.config.theme, Instant::now());
        assert!(pill.is_some_and(|p| p.label.text.contains("gone")), "the flash says why");
    });
}

#[test]
fn a_broken_workspace_shows_why_and_close_workspace_closes_it_and_flashes_why_it_cannot() {
    with_socket(|socket| {
        let settings = Settings::load(ConfigFiles::at("/nonexistent/config.toml".into()));
        let (theme, window) = (&settings.config.theme, layout::WINDOW.rect());
        let mut tools = Tools::new(settings.config.palette.clone());
        let mut sidebar = Sidebar::new(true);
        let daemon = control::Client::new(socket);
        let notes = open_notes(&daemon);
        let file = notes.path().join("slinger-workspace.toml");
        let broken = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n\
                      [agent]\nname = \"scout\"\ntools = [\"Shell\"]\n";
        std::fs::write(&file, broken).unwrap();
        sidebar.update(backend::workspaces(&daemon));
        let rows = sidebar.rows(theme, window);
        let texts: Vec<_> = rows.iter().map(|r| r.label.text.as_str()).collect();
        assert!(texts.contains(&"notes") && texts.contains(&"field: agent.tools"), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("\"Shell\"")), "why: {texts:?}");
        assert!(!texts.contains(&"Close workspace"), "the panes do not show it: {texts:?}");
        sidebar.pick(0);
        let rows = sidebar.rows(theme, window);
        let texts: Vec<_> = rows.iter().map(|r| r.label.text.as_str()).collect();
        assert!(texts.contains(&"Close workspace"), "it shows: it can close {texts:?}");
        assert!(window::close_workspace(socket, &mut sidebar, &mut tools));
        assert_eq!(daemon.workspace_list().unwrap(), [], "closed by its folder");
        assert!(file.is_file(), "its file stays");

        let gone =
            scene::sidebar::Workspace { dir: "/nonexistent/gone".into(), ..Default::default() };
        sidebar.update(Ok(vec![gone]));
        assert!(!window::close_workspace(socket, &mut sidebar, &mut tools), "not open any more");
        let pill = tools.flash().pill(window, theme, Instant::now());
        assert!(pill.is_some_and(|p| !p.label.text.is_empty()), "the flash says why");
    });
}

/// The flash's text in `tools`, if one shows.
fn flashed(tools: &Tools, settings: &Settings) -> Option<String> {
    let pill = tools.flash().pill(layout::WINDOW.rect(), &settings.config.theme, Instant::now());
    pill.map(|p| p.label.text)
}

#[test]
fn start_stop_and_restart_agent_go_to_the_shown_workspace_s_agent_and_flash_or_say_why_not() {
    with_agents(|socket| {
        let settings = Settings::load(ConfigFiles::at("/nonexistent/config.toml".into()));
        let mut tools = Tools::new(settings.config.palette.clone());
        let mut sidebar = Sidebar::new(true);
        let daemon = control::Client::new(socket);
        let home = open_notes(&daemon);
        let lines = |sidebar: &mut Sidebar| {
            sidebar.update(backend::workspaces(&daemon));
            let rows = sidebar.rows(&settings.config.theme, layout::WINDOW.rect());
            rows.into_iter().map(|r| r.label.text).collect::<Vec<_>>()
        };
        let start = scene::sidebar::START.to_string();
        assert!(lines(&mut sidebar).contains(&start), "never started: it shows");
        window::agent(Command::StartAgent, socket, &sidebar, &mut tools);
        assert_eq!(flashed(&tools, &settings).as_deref(), Some("Start agent · scout"));
        assert!(!lines(&mut sidebar).contains(&start), "it runs");
        let runs = || std::fs::read_to_string(home.path().join("args.txt")).unwrap_or_default();
        // A new conversation the first time.
        wait_until(|| runs().contains("--session-id"));
        window::agent(Command::StartAgent, socket, &sidebar, &mut tools);
        assert_eq!(runs().lines().count(), 1, "one that runs stays as it is");
        // Its answer names its conversation, which the daemon keeps.
        daemon.session_send("notes", "hi").unwrap();
        wait_until(|| daemon.session_list().unwrap()[0].session_id.is_some());
        window::agent(Command::StopAgent, socket, &sidebar, &mut tools);
        assert_eq!(flashed(&tools, &settings).as_deref(), Some("Stop agent · scout"));
        assert!(lines(&mut sidebar).contains(&start), "it stopped: it shows again");
        window::agent(Command::StartAgent, socket, &sidebar, &mut tools);
        wait_until(|| runs().contains("--resume"));
        assert_eq!(runs().lines().count(), 2, "in its last conversation");
        window::agent(Command::RestartAgent, socket, &sidebar, &mut tools);
        let restarted = flashed(&tools, &settings).unwrap_or_default();
        assert_eq!(restarted, "Restart agent in a new conversation · scout");
        wait_until(|| runs().lines().count() == 3);
        let last = runs().lines().last().unwrap_or_default().to_string();
        assert!(last.contains("--session-id"), "a new conversation: {last}");
        window::agent(Command::StopAgent, socket, &sidebar, &mut tools);

        daemon.workspace_close("notes").unwrap();
        window::agent(Command::StartAgent, socket, &sidebar, &mut tools);
        let why = flashed(&tools, &settings).unwrap_or_default();
        assert!(!why.is_empty() && !why.contains("scout"), "the flash says why: {why}");
    });
}

#[expect(clippy::unnecessary_wraps, reason = "it stands in for platform::pdf::pages")]
fn one_page(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    Ok(vec![(612.0, 792.0)])
}

/// slinger-fd53aa: files dropped on the window open their first PDF as
/// `open-pdf` does, beside the pane under the drop, which takes focus; a
/// drop with no PDF is refused.
#[test]
fn a_pdf_dropped_on_a_pane_opens_as_a_canvas_beside_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.shared.panes.split(layout::Axis::Row);
        let area = w.sidebar.panes(w.shared.size.rect());
        let (left, right) = (w.shared.panes.place(area)[0].0, w.shared.panes.focus());
        let frame = w.shared.panes.place(area)[0].2;
        let at = Point { x: frame.x + frame.width / 2.0, y: frame.y + frame.height / 2.0 };
        assert!(!w.send(Event::Dropped(at, vec!["notes.txt".into()])), "no PDF: refused");
        assert_eq!(w.shared.panes.focus(), right, "and focus stays");

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Paper.PDF");
        std::fs::write(&file, b"%PDF-1.7 one page").unwrap();
        let files = [dir.path().join("notes.txt"), file];
        let (shared, canvases) = (&mut w.shared, &mut w.canvases);
        assert!(window::open_pdf(&files, Some(at), shared, canvases, area, one_page));
        assert_eq!(w.shared.panes.focus(), left, "the pane under the drop took focus");
        wait_until(|| {
            w.canvases.poll();
            daemon.canvas_list().unwrap().contains(&"paper.pdf".to_string())
        });
    });
}
