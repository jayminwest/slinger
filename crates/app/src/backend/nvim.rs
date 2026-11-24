//! SPIKE (slinger-dd9b7b): the Nvim a canvas text is edited by, as a child
//! process (`nvim --embed`): its stdin takes the client's messages, and a
//! thread reads its stdout into values, each stamped with when it came,
//! and wakes the main thread. One process per text edited: it starts when
//! the text opens and is killed, by its own pid, when the text ends.
//!
//! Nothing of the user's is written: no swap file (`-n`), no shada
//! (`-i NONE`), and the buffer is a scratch one, so no undo file and no
//! autowrite. `SLINGER_NVIM` turns it on: `user` loads the user's config,
//! `clean` none (`--clean`). `SLINGER_NVIM_CLIP` names a file Nvim's
//! clipboard provider reads and writes in place of the system clipboard (a
//! driven window never touches the user's). `SLINGER_NVIM_TRACE` names a
//! file the timings go to.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use super::Wake;
use crate::nvim::pack::{Short, Value, decode};

/// How Nvim is started, from the environment.
#[derive(Debug, Clone)]
pub struct Setup {
    clean: bool,
    clip: Option<PathBuf>,
    /// Where the timings go, if anywhere.
    pub trace: Option<PathBuf>,
}

impl Setup {
    /// What the environment asks: `None` unless `SLINGER_NVIM` is `user` or `clean`.
    pub fn from_env() -> Option<Setup> {
        let clean = match std::env::var("SLINGER_NVIM").ok()?.as_str() {
            "clean" => true,
            "user" => false,
            _ => return None,
        };
        let path = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Some(Setup { clean, clip: path("SLINGER_NVIM_CLIP"), trace: path("SLINGER_NVIM_TRACE") })
    }
}

/// A running `nvim --embed`.
pub struct Process {
    child: Option<Child>,
    stdin: ChildStdin,
    heard: Receiver<(Value, Instant)>,
}

impl Process {
    /// Starts Nvim as `setup` says; its messages wake the main thread with `wake`.
    ///
    /// # Errors
    ///
    /// Nvim is not on the path, or cannot start.
    pub fn start(setup: &Setup, wake: &Wake) -> std::io::Result<Process> {
        let mut command = Command::new("nvim");
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
        // The user's config and plugins load, but what Nvim keeps between runs
        // (logs such as lsp.log, shada, undo, plugin state) goes to a
        // directory of its own, never the user's.
        command.env("XDG_STATE_HOME", std::env::temp_dir().join("slinger-nvim-state"));
        command.current_dir(std::env::temp_dir());
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
        Ok(Process { child: Some(child), stdin, heard })
    }

    /// Writes `bytes` to Nvim. False when it is gone.
    pub fn send(&mut self, bytes: &[u8]) -> bool {
        self.stdin.write_all(bytes).and_then(|()| self.stdin.flush()).is_ok()
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
