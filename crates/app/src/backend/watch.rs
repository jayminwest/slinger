//! The [`Watch`]: the agents' statuses, the workspace's panes and the code
//! of each agent UI they show, asked of the daemon on a thread of its own.
//! A round ([`round`]) starts at once when the window picks a workspace or
//! a socket ([`Watch::want`], [`Watch::follow`]), else every [`EVERY`], for
//! a pane changed from outside the app: the daemon has no feed of layouts.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slinger_control::{Client, Pane, PaneKind, PaneLayout, PaneTree};

use super::wake::Wake;
use super::{Problem, panes, ui, workspaces};
use crate::scene::sidebar::Workspace;
use crate::trace::Stamps;
use crate::ui::Shown;

/// How often a [`Watch`] asks the daemon when nobody pokes it.
const EVERY: Duration = Duration::from_secs(1);

/// Asks the daemon for the panes ([`panes::layout`]) and for
/// [`workspaces`] in rounds on a thread of its own, so a slow daemon never
/// holds up the window. Each time the layout changes it fetches the code of
/// every agent UI the panes show ([`ui::fetch`]) before it tells of the
/// layout, and asks for the agents' list only after. The window takes the
/// answers with [`Watch::latest`], [`Watch::layout`] and [`Watch::uis`].
#[derive(Debug)]
pub struct Watch {
    asked: Arc<Asked>,
    news: Receiver<Result<Vec<Workspace>, Problem>>,
    layouts: Receiver<(Result<PaneLayout, Problem>, Stamps)>,
    codes: Receiver<Vec<(String, Result<Shown, Problem>)>>,
}

/// What the window asks of the thread, and the bell that starts a round.
#[derive(Debug, Default)]
struct Asked {
    now: Mutex<Asks>,
    bell: Condvar,
}

/// The socket and the workspace wanted; whether they changed since the
/// thread last looked; and whether the watch is gone.
#[derive(Debug, Default)]
struct Asks {
    socket: PathBuf,
    wanted: Option<String>,
    poked: bool,
    gone: bool,
}

impl Asked {
    fn lock(&self) -> MutexGuard<'_, Asks> {
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Changes what is asked with `change`, and starts a round at once.
    fn poke(&self, change: impl FnOnce(&mut Asks)) {
        let mut asks = self.lock();
        change(&mut asks);
        asks.poked = true;
        self.bell.notify_all();
    }

    /// Waits up to `every` for a poke; then what is asked, or `None` once
    /// the watch is gone.
    fn next(&self, every: Duration) -> Option<(PathBuf, Option<String>)> {
        let asks = self.lock();
        let (mut asks, _) = self
            .bell
            .wait_timeout_while(asks, every, |a| !a.poked && !a.gone)
            .unwrap_or_else(PoisonError::into_inner);
        asks.poked = false;
        (!asks.gone).then(|| (asks.socket.clone(), asks.wanted.clone()))
    }
}

