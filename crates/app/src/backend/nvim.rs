//! The Nvim a canvas text is edited by, as a child process (`nvim --embed`):
//! its stdin takes the client's messages, and a thread reads its stdout
//! into values, each stamped with when it came, and wakes the main thread.
//! One process per text edited (docs/design/writing-column.md, 6A): it
//! starts when the text opens and is killed, by its own pid, when the text
//! ends.
//!
//! Which Nvim is [`Setup::resolve`]'s: `[editor] nvim`, else the first on
//! the app's `PATH` or in [`PLACES`]. Nothing of the user's is written:
//! no swap file (`-n`), no shada (`-i NONE`), a scratch buffer (no undo
//! file, no autowrite), and `XDG_STATE_HOME` and `XDG_CACHE_HOME` in a
//! directory of slinger's from the first start, so the logs (`lsp.log`,
//! Nvim's own), undo and plugin caches never land in the user's. Config and
//! data stay the user's, so their plugins load.
//!
//! `SLINGER_NVIM` is for tests and drives: `clean` starts Nvim with
//! `--clean`, `user` with the user's config, `off` with none, whatever
//! the config says. `SLINGER_NVIM_CLIP` names a file Nvim's clipboard reads
//! and writes in place of the system clipboard (a driven window never
//! touches the user's). `SLINGER_NVIM_TRACE` names a file the timings go to.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use super::Wake;
use crate::config::{Nvim, expand_home};
use crate::nvim::pack::{Short, Value, decode};

/// Where `auto` looks after the app's `PATH`: an app opened from the
/// Finder has launchd's short `PATH`. `~/` is the user's home.
pub const PLACES: [&str; 5] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "~/.nix-profile/bin",
    "/etc/profiles/per-user/$USER/bin",
    "/run/current-system/sw/bin",
];

