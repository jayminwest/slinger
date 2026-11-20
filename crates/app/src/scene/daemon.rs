//! What the window says of the daemon and of the problems it has: the
//! lines in the focused pane, the sidebar's line in place of its list, and
//! why a save waits. Pure.
//!
//! While the app starts its bundled daemon ([`crate::launch`]), or it did
//! not start, or it is stopped on purpose, one launch line takes the place
//! of every problem from asking the daemon: those all say it does not
//! answer, and a shell hint is no help to someone who opened an app. A
//! problem with the config still shows.

use crate::backend::Problem;
use crate::commands::Command;
use crate::config::{Rgb, Theme};

/// The daemon as a managed app tells of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    /// The app started it and waits for it to answer.
    Starting,
    /// It did not start: its exit code (`None`: it did not exit, or a
    /// signal ended it), the first line it said, and its log.
    Failed { code: Option<i32>, said: Option<String>, log: String },
    /// Stopped on purpose (`slinger daemon stop`); `start-slingerd` starts it.
    Stopped,
}

/// The launch line that says the daemon is starting.
pub const STARTING: &str = "Starting slingerd…";
/// The sidebar's line under a stopped daemon: a tap starts it.
pub const START: &str = "Start slingerd";

impl Daemon {
    /// Its lines, each with its colour: what it is, then what to do.
    pub fn lines(&self, theme: &Theme) -> Vec<(String, Rgb)> {
        match self {
            Daemon::Starting => vec![(STARTING.into(), theme.muted)],
            Daemon::Failed { code, said, log } => {
                let code = code.map(|c| format!(" (exit {c})")).unwrap_or_default();
                let said = said.as_deref().map(|s| format!(": {}", s.trim_start_matches("✗ ")));
                let failed = format!("✗ slingerd did not start{code}{}", said.unwrap_or_default());
                vec![(failed, theme.error), (format!("see {log}"), theme.muted)]
            }
            Daemon::Stopped => vec![
                ("slingerd is stopped".into(), theme.muted),
                (format!("{START} in the sidebar or the palette starts it"), theme.muted),
            ],
        }
    }

    /// The sidebar's lines in place of its list: the first line, and for a
    /// stopped daemon the line whose tap runs `start-slingerd`.
    pub fn sidebar(&self, theme: &Theme) -> Vec<(String, Rgb, Option<Command>)> {
        let mut lines = self.lines(theme);
        lines.truncate(1);
        let mut lines: Vec<_> = lines.into_iter().map(|(t, c)| (t, c, None)).collect();
        if *self == Daemon::Stopped {
            lines.push((START.into(), theme.text, Some(Command::StartSlingerd)));
        }
        lines
    }

    /// Why a save waits, while the daemon is not there to take it.
    pub fn unsaved(&self) -> &'static str {
        match self {
            Daemon::Starting => "slingerd is starting",
            Daemon::Failed { .. } => "slingerd did not start",
            Daemon::Stopped => "slingerd is stopped",
        }
    }
}

/// The lines the focused pane shows: `daemon`'s launch line in place of
/// the problems `asking` the daemon gave, else those; then the config's
/// `problem`; each once, each problem with its hint under it.
pub fn said<'a>(
    daemon: Option<&Daemon>,
    asking: impl Iterator<Item = &'a Problem>,
    problem: Option<&'a Problem>,
    theme: &Theme,
) -> Vec<(String, Rgb)> {
    let mut problems: Vec<&Problem> = Vec::new();
    for p in asking.filter(|_| daemon.is_none()).chain(problem) {
        if !problems.contains(&p) {
            problems.push(p);
        }
    }
    let mut lines = daemon.map(|d| d.lines(theme)).unwrap_or_default();
    for p in problems {
        lines.push((format!("✗ {}", p.message), theme.error));
        lines.extend(p.hint.clone().map(|hint| (hint, theme.muted)));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rgb;

    const T: Theme = Theme {
        background: Rgb { r: 0, g: 0, b: 0 },
        text: Rgb { r: 1, g: 1, b: 1 },
        muted: Rgb { r: 2, g: 2, b: 2 },
        error: Rgb { r: 3, g: 3, b: 3 },
        focus: Rgb { r: 4, g: 4, b: 4 },
        done: Rgb { r: 5, g: 5, b: 5 },
    };

    fn problem(message: &str, hint: Option<&str>) -> Problem {
        Problem { message: message.into(), hint: hint.map(str::to_string) }
    }

    fn texts(lines: &[(String, Rgb)]) -> Vec<(&str, Rgb)> {
        lines.iter().map(|(t, c)| (t.as_str(), *c)).collect()
    }

    #[test]
    fn with_no_launch_line_each_problem_shows_once_with_its_hint() {
        let lost = problem("slingerd does not answer on /s", Some("start it with 'slingerd'"));
        let config = problem("config.toml: line 2: x", None);
        let lines = said(None, [&lost, &lost].into_iter(), Some(&config), &T);
        assert_eq!(
            texts(&lines),
            [
                ("✗ slingerd does not answer on /s", T.error),
                ("start it with 'slingerd'", T.muted),
                ("✗ config.toml: line 2: x", T.error),
            ]
        );
    }

    #[test]
    fn starting_is_one_muted_line_in_place_of_the_daemon_s_problems() {
        let lost = problem("slingerd does not answer on /s", Some("start it with 'slingerd'"));
        let config = problem("config.toml: line 2: x", None);
        let lines = said(Some(&Daemon::Starting), [&lost].into_iter(), Some(&config), &T);
        assert_eq!(
            texts(&lines),
            [("Starting slingerd…", T.muted), ("✗ config.toml: line 2: x", T.error)]
        );
        assert!(lines.iter().all(|(t, _)| !t.contains("--socket")));
    }

    #[test]
    fn a_failed_start_says_its_exit_and_points_at_the_log() {
        let said = Some("✗ cannot open /d: denied".to_string());
        let failed = Daemon::Failed { code: Some(1), said, log: "/d/slingerd.log".into() };
        assert_eq!(
            texts(&failed.lines(&T)),
            [
                ("✗ slingerd did not start (exit 1): cannot open /d: denied", T.error),
                ("see /d/slingerd.log", T.muted),
            ]
        );
        let silent = Daemon::Failed { code: None, said: None, log: "/l".into() };
        assert_eq!(texts(&silent.lines(&T))[0], ("✗ slingerd did not start", T.error));
        assert_eq!(silent.sidebar(&T), [("✗ slingerd did not start".into(), T.error, None)]);
    }

    #[test]
    fn a_stopped_daemon_offers_to_start_it() {
        assert_eq!(
            Daemon::Stopped.sidebar(&T),
            [
                ("slingerd is stopped".into(), T.muted, None),
                ("Start slingerd".into(), T.text, Some(Command::StartSlingerd)),
            ]
        );
        assert_eq!(Daemon::Starting.sidebar(&T), [(STARTING.into(), T.muted, None)]);
        assert_eq!(Daemon::Starting.unsaved(), "slingerd is starting");
    }
}