/// What a round tells the window, in the order it tells it.
#[derive(Debug)]
enum Told {
    /// The code of the agent UIs a new layout shows: before the layout.
    Uis(Vec<(String, Result<Shown, Problem>)>),
    /// A new layout, with when the round read the wanted name and when it sent it.
    Layout(Result<PaneLayout, Problem>, Stamps),
    /// The workspaces and their agents, when they changed.
    Agents(Result<Vec<Workspace>, Problem>),
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

/// `layout` as the window arranges it: its workspace and its panes, by
/// id, what each shows and their splits, with no kept view or focus. The
/// app's own `pane_view` changes only those, so it is not layout news.
fn arrangement(layout: &PaneLayout) -> PaneLayout {
    fn bare(tree: &PaneTree) -> PaneTree {
        match tree {
            PaneTree::Pane(pane) => PaneTree::Pane(Pane { view: None, ..pane.clone() }),
            PaneTree::Split { split, ratio, first, second } => PaneTree::Split {
                split: *split,
                ratio: *ratio,
                first: Box::new(bare(first)),
                second: Box::new(bare(second)),
            },
        }
    }
    let workspace = layout.workspace.clone();
    PaneLayout { workspace, layout: bare(&layout.layout), pane: None, event: None, focus: None }
}

/// What a round remembers: the last layout told, as arranged, and the last
/// agents told.
#[derive(Default)]
struct Last {
    drawn: Option<Result<PaneLayout, Problem>>,
    heard: Option<Result<Vec<Workspace>, Problem>>,
}

/// One round through `daemon`, the wanted name read at `watch`: the panes
/// of `wanted` (else of `last`); when they changed, the code of their agent
/// UIs, then the layout; then the agents, when they changed. Each goes to
/// `tell`; false once it says nobody listens.
fn round(
    daemon: &Client,
    (wanted, last): (Option<&str>, Option<&str>),
    watch: Instant,
    told: &mut Last,
    tell: &mut impl FnMut(Told) -> bool,
) -> bool {
    let layout = panes::layout(daemon, wanted, last);
    let arranged = layout.as_ref().map(arrangement).map_err(Clone::clone);
    if told.drawn.as_ref() != Some(&arranged) {
        if let Ok(now) = &layout
            && !tell(Told::Uis(load(daemon, now)))
        {
            return false;
        }
        told.drawn = Some(arranged);
        if !tell(Told::Layout(layout, Stamps { watch, layout: Instant::now() })) {
            return false;
        }
    }
    let agents = workspaces(daemon);
    if told.heard.as_ref() == Some(&agents) {
        return true;
    }
    told.heard = Some(agents.clone());
    tell(Told::Agents(agents))
}

impl Watch {
    /// Starts asking the daemon on `socket`. The thread ends once the
    /// watch is dropped. The panes are those of `wanted`, else of `last`
    /// (see [`panes::layout`]). Each answer wakes the main thread.
    pub fn start(
        socket: PathBuf,
        wanted: Option<String>,
        last: Option<String>,
        wake: &Wake,
    ) -> Self {
        Self::every(EVERY, (socket, wanted, last), wake)
    }

    /// [`Watch::start`], with a round at least every `every`.
    fn every(
        every: Duration,
        (socket, wanted, last): (PathBuf, Option<String>, Option<String>),
        wake: &Wake,
    ) -> Self {
        let asked = Arc::new(Asked::default());
        asked.poke(|a| (a.socket, a.wanted) = (socket, wanted));
        let (agents, news) = wake.channel();
        let (panes, layouts) = wake.channel();
        let (uis, codes) = wake.channel();
        let theirs = Arc::clone(&asked);
        std::thread::spawn(move || {
            let mut told = Last::default();
            let mut tell = |news| match news {
                Told::Uis(code) => uis.send(code).is_ok(),
                Told::Layout(layout, stamps) => panes.send((layout, stamps)).is_ok(),
                Told::Agents(now) => agents.send(now).is_ok(),
            };
            while let Some((socket, wanted)) = theirs.next(every) {
                let wanted = (wanted.as_deref(), last.as_deref());
                let watch = Instant::now();
                if !round(&Client::new(socket), wanted, watch, &mut told, &mut tell) {
                    return;
                }
            }
        });
        Self { asked, news, layouts, codes }
    }

    /// Asks for the panes of `wanted`, at once.
    pub fn want(&self, wanted: Option<String>) {
        self.asked.poke(|a| a.wanted = wanted);
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

    /// Asks again, at once: a daemon came to answer.
    pub fn poke(&self) {
        self.asked.poke(|_| {});
    }

    /// Asks the daemon on `socket`, at once.
    pub fn follow(&self, socket: PathBuf) {
        self.asked.poke(|a| a.socket = socket);
    }

    /// The newest answer since the last call, if it changed. Never blocks.
    pub fn latest(&self) -> Option<Result<Vec<Workspace>, Problem>> {
        self.news.try_iter().last()
    }
}

impl Drop for Watch {
    /// The thread ends after the round under way, if any.
    fn drop(&mut self) {
        self.asked.poke(|a| a.gone = true);
    }
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

