//! The app's IO: the one module that talks to the daemon (docs/service.md)
//! and reads the config files. It turns their errors into what the window can
//! show. [`Settings::refresh`] is cheap when nothing changed (two `stat`
//! calls), so the window's timer calls it. A daemon round trip waits up to
//! the client's timeout, so a [`Watch`] asks for the agents' statuses and
//! the workspace's panes, and the code of each agent UI they show
//! ([`ui`]), on its own thread; the window sends only its own pane changes
//! ([`panes`]). A canvas is followed on a thread of its own ([`Follower`]),
//! the pen's strokes are sent on another ([`Outbox`]) and kept on disk until
//! sent ([`Unsent`]), slings on a third ([`Slinger`]) and notes of feedback
//! on their own ([`Noter`]). It also asks Flick to dictate ([`Flick`]), on a fourth.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use slinger_control::{self as control, Client, PaneKind, PaneLayout, RemoteError};

use crate::config::{self, Config, Layer};
use crate::scene::sidebar::Workspace;
use crate::ui::Shown;

mod agents;
mod feedback;
mod flick;
mod follow;
mod outbox;
pub mod panes;
pub mod pdf;
mod sling;
#[cfg(test)]
pub(crate) mod testing;
pub mod ui;
mod unsent;

pub use agents::{close_workspace, open_workspace, start_agent, stop_agent, workspaces};
pub use feedback::{Noter, feedback_file};
pub use flick::Flick;
pub use follow::{Follower, News};
pub use outbox::{Outbox, run};
pub use sling::{Heard, Render, Slinger};
pub use unsent::Unsent;

/// A failure as the window shows it: the message and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub hint: Option<String>,
}

impl Problem {
    fn new(message: String, hint: Option<&str>) -> Self {
        Self { message, hint: hint.map(str::to_string) }
    }
}

impl From<RemoteError> for Problem {
    fn from(e: RemoteError) -> Self {
        Problem::new(e.to_string(), e.hint())
    }
}

/// How often a [`Watch`] asks the daemon.
const EVERY: Duration = Duration::from_secs(1);

/// Asks the daemon for [`workspaces`] and for the panes
/// ([`panes::layout`]) every second on a thread of its own, so a slow
/// daemon never holds up the window. Each time the layout changes it
/// fetches the code of every agent UI the panes show ([`ui::fetch`]) before
/// it tells of the layout. The window takes the answers with
/// [`Watch::latest`], [`Watch::layout`] and [`Watch::uis`].
#[derive(Debug)]
pub struct Watch {
    socket: Arc<Mutex<PathBuf>>,
    wanted: Arc<Mutex<Option<String>>>,
    news: Receiver<Result<Vec<Workspace>, Problem>>,
    layouts: Receiver<Result<PaneLayout, Problem>>,
    codes: Receiver<Vec<(String, Result<Shown, Problem>)>>,
}

