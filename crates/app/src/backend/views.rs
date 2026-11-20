//! Keeping where the panes look (SPEC section 5: the daemon keeps each
//! pane's view and the focus): the window hands the shown workspace's
//! [`Looks`] to a [`Saver`] each time they change, and a thread of its own
//! sends them as one `pane_view` once [`WAIT`] passes with no newer looks,
//! only the panes that changed since it last sent. Looks of another
//! workspace send the pending ones at once (a switch), and so does
//! [`Saver::flush`] (quit), which waits a bounded time. The main thread
//! never waits on the socket. A send that fails is tried again with the
//! next looks, and is otherwise dropped: a lost view is not worth a
//! problem on screen. A daemon without the verb (`bad_request`) leaves the
//! app as it was.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use slinger_control::{Client, PaneViewed};

/// How long looks wait for newer ones before they are sent.
pub const WAIT: Duration = Duration::from_millis(300);

/// Where one workspace's panes look: each canvas pane that has a place, by
/// its id, and the id of the pane with focus.
#[derive(Debug, Clone, PartialEq)]
pub struct Looks {
    pub workspace: String,
    pub views: Vec<PaneViewed>,
    pub focus: Option<u32>,
}

impl Looks {
    /// What changed since `sent` (the same workspace's looks last sent):
    /// the views that differ and the focus if it moved; `None` for nothing.
    fn since(&self, sent: Option<&Self>) -> Option<(Vec<PaneViewed>, Option<u32>)> {
        let was = |v: &PaneViewed| sent.is_some_and(|s| s.views.contains(v));
        let views: Vec<_> = self.views.iter().filter(|v| !was(v)).cloned().collect();
        let focus = self.focus.filter(|_| sent.map(|s| s.focus) != Some(self.focus));
        (!views.is_empty() || focus.is_some()).then_some((views, focus))
    }
}

/// What the window asks of the thread.
enum Ask {
    /// The newest looks.
    Looks(Looks),
    /// Send what waits now, then say so.
    Flush(Sender<()>),
}

/// Sends one workspace's changed views and focus; whether it took them.
type Send = Box<dyn FnMut(&str, &[PaneViewed], Option<u32>) -> bool + std::marker::Send>;

/// The thread that keeps the panes' views. Dropping it sends what waits,
/// then ends the thread.
pub struct Saver {
    asks: Sender<Ask>,
    /// The looks last handed over.
    handed: Option<Looks>,
    /// The client the thread sends through; `None` in a test.
    daemon: Option<Arc<Mutex<Client>>>,
}

impl std::fmt::Debug for Saver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Saver").field("handed", &self.handed).finish_non_exhaustive()
    }
}

impl Saver {
    /// Starts keeping views through `daemon`. Returns at once.
    pub fn start(daemon: Client) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let theirs = Arc::clone(&daemon);
        let send = move |workspace: &str, views: &[PaneViewed], focus| {
            let client = theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
            client.pane_view(workspace, views, focus).is_ok()
        };
        Self { daemon: Some(daemon), ..Self::over(WAIT, Box::new(send)) }
    }

    /// Starts a thread that sends with `send`, `wait` after the last looks.
    fn over(wait: Duration, send: Send) -> Self {
        let (asks, heard) = mpsc::channel();
        thread::spawn(move || keep(&heard, wait, send));
        Self { asks, handed: None, daemon: None }
    }

    /// Sends through `daemon` from the next send on.
    pub fn reconnect(&self, daemon: Client) {
        if let Some(mine) = &self.daemon {
            *mine.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
        }
    }

    /// Hands over `looks` if they differ from the last handed. Never waits.
    pub fn hand(&mut self, looks: Looks) {
        if self.handed.as_ref() != Some(&looks) {
            self.handed = Some(looks.clone());
            // The thread lives as long as the saver, so the send cannot fail.
            let _ = self.asks.send(Ask::Looks(looks));
        }
    }

    /// Sends what waits at once, and waits for that at most `within`;
    /// whether it was sent in time (quit).
    pub fn flush(&self, within: Duration) -> bool {
        let (done, sent) = mpsc::channel();
        self.asks.send(Ask::Flush(done)).is_ok() && sent.recv_timeout(within).is_ok()
    }
}

/// The thread: takes looks from `heard` until the saver is gone, and sends
/// each workspace's newest with `send` once `wait` passes with no newer
/// ones, or at once for a switch or a flush.
fn keep(heard: &Receiver<Ask>, wait: Duration, mut send: Send) {
    let (mut waiting, mut told): (Option<Looks>, Vec<Looks>) = (None, Vec::new());
    let mut out = |looks: Option<Looks>| {
        let Some(looks) = looks else { return };
        let at = told.iter().position(|s| s.workspace == looks.workspace);
        let Some((views, focus)) = looks.since(at.map(|at| &told[at])) else { return };
        if send(&looks.workspace, &views, focus) {
            at.map(|at| told.swap_remove(at));
            told.push(looks);
        }
    };
    loop {
        let ask = match waiting {
            Some(_) => heard.recv_timeout(wait),
            None => heard.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match ask {
            Ok(Ask::Looks(looks)) => {
                if waiting.as_ref().is_some_and(|w| w.workspace != looks.workspace) {
                    out(waiting.take());
                }
                waiting = Some(looks);
            }
            Ok(Ask::Flush(done)) => {
                out(waiting.take());
                let _ = done.send(());
            }
            Err(RecvTimeoutError::Timeout) => out(waiting.take()),
            Err(RecvTimeoutError::Disconnected) => return out(waiting.take()),
        }
    }
}

#[cfg(test)]
mod tests;
