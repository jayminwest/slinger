//! The app's IO: the one module that talks to the daemon (docs/service.md)
//! and reads the config files. It turns their errors into what the window can
//! show. Everything here blocks. [`Settings::refresh`] is cheap when nothing
//! changed (two `stat` calls), so the window's timer calls it; a daemon round
//! trip waits up to the client's timeout, so the app asks only at start and
//! when the socket changes.
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use slinger_control::{self as control, Client};

use crate::config::{self, Config, Layer};

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

/// Blocks until the daemon answers or its client gives up.
pub fn greeting(daemon: &Client, name: &str) -> Result<String, Problem> {
    daemon.hello(name).map_err(|e| Problem::new(e.to_string(), e.hint()))
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

    /// Reads the files again if either changed since the last read; returns
    /// whether it did. Call it every so often to follow edits.
    pub fn refresh(&mut self) -> bool {
        let changed = self.files.stamp() != self.seen;
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
mod tests {
    use slingerd as daemon;

    use super::*;

    /// Runs `f` against a daemon that lives for this one call.
    fn with_daemon<T>(f: impl FnOnce(&Client) -> T) -> T {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let socket = std::env::temp_dir().join(format!("a-{}-{n}.sock", std::process::id()));
        let data = tempfile::tempdir().unwrap();
        let store = daemon::Store::open(data.path(), "my-laptop").unwrap();
        let server = daemon::Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        let serving = std::thread::spawn(move || server.serve(&store));
        let result = f(&Client::new(&socket));
        stopper.stop();
        serving.join().unwrap().unwrap();
        std::fs::remove_file(format!("{}.lock", socket.display())).unwrap();
        result
    }

    #[test]
    fn returns_the_daemons_greeting() {
        assert_eq!(with_daemon(|d| greeting(d, "ada")), Ok("hello, ada".to_string()));
    }

    #[test]
    fn a_daemon_error_becomes_a_problem_with_its_hint() {
        let problem = with_daemon(|d| greeting(d, " ")).unwrap_err();
        assert_eq!(problem.message, "name is empty");
        assert!(problem.hint.is_some());
    }

    #[test]
    fn no_daemon_is_a_problem_that_says_how_to_start_it() {
        let problem = greeting(&Client::new("/nonexistent/x.sock"), "ada").unwrap_err();
        assert!(problem.hint.unwrap().starts_with("start it with '"));
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
        assert!(!settings.refresh(), "nothing changed");
        let good = settings.config.clone();

        fs::write(&file, "[theme]\n\ntext = \"#12345\"\n").unwrap();
        assert!(settings.refresh());
        assert_eq!(settings.config, good);
        let problem = settings.problem.clone().unwrap();
        assert!(problem.message.ends_with("config.toml: line 3: the value is not a colour"));
        assert_eq!(problem.hint.as_deref(), Some("write a colour as \"#rrggbb\""));

        fs::write(&file, "[theme]\ntext = \"#123456\"\n# fixed\n").unwrap();
        assert!(settings.refresh());
        assert_eq!(settings.problem, None);
        assert_eq!(settings.config.theme.text, config::Rgb { r: 0x12, g: 0x34, b: 0x56 });
    }

    #[test]
    fn an_overlay_that_appears_or_goes_away_is_picked_up() {
        let dir = config_dir();
        fs::write(dir.join("config.toml"), TEXT_1).unwrap();
        let mut settings = Settings::load(ConfigFiles::at(dir.join("config.toml")));
        fs::write(dir.join("config.local.toml"), "[daemon]\nsocket = \"/x.sock\"\n").unwrap();
        assert!(settings.refresh());
        assert_eq!(settings.socket(), PathBuf::from("/x.sock"));
        fs::remove_file(dir.join("config.local.toml")).unwrap();
        assert!(settings.refresh());
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