/// What each agent UI shown in `layout` is: its name and its code.
fn load(daemon: &Client, layout: &PaneLayout) -> Vec<(String, Result<Shown, Problem>)> {
    let mut names: Vec<&str> = layout
        .layout
        .panes()
        .into_iter()
        .filter(|p| p.kind == PaneKind::Ui)
        .map(|p| p.target.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    names.into_iter().map(|name| (name.to_string(), ui::fetch(daemon, name))).collect()
}

impl Watch {
    /// Starts asking the daemon on `socket`. The thread ends after the
    /// watch is dropped, at its next answer that differs from the last.
    /// The panes are those of `wanted`, else of `last` (see [`panes::layout`]).
    pub fn start(socket: PathBuf, wanted: Option<String>, last: Option<String>) -> Self {
        let socket = Arc::new(Mutex::new(socket));
        let wanted = Arc::new(Mutex::new(wanted));
        let (agents, news) = mpsc::channel();
        let (panes, layouts) = mpsc::channel();
        let (uis, codes) = mpsc::channel();
        let (theirs, their_wanted) = (Arc::clone(&socket), Arc::clone(&wanted));
        std::thread::spawn(move || {
            let (mut heard, mut drawn) = (None, None);
            loop {
                let daemon =
                    Client::new(theirs.lock().unwrap_or_else(PoisonError::into_inner).clone());
                let wanted = their_wanted.lock().unwrap_or_else(PoisonError::into_inner).clone();
                let layout = panes::layout(&daemon, wanted.as_deref(), last.as_deref());
                let fresh = drawn.as_ref() != Some(&layout);
                let loaded = match &layout {
                    Ok(now) if fresh => uis.send(load(&daemon, now)).is_ok(),
                    _ => true,
                };
                if !loaded
                    || !tell(&agents, &mut heard, workspaces(&daemon))
                    || !tell(&panes, &mut drawn, layout)
                {
                    return;
                }
                std::thread::sleep(EVERY);
            }
        });
        Self { socket, wanted, news, layouts, codes }
    }

    /// Asks for the panes of `wanted` from the next time on.
    pub fn want(&self, wanted: Option<String>) {
        *self.wanted.lock().unwrap_or_else(PoisonError::into_inner) = wanted;
    }

    /// The newest layout since the last call, if it changed. Never blocks.
    pub fn layout(&self) -> Option<Result<PaneLayout, Problem>> {
        self.layouts.try_iter().last()
    }

    /// The code of the agent UIs fetched since the last call, by name. A
    /// layout's UIs come before the layout. Never blocks.
    pub fn uis(&self) -> Vec<(String, Result<Shown, Problem>)> {
        self.codes.try_iter().flatten().collect()
    }

    /// Asks the daemon on `socket` from the next time on.
    pub fn follow(&self, socket: PathBuf) {
        *self.socket.lock().unwrap_or_else(PoisonError::into_inner) = socket;
    }

    /// The newest answer since the last call, if it changed. Never blocks.
    pub fn latest(&self) -> Option<Result<Vec<Workspace>, Problem>> {
        self.news.try_iter().last()
    }
}

/// Sends `now` when it differs from `last`; false once nobody listens.
fn tell<T: Clone + PartialEq>(to: &Sender<T>, last: &mut Option<T>, now: T) -> bool {
    if last.as_ref() == Some(&now) {
        return true;
    }
    *last = Some(now.clone());
    to.send(now).is_ok()
}

/// Where the config lives: the file shared by every machine, and the
/// machine's overlay next to it (`config.toml` and `config.local.toml`).
#[derive(Debug, Clone)]
pub struct ConfigFiles {
    base: PathBuf,
    overlay: PathBuf,
}

impl ConfigFiles {
    /// `$SLINGER_CONFIG`, else `$XDG_CONFIG_HOME/slinger/config.toml`, else
    /// `~/.config/slinger/config.toml`.
    pub fn locate() -> Self {
        let base = env_path("SLINGER_CONFIG").unwrap_or_else(|| {
            let dir =
                env_path("XDG_CONFIG_HOME").or_else(|| Some(env_path("HOME")?.join(".config")));
            dir.unwrap_or_default().join("slinger").join("config.toml")
        });
        Self::at(base)
    }

    /// The files for the shared file `base`: its overlay is `<name>.local.toml`.
    pub fn at(base: PathBuf) -> Self {
        let overlay = base.with_extension("local.toml");
        Self { base, overlay }
    }

    fn stamp(&self) -> Stamp {
        let stamp = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
        [stamp(&self.base), stamp(&self.overlay)]
    }

    /// The overlay merged over the shared file. A missing file sets nothing.
    fn read(&self) -> Result<Config, Problem> {
        Ok(layer(&self.base)?.over(layer(&self.overlay)?).resolve())
    }
}

/// When each file last changed; `None` while it is missing.
type Stamp = [Option<SystemTime>; 2];

fn layer(path: &Path) -> Result<Layer, Problem> {
    let problem = |e: &dyn std::fmt::Display, hint| {
        Problem::new(format!("{}: {e}", path.display()), Some(hint))
    };
    match fs::read_to_string(path) {
        Ok(text) => config::parse(&text).map_err(|e| problem(&e, e.hint())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Layer::default()),
        Err(e) => Err(problem(&e, "make it a readable text file, or remove it")),
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The live config: the last good one, and the problem with the files as
/// they are now, if any. A bad edit never takes the app down.
#[derive(Debug)]
pub struct Settings {
    files: ConfigFiles,
    seen: Stamp,
    pub config: Config,
    pub problem: Option<Problem>,
}

impl Settings {
    /// Reads `files`. With a problem, the config is the defaults.
    pub fn load(files: ConfigFiles) -> Self {
        let mut settings =
            Self { files, seen: [None, None], config: Config::default(), problem: None };
        settings.reload();
        settings
    }

    /// Reads the files again if either changed since the last read, or if
    /// `force` (the `reload-config` command); returns whether it did. Call it
    /// every so often to follow edits.
    pub fn refresh(&mut self, force: bool) -> bool {
        let changed = force || self.files.stamp() != self.seen;
        if changed {
            self.reload();
        }
        changed
    }

    fn reload(&mut self) {
        // Stamp first: an edit that lands during the read shows on the next refresh.
        self.seen = self.files.stamp();
        match self.files.read() {
            Ok(config) => (self.config, self.problem) = (config, None),
            Err(problem) => self.problem = Some(problem),
        }
    }

    /// The daemon's socket: the config's, else the daemon's default.
    pub fn socket(&self) -> PathBuf {
        match self.config.socket.as_deref() {
            Some(socket) => config::expand_home(socket, env_path("HOME").as_deref()),
            None => control::default_socket(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use slingerd as daemon;

    use super::*;
    use crate::scene::sidebar::{Agent, Status};

    /// Runs `f` against a daemon that lives for this one call.
    pub(crate) fn with_daemon<T>(f: impl FnOnce(&Client) -> T) -> T {
        with_socket(|socket| f(&Client::new(socket)))
    }

    /// Runs `f` against a daemon that lives for this one call and starts
    /// agents with `claude`, a stand-in for Claude Code (none: no program).
    pub(crate) fn with_store<T>(claude: Option<&Path>, f: impl FnOnce(&Client) -> T) -> T {
        serve(claude.unwrap_or(Path::new("/nonexistent/claude")), |socket| f(&Client::new(socket)))
    }

    /// Runs `f` with the socket of a daemon that lives for this one call.
    pub(crate) fn with_socket<T>(f: impl FnOnce(&Path) -> T) -> T {
        serve(Path::new("/nonexistent/claude"), f)
    }

    /// Runs `f` with the socket of a daemon that lives for this one call and
    /// starts agents with the daemon's kept stand-in for Claude Code
    /// (`crates/daemon/src/tests/claude`, which says what it does).
    pub(crate) fn with_agents<T>(f: impl FnOnce(&Path) -> T) -> T {
        serve(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../daemon/src/tests/claude"), f)
    }

    /// The socket is in a new directory of its own, removed on drop. Short:
    /// socket paths have a length limit.
    fn serve<T>(claude: &Path, f: impl FnOnce(&Path) -> T) -> T {
        let sockets = tempfile::tempdir().unwrap();
        let socket = sockets.path().join("d.sock");
        let data = tempfile::tempdir().unwrap();
        let store = daemon::Store::open(data.path(), "my-laptop").unwrap().with_claude(claude);
        let server = daemon::Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        let serving = std::thread::spawn(move || server.serve(&store));
        let result = f(&socket);
        stopper.stop();
        serving.join().unwrap().unwrap();
        result
    }

    /// Opens a workspace `notes` whose agent is `scout`, in a fresh directory.
    pub(crate) fn open_notes(daemon: &Client) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let file = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n[agent]\nname = \"scout\"\n";
        fs::write(dir.path().join("slinger-workspace.toml"), file).unwrap();
        daemon.workspace_open(Some(dir.path()), None).unwrap();
        dir
    }

    /// The workspace `notes` that `d` holds open, as the sidebar shows it.
    fn notes(d: &Client, status: Option<Status>, note: Option<&str>) -> Vec<Workspace> {
        let note = note.map(str::to_string);
        let dir = d.workspace_list().unwrap().remove(0).dir;
        let agent = Agent { name: "scout".into(), status, note, ..Agent::default() };
        vec![Workspace {
            name: "notes".into(),
            canvas: "notes".into(),
            agent: Some(agent),
            dir,
            broken: None,
        }]
    }

    #[test]
    fn lists_the_workspaces_with_their_agents_status() {
        with_daemon(|d| {
            assert_eq!(workspaces(d), Ok(vec![]));
            let _dir = open_notes(d);
            assert_eq!(workspaces(d), Ok(notes(d, Some(Status::Stopped), None)), "no process");
            d.status_set("scout", "blocked", Some("which key?")).unwrap();
            let hooks = notes(d, Some(Status::Blocked), Some("which key?"));
            assert_eq!(workspaces(d), Ok(hooks), "never started: its hooks say");
        });
    }

    /// The watch's answers until one is `want`, or a failure after 10 s.
    fn until(watch: &Watch, want: &Result<Vec<Workspace>, Problem>) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while watch.latest().as_ref() != Some(want) {
            assert!(std::time::Instant::now() < deadline, "timed out waiting for {want:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_watch_hears_a_status_change_and_follows_a_new_socket() {
        let watch = Watch::start(PathBuf::from("/nonexistent/x.sock"), None, None);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let lost = loop {
            if let Some(answer) = watch.latest() {
                break answer;
            }
            assert!(std::time::Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(lost.unwrap_err().hint.unwrap().starts_with("start it with '"));
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            watch.follow(socket.to_path_buf());
            until(&watch, &Ok(notes(d, Some(Status::Stopped), None)));
            d.status_set("scout", "blocked", Some("which key?")).unwrap();
            until(&watch, &Ok(notes(d, Some(Status::Blocked), Some("which key?"))));
        });
    }

    #[test]
    fn a_watch_hears_a_pane_opened_elsewhere_in_the_workspace_it_wants() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            let watch = Watch::start(socket.to_path_buf(), Some("nope".into()), None);
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let count = || {
                assert!(std::time::Instant::now() < deadline, "timed out");
                std::thread::sleep(Duration::from_millis(20));
                watch.layout().map(|l| l.map(|l| l.layout.panes().len()).map_err(|p| p.message))
            };
            while count() != Some(Err("open workspace \"nope\" not found".into())) {}
            watch.want(Some("notes".into()));
            while count() != Some(Ok(1)) {}
            d.pane_open("notes", control::PaneKind::Canvas, "notes", None, false).unwrap();
            while count() != Some(Ok(2)) {}
        });
    }

    /// A fresh, empty directory for one test's config files.
    fn config_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("slinger-app-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const TEXT_1: &str = "[theme]\ntext = \"#111111\"\n";

    #[test]
    fn no_files_is_every_default() {
        let settings = Settings::load(ConfigFiles::at(config_dir().join("config.toml")));
        assert_eq!((settings.config, settings.problem), (Config::default(), None));
    }

    #[test]
    fn the_overlay_is_merged_over_the_shared_file() {
        let dir = config_dir();
        fs::write(dir.join("config.toml"), "[theme]\ntext = \"#111111\"\nmuted = \"#222222\"\n")
            .unwrap();
        fs::write(dir.join("config.local.toml"), "[theme]\nmuted = \"#333333\"\n").unwrap();
        let settings = Settings::load(ConfigFiles::at(dir.join("config.toml")));
        assert_eq!(settings.config.theme.text, config::Rgb { r: 0x11, g: 0x11, b: 0x11 });
        assert_eq!(settings.config.theme.muted, config::Rgb { r: 0x33, g: 0x33, b: 0x33 });
        assert_eq!(settings.problem, None);
    }

    #[test]
    fn an_edit_is_picked_up_and_a_bad_edit_keeps_the_last_good_config() {
        let dir = config_dir();
        let file = dir.join("config.toml");
        fs::write(&file, TEXT_1).unwrap();
        let mut settings = Settings::load(ConfigFiles::at(file.clone()));
        assert!(!settings.refresh(false), "nothing changed");
        assert!(settings.refresh(true), "forced");
        let good = settings.config.clone();

        fs::write(&file, "[theme]\n\ntext = \"#12345\"\n").unwrap();
        assert!(settings.refresh(false));
        assert_eq!(settings.config, good);
        let problem = settings.problem.clone().unwrap();
        assert!(problem.message.ends_with("config.toml: line 3: the value is not a colour"));
        assert_eq!(problem.hint.as_deref(), Some("write a colour as \"#rrggbb\""));

        fs::write(&file, "[theme]\ntext = \"#123456\"\n# fixed\n").unwrap();
        assert!(settings.refresh(false));
        assert_eq!(settings.problem, None);
        assert_eq!(settings.config.theme.text, config::Rgb { r: 0x12, g: 0x34, b: 0x56 });
    }

    #[test]
    fn an_overlay_that_appears_or_goes_away_is_picked_up() {
        let dir = config_dir();
        fs::write(dir.join("config.toml"), TEXT_1).unwrap();
        let mut settings = Settings::load(ConfigFiles::at(dir.join("config.toml")));
        fs::write(dir.join("config.local.toml"), "[daemon]\nsocket = \"/x.sock\"\n").unwrap();
        assert!(settings.refresh(false));
        assert_eq!(settings.socket(), PathBuf::from("/x.sock"));
        fs::remove_file(dir.join("config.local.toml")).unwrap();
        assert!(settings.refresh(false));
        assert_eq!(settings.socket(), control::default_socket());
    }

    #[test]
    fn a_file_that_cannot_be_read_is_a_problem_at_start() {
        let dir = config_dir();
        fs::create_dir(dir.join("config.toml")).unwrap();
        let settings = Settings::load(ConfigFiles::at(dir.join("config.toml")));
        assert_eq!(settings.config, Config::default());
        let problem = settings.problem.unwrap();
        assert!(problem.message.starts_with(&format!("{}: ", dir.join("config.toml").display())));
        assert!(problem.hint.is_some());
    }

    #[test]
    fn the_overlay_sits_next_to_the_shared_file() {
        let files = ConfigFiles::at(PathBuf::from("/c/slinger/config.toml"));
        assert_eq!(files.overlay, PathBuf::from("/c/slinger/config.local.toml"));
        assert!(
            ConfigFiles::locate().base.ends_with("config.toml")
                || std::env::var_os("SLINGER_CONFIG").is_some()
        );
    }
}