/// How an Nvim is started.
#[derive(Debug, Clone)]
pub struct Setup {
    program: PathBuf,
    clean: bool,
    clip: Option<PathBuf>,
    /// Where the timings go, if anywhere.
    trace: Option<PathBuf>,
    /// Nvim's working directory, with its state and cache under it.
    scratch: PathBuf,
    /// More of the environment: a test's own config and data homes.
    homes: Vec<(&'static str, PathBuf)>,
}

impl Setup {
    /// The Nvim `nvim` (the config's) and the environment `env` ask for:
    /// `Ok(None)` when it is off. Without `by_config` (a driven window, a
    /// test) only `SLINGER_NVIM` turns it on.
    ///
    /// # Errors
    ///
    /// No Nvim is where it was asked for, or anywhere `auto` looks: what
    /// the flash says.
    pub fn resolve(
        nvim: &Nvim,
        by_config: bool,
        env: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Setup>, String> {
        let var = |name: &str| env(name).filter(|v| !v.is_empty());
        let clean = match var("SLINGER_NVIM").as_ref().and_then(|v| v.to_str()) {
            Some("clean") => true,
            Some("user") => false,
            Some("off") => return Ok(None),
            _ if !by_config || *nvim == Nvim::Off => return Ok(None),
            _ => false,
        };
        let home = var("HOME").map(PathBuf::from);
        let program = match nvim {
            Nvim::Path(path) => {
                let path = expand_home(path, home.as_deref());
                if !runs(&path) {
                    return Err(format!("no nvim at {}", path.display()));
                }
                path
            }
            Nvim::Auto | Nvim::Off => {
                let user =
                    var("USER").map(|u| u.to_string_lossy().into_owned()).unwrap_or_default();
                find(var("PATH").as_deref(), home.as_deref(), &user).ok_or("nvim not found")?
            }
        };
        let path = |name: &str| var(name).map(PathBuf::from);
        Ok(Some(Setup {
            program,
            clean,
            clip: path("SLINGER_NVIM_CLIP"),
            trace: path("SLINGER_NVIM_TRACE"),
            scratch: std::env::temp_dir().join("slinger-nvim"),
            homes: Vec::new(),
        }))
    }

    /// `program` with no config (`--clean`), in `dir`: Nvim's config,
    /// data, state and cache homes are all under it (a test's).
    #[cfg(test)]
    pub fn clean_in(program: PathBuf, dir: &Path) -> Setup {
        let homes =
            vec![("XDG_CONFIG_HOME", dir.join("config")), ("XDG_DATA_HOME", dir.join("data"))];
        Setup { program, clean: true, clip: None, trace: None, scratch: dir.to_path_buf(), homes }
    }
}

/// The first `nvim` that runs in `path` (the `PATH`), then in [`PLACES`].
pub fn find(path: Option<&std::ffi::OsStr>, home: Option<&Path>, user: &str) -> Option<PathBuf> {
    let listed = path.map(|p| std::env::split_paths(p).collect::<Vec<_>>()).unwrap_or_default();
    let places = PLACES.iter().map(|p| expand_home(&p.replace("$USER", user), home));
    listed.into_iter().chain(places).map(|dir| dir.join("nvim")).find(|p| runs(p))
}

/// Whether `path` is a file anyone may run.
fn runs(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// A running `nvim --embed`.
pub struct Process {
    child: Option<Child>,
    stdin: ChildStdin,
    heard: Receiver<(Value, Instant)>,
    /// Where the timings go, if anywhere (`SLINGER_NVIM_TRACE`).
    trace: Option<std::fs::File>,
}

impl Process {
    /// Starts Nvim as `setup` says; its messages wake the main thread with `wake`.
    ///
    /// # Errors
    ///
    /// Nvim cannot start, or its scratch directory cannot be made.
    pub fn start(setup: &Setup, wake: &Wake) -> std::io::Result<Process> {
        let (state, cache) = (setup.scratch.join("state"), setup.scratch.join("cache"));
        for dir in [&state, &cache] {
            std::fs::create_dir_all(dir)?;
        }
        let mut command = Command::new(&setup.program);
        command.args(["--embed", "-n", "-i", "NONE"]);
        if setup.clean {
            command.arg("--clean");
        }
        if let Some(clip) = &setup.clip {
            let file = clip.display().to_string().replace('\'', "''");
            let copy = format!("['sh','-c','cat > \"$0\"','{file}']");
            let paste = format!("['cat','{file}']");
            command.args([
                "--cmd",
                &format!(
                    "let g:clipboard={{'name':'slinger','copy':{{'+':{copy},'*':{copy}}},'paste':{{'+':{paste},'*':{paste}}},'cache_enabled':0}}"
                ),
            ]);
        }
        command.env("XDG_STATE_HOME", &state).env("XDG_CACHE_HOME", &cache).env_remove("NVIM");
        command.envs(setup.homes.iter().map(|(name, dir)| (name, dir)));
        command.current_dir(&setup.scratch);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = command.spawn()?;
        let (Some(stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(std::io::Error::other("nvim has no pipes"));
        };
        let (tell, heard) = wake.channel();
        let wake = wake.clone();
        std::thread::spawn(move || {
            let (mut bytes, mut chunk) = (Vec::new(), vec![0; 1 << 16]);
            'read: while let Ok(n @ 1..) = stdout.read(&mut chunk) {
                bytes.extend_from_slice(&chunk[..n]);
                let mut at = 0;
                loop {
                    match decode(&bytes[at..]) {
                        Ok((value, took)) => {
                            at += took;
                            if tell.send((value, Instant::now())).is_err() {
                                break 'read;
                            }
                        }
                        Err(Short::More) => break,
                        Err(Short::Bad(_)) => break 'read,
                    }
                }
                bytes.drain(..at);
            }
            // The main thread hears that Nvim is gone.
            drop(tell);
            wake.wake();
        });
        let trace = setup
            .trace
            .as_ref()
            .and_then(|path| std::fs::OpenOptions::new().create(true).append(true).open(path).ok());
        Ok(Process { child: Some(child), stdin, heard, trace })
    }

    /// A line of the trace, if one is kept.
    pub fn say(&mut self, line: &str) {
        if let Some(trace) = &mut self.trace {
            // A trace that cannot be written is not worth stopping for.
            let _ = writeln!(trace, "{line}");
        }
    }

    /// Writes `bytes` to Nvim. False when it is gone.
    pub fn send(&mut self, bytes: &[u8]) -> bool {
        bytes.is_empty() || self.stdin.write_all(bytes).and_then(|()| self.stdin.flush()).is_ok()
    }

    /// The messages that came, oldest first, and whether Nvim is gone.
    pub fn heard(&self) -> (Vec<(Value, Instant)>, bool) {
        let mut messages = Vec::new();
        loop {
            match self.heard.try_recv() {
                Ok(message) => messages.push(message),
                Err(TryRecvError::Empty) => return (messages, false),
                Err(TryRecvError::Disconnected) => return (messages, true),
            }
        }
    }

    /// The next message, waiting at most `most` for it.
    pub fn wait(&self, most: Duration) -> Option<(Value, Instant)> {
        self.heard.recv_timeout(most).ok()
    }

    /// Kills Nvim now, as a crash would (a test's).
    #[cfg(test)]
    pub fn crash(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Process {
    /// Kills this Nvim, by its pid, and reaps it off the main thread.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
