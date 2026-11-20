//! The sidebar's offer to restart a daemon of another build: both builds
//! named, the agents a restart ends counted, a dismissed offer hidden for
//! that daemon only; an old daemon says how to stop it and offers none.

use std::path::PathBuf;

use super::*;
use crate::config::Rgb;
use crate::scene::sidebar::{Agent, Status, Workspace};

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

const PANES: Rect = Rect { x: 200.0, y: 0.0, width: 760.0, height: 600.0 };

fn other(pid: u32) -> Offer {
    let skew = Skew::Differs { daemon: "def5678".into(), app: "abc1234".into() };
    Offer { pid: Some(pid), skew }
}

/// The workspace `<agent>-ws` whose agent's process runs or not.
fn ws(agent: &str, running: bool) -> Workspace {
    let status = Some(if running { Status::Working } else { Status::Stopped });
    let agent = Agent { name: agent.into(), status, running, ..Agent::default() };
    Workspace {
        name: format!("{}-ws", agent.name),
        canvas: agent.name.clone(),
        dir: PathBuf::from("/w").join(&agent.name),
        agent: Some(agent),
        ..Workspace::default()
    }
}

fn texts(sidebar: &Sidebar) -> Vec<String> {
    sidebar.notice(&T, PANES).into_iter().map(|l| l.text).collect()
}

#[test]
fn a_daemon_of_another_build_is_offered_for_a_restart_with_both_builds() {
    let mut sidebar = Sidebar::new(true);
    assert_eq!(
        texts(&sidebar),
        Vec::<String>::new(),
        "no offer: an unmanaged app, or the same build"
    );
    assert!(sidebar.set_offer(Some(other(7))));
    assert!(!sidebar.set_offer(Some(other(7))), "nothing new");
    assert_eq!(
        texts(&sidebar),
        ["slingerd is another build (def5678; this app is abc1234)", "✕", "Restart slingerd"]
    );
    let labels = sidebar.notice(&T, PANES);
    let targets = sidebar.notice_targets(&T, PANES);
    let restart = (labels[2].frame, Tap::Run(Command::RestartDaemon));
    assert_eq!(targets, [(labels[1].frame, Tap::Waive), restart]);
    assert!(labels[2].frame.y + labels[2].frame.height < PANES.height - status::HEIGHT);
}

#[test]
fn the_offer_counts_the_running_agents_a_restart_ends_once_each() {
    let mut sidebar = Sidebar::new(true);
    let shared = Workspace { name: "again".into(), ..ws("ada", true) };
    sidebar.update(Ok(vec![ws("ada", true), ws("bo", false), shared]));
    sidebar.set_offer(Some(other(7)));
    assert_eq!(texts(&sidebar)[2], "Restart slingerd · ends 1 running agent");
    sidebar.update(Ok(vec![ws("ada", true), ws("bo", true)]));
    assert_eq!(texts(&sidebar)[2], "Restart slingerd · ends 2 running agents");
}

#[test]
fn a_dismissed_offer_stays_hidden_for_that_daemon_only() {
    let mut sidebar = Sidebar::new(true);
    sidebar.set_offer(Some(other(7)));
    sidebar.waive();
    assert_eq!(sidebar.offer(), None);
    assert!(!sidebar.set_offer(Some(other(7))), "asked again: still hidden");
    assert!(sidebar.set_offer(Some(other(8))), "another daemon");
    sidebar.dismiss();
    assert_eq!(texts(&sidebar), Vec::<String>::new(), "dismiss-notice hides it too");
    assert!(!sidebar.set_offer(None));
}

#[test]
fn an_old_daemon_says_how_to_stop_it_and_offers_no_restart() {
    let mut sidebar = Sidebar::new(true);
    sidebar.set_offer(Some(Offer { pid: None, skew: Skew::Old { app: "abc1234".into() } }));
    assert!(sidebar.offer().is_some_and(Offer::old));
    assert_eq!(
        texts(&sidebar),
        [
            "slingerd is an older build (this app is abc1234)",
            "✕",
            "stop it once with SIGTERM: slinger then starts its own"
        ]
    );
    let targets = sidebar.notice_targets(&T, PANES);
    assert!(targets.iter().all(|t| t.1 != Tap::Run(Command::RestartDaemon)), "{targets:?}");
}

#[test]
fn the_offer_sits_above_the_agents_bar() {
    let mut sidebar = Sidebar::new(true);
    sidebar.update(Ok(vec![ws("ada", true)]));
    sidebar.update(Ok(vec![Workspace {
        agent: Some(Agent { status: Some(Status::Blocked), ..ws("ada", true).agent.unwrap() }),
        ..ws("ada", true)
    }]));
    sidebar.set_offer(Some(other(7)));
    let labels = sidebar.notice(&T, PANES);
    assert_eq!(labels[0].text, "ada is blocked");
    let (bar, restart) = (labels[0].frame, labels[4].frame);
    assert_eq!(labels[4].text, "Restart slingerd · ends 1 running agent");
    assert!(restart.y + restart.height < bar.y, "{restart:?} above {bar:?}");
}
