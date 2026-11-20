//! The daemon's own verbs: `daemon_info` and `daemon_stop` (docs/service.md,
//! "The daemon's own verbs"). They describe and end the process that
//! answers, so a client can tell which build serves it and stop it without
//! its pid.
//!
//! [`crate::Request`] reads them as one variant, [`DaemonAsk`], and does not
//! write them: a client writes [`DaemonAsk`] itself, and a binary that never
//! sends one carries no code for them. A daemon older than these verbs
//! answers `bad_request`, which a client reads as "an older build".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// The file in `data_dir` that says its daemon was stopped on purpose
/// (`daemon_stop`). The daemon writes it as it stops that way and removes it
/// when it next starts, so a launcher that finds it leaves the daemon
/// stopped; one that finds no daemon and no mark may start one.
#[must_use]
pub fn stop_mark(data_dir: &Path) -> PathBuf {
    data_dir.join("stopped")
}

/// A request about the daemon itself, as a client writes it and the daemon
/// reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DaemonAsk {
    /// Which daemon answers: its build, process and files.
    DaemonInfo,
    /// Stop the daemon as SIGTERM does, after this reply, and leave
    /// [`stop_mark`] in its data directory.
    DaemonStop,
}

/// The reply to `daemon_info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    /// The package version (`slingerd --version`).
    pub version: String,
    /// The build stamp ([`crate::stamp`]): a commit, `-dirty`, or `dev`.
    pub build: String,
    /// The protocol version it speaks ([`crate::VERSION`]).
    pub protocol: u64,
    pub pid: u32,
    /// When it began to serve, in milliseconds since the Unix epoch, UTC.
    pub started_ms: u64,
    /// The executable it runs.
    pub exe: String,
    pub socket: String,
    pub data_dir: String,
    /// The Claude Code it would run for an agent started now; none when it
    /// finds none.
    pub claude: Option<String>,
    /// The CLI its agents' hooks run.
    pub slinger: String,
    /// Its `--log` file; none when it writes to stderr.
    pub log: Option<String>,
}

/// The reply to `daemon_stop`: the process that stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStopped {
    pub pid: u32,
}

impl Client {
    /// Which daemon answers.
    ///
    /// # Errors
    ///
    /// `bad_request` from a daemon older than the verb, and `unavailable`
    /// when the daemon does not answer.
    pub fn daemon_info(&self) -> Result<DaemonInfo, RemoteError> {
        self.call(&DaemonAsk::DaemonInfo)
    }

    /// Stops the daemon. It replies first; replies already owed still go out.
    ///
    /// # Errors
    ///
    /// `bad_request` from a daemon older than the verb, `internal` when it
    /// cannot leave its mark (it then keeps serving), and `unavailable` when
    /// the daemon does not answer.
    pub fn daemon_stop(&self) -> Result<DaemonStopped, RemoteError> {
        self.call(&DaemonAsk::DaemonStop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn each_ask_reads_back_as_itself_by_its_own_op() {
        for (ask, line) in [
            (DaemonAsk::DaemonInfo, r#"{"v":1,"op":"daemon_info"}"#),
            (DaemonAsk::DaemonStop, r#"{"v":1,"op":"daemon_stop"}"#),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Daemon(ask)), "{line}");
        }
    }

    #[test]
    fn an_info_reply_reads_with_or_without_claude_and_log() {
        let reply = r#"{"version":"0.1.0","build":"abc1234-dirty","protocol":1,"pid":42,
            "started_ms":5,"exe":"/a/slingerd","socket":"/t/s.sock","data_dir":"/d",
            "claude":null,"slinger":"/a/slinger","log":"/d/slingerd.log"}"#;
        let info: DaemonInfo = serde_json::from_str(reply).unwrap();
        assert_eq!((info.build.as_str(), info.pid, info.claude), ("abc1234-dirty", 42, None));
        assert_eq!(info.log.as_deref(), Some("/d/slingerd.log"));
        let stopped: DaemonStopped = serde_json::from_str(r#"{"pid":42}"#).unwrap();
        assert_eq!(stopped, DaemonStopped { pid: 42 });
    }

    #[test]
    fn the_mark_is_in_the_data_directory() {
        assert_eq!(stop_mark(Path::new("/d")), Path::new("/d/stopped"));
    }
}