    /// A round no poke starts within a test: 30 s.
    const LONG: Duration = Duration::from_secs(30);

    #[test]
    fn want_starts_a_round_at_once() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            let asked = (socket.to_path_buf(), Some("nope".into()), None);
            let watch = Watch::every(LONG, asked, &Wake::none());
            crate::backend::testing::wait_until(|| watch.layout().is_some());
            let picked = Instant::now();
            watch.want(Some("notes".into()));
            let came = loop {
                assert!(picked.elapsed() < Duration::from_secs(10), "the round did not start");
                std::thread::sleep(Duration::from_millis(10));
                if let Some((layout, _)) = watch.layout() {
                    break layout;
                }
            };
            assert_eq!(came.unwrap().workspace, "notes");
            assert!(picked.elapsed() < LONG / 3, "a poke, not the timeout");
        });
    }

    #[test]
    fn a_poke_starts_a_round_at_once() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let watch = Watch::every(LONG, (socket.to_path_buf(), None, None), &Wake::none());
            crate::backend::testing::wait_until(|| watch.latest() == Some(Ok(Vec::new())));
            let _dir = open_notes(d);
            let poked = Instant::now();
            watch.poke();
            crate::backend::testing::wait_until(|| watch.latest().is_some());
            assert!(poked.elapsed() < LONG / 3, "a poke, not the timeout");
        });
    }

    #[test]
    fn a_round_tells_the_uis_then_the_layout_then_the_agents() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            let (mut told, mut order) = (Last::default(), Vec::new());
            let mut tell = |news: Told| {
                order.push(match news {
                    Told::Uis(_) => "uis",
                    Told::Layout(..) => "layout",
                    Told::Agents(_) => "agents",
                });
                true
            };
            let wanted = (Some("notes"), None);
            assert!(round(d, wanted, Instant::now(), &mut told, &mut tell));
            assert!(round(d, wanted, Instant::now(), &mut told, &mut tell), "nothing new");
            assert_eq!(order, ["uis", "layout", "agents"]);
        });
    }

    #[test]
    fn a_pane_view_sent_from_elsewhere_is_no_layout_news() {
        with_socket(|socket| {
            let d = &Client::new(socket);
            let _dir = open_notes(d);
            let asked = (socket.to_path_buf(), Some("notes".into()), None);
            let watch = Watch::every(Duration::from_millis(20), asked, &Wake::none());
            crate::backend::testing::wait_until(|| watch.layout().is_some());
            let view =
                control::PaneView { canvas: "notes".into(), page: None, x: 5.0, y: 5.0, zoom: 2.0 };
            let kept = d.pane_view("notes", &[control::PaneViewed { id: 1, view }], Some(1));
            assert!(kept.unwrap().layout.panes()[0].view.is_some(), "the daemon keeps it");
            for _ in 0..10 {
                watch.poke();
                std::thread::sleep(Duration::from_millis(30));
            }
            assert!(watch.layout().is_none(), "no layout news");
            d.pane_open("notes", control::PaneKind::Canvas, "notes", None, false).unwrap();
            crate::backend::testing::wait_until(|| {
                watch.layout().is_some_and(|(l, _)| l.unwrap().layout.panes()[0].view.is_some())
            });
        });
    }

    #[test]
    fn dropping_the_watch_ends_its_thread() {
        let asked = (PathBuf::from("/nonexistent/x.sock"), None, None);
        let watch = Watch::every(LONG, asked, &Wake::none());
        let held = Arc::downgrade(&watch.asked);
        crate::backend::testing::wait_until(|| watch.latest().is_some());
        drop(watch);
        crate::backend::testing::wait_until(|| held.upgrade().is_none());
    }
}
