//! `slinger daemon ...`: which slingerd answers, and stopping it without
//! its pid. One request per command.

use clap::Subcommand;
use serde_json::json;

use slinger_control as control;

use crate::output::Output;

#[derive(Subcommand)]
pub enum Daemon {
    /// Show the slingerd that answers: version, build, pid, executable, socket, data directory, Claude Code and log
    Info,
    /// Stop slingerd as SIGTERM does; it stays stopped until it is started again
    Stop,
}

impl Daemon {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Info => "daemon info",
            Self::Stop => "daemon stop",
        }
    }

    pub fn execute(&self, daemon: &control::Client) -> anyhow::Result<Output> {
        match self {
            Self::Info => {
                let info = daemon.daemon_info()?;
                let human = lines(&info);
                Ok(Output::new(serde_json::to_value(info)?, human))
            }
            Self::Stop => {
                let stopped = daemon.daemon_stop()?;
                Ok(Output::new(json!({ "pid": stopped.pid }), String::new()))
            }
        }
    }
}

/// One field per line, the name and the value separated by a tab; `-` for
/// a path it has none of.
fn lines(info: &control::DaemonInfo) -> String {
    let none = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
    [
        ("version", info.version.clone()),
        ("build", info.build.clone()),
        ("protocol", info.protocol.to_string()),
        ("pid", info.pid.to_string()),
        ("started_ms", info.started_ms.to_string()),
        ("exe", info.exe.clone()),
        ("socket", info.socket.clone()),
        ("data_dir", info.data_dir.clone()),
        ("claude", none(&info.claude)),
        ("slinger", info.slinger.clone()),
        ("log", none(&info.log)),
    ]
    .map(|(name, value)| format!("{name}\t{value}"))
    .join("\n")
}
