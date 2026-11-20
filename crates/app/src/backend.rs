//! The app's IO: the one module that talks to the daemon (docs/service.md)
//! and reads the config files. It turns their errors into what the window can
//! show. [`Settings::refresh`] is cheap when nothing changed (three `stat`
//! calls: the config files and the daemon's name for its socket, [`place`]),
//! so the window's timer calls it. A daemon round trip waits up to
//! the client's timeout, so a [`Watch`] ([`watch`]) asks for the agents' statuses and
//! the workspace's panes, and the code of each agent UI they show
//! ([`ui`]), on its own thread; the window sends only its own pane changes
//! ([`panes`]). A canvas is followed on a thread of its own ([`Follower`]),
//! the pen's strokes are sent on another ([`Outbox`]) and kept on disk until
//! sent ([`Unsent`]), slings on a third ([`Slinger`]) and notes of feedback
//! on their own ([`Noter`]). It also asks Flick to dictate ([`Flick`]), on a fourth.
//! A bundled app keeps its daemon running from a [`Launcher`] ([`daemon`]).
//! Each of these threads hands its news on through a [`Teller`], which wakes
//! the main thread to take it ([`wake`]): news never waits for the timer.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use slinger_control::{self as control, RemoteError};

use crate::config::{self, Config, Layer};

mod agents;
pub mod daemon;
mod feedback;
mod flick;
mod follow;
mod outbox;
pub mod panes;
pub mod pdf;
mod place;
mod sling;
#[cfg(test)]
pub(crate) mod testing;
pub mod ui;
mod unsent;
pub mod wake;
mod watch;

pub use agents::{close_workspace, open_workspace, start_agent, stop_agent, workspaces};
pub use daemon::Launcher;
pub use feedback::{Noter, feedback_file};
pub use flick::Flick;
pub use follow::{Follower, News};
pub use outbox::{Outbox, run};
pub use sling::{Heard, Render, Slinger};
pub use unsent::Unsent;
pub use wake::{Teller, Wake};
pub use watch::Watch;

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

/// Where the config lives: the file shared by every machine, and the
/// machine's overlay next to it (`config.toml` and `config.local.toml`); and
/// the data directory whose daemon the app finds when the config names no
/// socket ([`place`]).
#[derive(Debug, Clone)]
pub struct ConfigFiles {
    base: PathBuf,
    overlay: PathBuf,
    data_dir: Option<PathBuf>,
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
        Self { data_dir: control::default_data_dir(), ..Self::at(base) }
    }

    /// The files for the shared file `base`: its overlay is
    /// `<name>.local.toml`. No data directory: no daemon names its socket.
    pub fn at(base: PathBuf) -> Self {
        let overlay = base.with_extension("local.toml");
        Self { base, overlay, data_dir: None }
    }

    fn stamp(&self) -> Stamp {
        let stamp = |path: &Path| fs::metadata(path).and_then(|m| m.modified()).ok();
        let named = self.data_dir.as_deref().map(control::socket_name_file);
        [stamp(&self.base), stamp(&self.overlay), named.as_deref().and_then(stamp)]
    }

    /// The overlay merged over the shared file. A missing file sets nothing.
    fn read(&self) -> Result<Config, Problem> {
        Ok(layer(&self.base)?.over(layer(&self.overlay)?).resolve())
    }
}

/// When each file last changed; `None` while it is missing.
type Stamp = [Option<SystemTime>; 3];

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
            Self { files, seen: [None; 3], config: Config::default(), problem: None };
        settings.reload();
        settings
    }

    /// Reads the files again if any changed since the last read, or if
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

    /// The daemon's socket: the config's, else the one the running daemon
    /// named, else the daemon's default ([`place::socket`]).
    pub fn socket(&self) -> PathBuf {
        let home = env_path("HOME");
        let set = self.config.socket.as_deref().map(|s| config::expand_home(s, home.as_deref()));
        place::socket(set, self.files.data_dir.as_deref())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use slingerd as daemon;

    use slinger_control::Client;

    use super::*;
    use crate::scene::sidebar::{Agent, Status, Workspace};

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
    pub(crate) fn notes(d: &Client, status: Option<Status>, note: Option<&str>) -> Vec<Workspace> {
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

    /// A fresh, empty directory for one test's config files, removed on drop
    /// even when the test fails (a named one was never removed, slinger-ca6e2f).
    fn config_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    const TEXT_1: &str = "[theme]\ntext = \"#111111\"\n";

    #[test]
    fn no_files_is_every_default() {
        let settings = Settings::load(ConfigFiles::at(config_dir().path().join("config.toml")));
        assert_eq!((settings.config, settings.problem), (Config::default(), None));
    }

    #[test]
    fn the_overlay_is_merged_over_the_shared_file() {
        let dir = config_dir();
        let dir = dir.path();
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
        let dir = dir.path();
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
        let dir = dir.path();
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
        let dir = dir.path();
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
