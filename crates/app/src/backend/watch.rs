//! The [`Watch`]: the agents' statuses, the workspace's panes and the code
//! of each agent UI they show, asked of the daemon on a thread of its own.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use slinger_control::{Client, PaneKind, PaneLayout};

use super::wake::{Teller, Wake};
use super::{Problem, panes, ui, workspaces};
use crate::scene::sidebar::Workspace;
use crate::trace::Stamps;
use crate::ui::Shown;

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
    layouts: Receiver<(Result<PaneLayout, Problem>, Stamps)>,
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
    /// Each answer wakes the main thread.
    pub fn start(
        socket: PathBuf,
        wanted: Option<String>,
        last: Option<String>,
        wake: &Wake,
    ) -> Self {
        let socket = Arc::new(Mutex::new(socket));
        let wanted = Arc::new(Mutex::new(wanted));
        let (agents, news) = wake.channel();
        let (panes, layouts) = wake.channel();
        let (uis, codes) = wake.channel();
        let (theirs, their_wanted) = (Arc::clone(&socket), Arc::clone(&wanted));
        std::thread::spawn(move || {
            let (mut heard, mut drawn) = (None, None);
            loop {
                let daemon =
                    Client::new(theirs.lock().unwrap_or_else(PoisonError::into_inner).clone());
                let wanted = their_wanted.lock().unwrap_or_else(PoisonError::into_inner).clone();
                let watch = Instant::now();
                let layout = panes::layout(&daemon, wanted.as_deref(), last.as_deref());
                let fresh = drawn.as_ref() != Some(&layout);
                let loaded = match &layout {
                    Ok(now) if fresh => uis.send(load(&daemon, now)).is_ok(),
                    _ => true,
                };
                if !loaded || !tell(&agents, &mut heard, workspaces(&daemon)) {
                    return;
                }
                if fresh {
                    drawn = Some(layout.clone());
                    let stamps = Stamps { watch, layout: Instant::now() };
                    if panes.send((layout, stamps)).is_err() {
                        return;
                    }
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

    /// The newest layout since the last call, if it changed, with when the
    /// thread read the wanted name for it and when it sent it. Never blocks.
    pub fn layout(&self) -> Option<(Result<PaneLayout, Problem>, Stamps)> {
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
fn tell<T: Clone + PartialEq>(to: &Teller<T>, last: &mut Option<T>, now: T) -> bool {
    if last.as_ref() == Some(&now) {
        return true;
    }
    *last = Some(now.clone());
    to.send(now).is_ok()
}

#[cfg(test)]
mod tests {
    use slinger_control as control;

    use super::*;
    use crate::backend::tests::{notes, open_notes, with_socket};
    use crate::scene::sidebar::Status;

    /// The watch's answers until one is `want`, or a failure after 10 s.
    fn until(watch: &Watch, want: &Result<Vec<Workspace>, Problem>) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while watch.latest().as_ref() != Some(want) {
            assert!(Instant::now() < deadline, "timed out waiting for {want:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_watch_hears_a_status_change_and_follows_a_new_socket() {
        let watch = Watch::start(PathBuf::from("/nonexistent/x.sock"), None, None, &Wake::none());
        let deadline = Instant::now() + Duration::from_secs(10);
        let lost = loop {
            if let Some(answer) = watch.latest() {
                break answer;
            }
            assert!(Instant::now() < deadline, "timed out");
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
            let watch =
                Watch::start(socket.to_path_buf(), Some("nope".into()), None, &Wake::none());
            let deadline = Instant::now() + Duration::from_secs(10);
            let count = || {
                assert!(Instant::now() < deadline, "timed out");
                std::thread::sleep(Duration::from_millis(20));
                let layout = watch.layout().map(|(l, _)| l);
                layout.map(|l| l.map(|l| l.layout.panes().len()).map_err(|p| p.message))
            };
            while count() != Some(Err("open workspace \"nope\" not found".into())) {}
            watch.want(Some("notes".into()));
            while count() != Some(Ok(1)) {}
            d.pane_open("notes", control::PaneKind::Canvas, "notes", None, false).unwrap();
            while count() != Some(Ok(2)) {}
        });
    }

    #[test]
    fn a_layout_carries_when_the_watch_read_the_name_and_when_it_sent_it() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            let watch =
                Watch::start(socket.to_path_buf(), Some("nope".into()), None, &Wake::none());
            let deadline = Instant::now() + Duration::from_secs(10);
            let picked = loop {
                assert!(Instant::now() < deadline, "timed out");
                std::thread::sleep(Duration::from_millis(20));
                if watch.layout().is_some() {
                    let picked = Instant::now();
                    watch.want(Some("notes".into()));
                    break picked;
                }
            };
            let (layout, stamps) = loop {
                assert!(Instant::now() < deadline, "timed out");
                std::thread::sleep(Duration::from_millis(20));
                if let Some((Ok(layout), stamps)) = watch.layout() {
                    break (layout, stamps);
                }
            };
            assert_eq!(layout.workspace, "notes");
            assert!(picked <= stamps.watch, "read before the pick");
            assert!(stamps.watch <= stamps.layout, "sent before it was read");
            assert!(stamps.layout <= Instant::now());
        });
    }
}
