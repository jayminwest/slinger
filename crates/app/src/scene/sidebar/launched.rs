//! What a bundled app says of the daemon it manages: the launch line in
//! place of the list ([`Daemon`]), and the offer to restart a daemon of
//! another build ([`Offer`]). The offer is a bar of the notice, above the
//! agents' bar: what the daemon and the app are, and "Restart slingerd"
//! with the count of the agents' processes a restart ends. It stays until
//! the user taps its ✕ or `dismiss-notice`, and is not shown again for
//! that daemon's pid. Pure.

use super::{PAD, Sidebar};
use crate::commands::Command;
use crate::config::Theme;
use crate::launch::Skew;
use crate::layout::Rect;
use crate::scene::daemon::Daemon;
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, status, style};

/// The offer's button.
pub const RESTART: &str = "Restart slingerd";

/// A daemon of another build ([`Skew::Differs`] or [`Skew::Old`]), and its
/// pid when it said (an old one cannot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub pid: Option<u32>,
    pub skew: Skew,
}

impl Offer {
    /// It predates `daemon_info`, so it cannot be stopped from here.
    pub fn old(&self) -> bool {
        matches!(self.skew, Skew::Old { .. })
    }
}

/// The daemon as the app that manages it last told of it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Launched {
    daemon: Option<Daemon>,
    offer: Option<Offer>,
    /// The offer the user dismissed: hidden for its daemon's pid.
    dismissed: Option<Offer>,
    /// The app manages a bundled slingerd ([`crate::launch`]).
    managed: bool,
}

impl Sidebar {
    /// Takes whether the app manages a bundled slingerd: what
    /// `start-slingerd` and `restart-daemon` can act on.
    pub fn set_managed(&mut self, managed: bool) {
        self.launched.managed = managed;
    }

    /// Whether the app manages a bundled slingerd.
    pub fn managed(&self) -> bool {
        self.launched.managed
    }

    /// Takes what the app now says of the daemon it starts; whether that changed.
    pub fn set_daemon(&mut self, daemon: Option<Daemon>) -> bool {
        let changed = self.launched.daemon != daemon;
        self.launched.daemon = daemon;
        changed
    }

    /// What the app says of the daemon it starts, if anything.
    pub fn daemon(&self) -> Option<&Daemon> {
        self.launched.daemon.as_ref()
    }

    /// Takes the offer for the daemon that answers now; whether what shows changed.
    pub fn set_offer(&mut self, offer: Option<Offer>) -> bool {
        let before = self.offer().cloned();
        self.launched.offer = offer;
        self.offer() != before.as_ref()
    }

    /// The offer the window shows: none once dismissed for its daemon.
    pub fn offer(&self) -> Option<&Offer> {
        let offer = self.launched.offer.as_ref()?;
        let dismissed = self.launched.dismissed.as_ref().is_some_and(|d| d.pid == offer.pid);
        (!dismissed).then_some(offer)
    }

    /// Hides the offer for the daemon it names.
    pub fn waive(&mut self) {
        if let Some(offer) = &self.launched.offer {
            self.launched.dismissed = Some(offer.clone());
        }
    }

    /// How many agents' processes slingerd runs: what a restart ends.
    pub fn running(&self) -> usize {
        let mut names: Vec<&str> = (self.workspaces.iter().flatten())
            .filter_map(|w| w.agent.as_ref().filter(|a| a.running))
            .map(|a| a.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }

    /// The offer's frame in the panes' area `panes`: two rows a tap high,
    /// above the agents' bar `below` when it shows, else where it would.
    fn offer_frame(&self, panes: Rect, below: Option<Rect>) -> Option<Rect> {
        self.offer()?;
        let width = super::notice::WIDTH.min(panes.width - 2.0 * PAD).max(0.0);
        let bottom = below.map_or(panes.y + panes.height - status::HEIGHT, |b| b.y) - PAD;
        let height = 2.0 * style::TAP;
        Some(Rect { x: panes.x + (panes.width - width) / 2.0, y: bottom - height, width, height })
    }

    /// The offer's labels: what the daemon is and its ✕, then the restart
    /// button (or, for an old daemon, how to stop it); and their targets.
    pub(super) fn offer_bar(
        &self,
        theme: &Theme,
        panes: Rect,
        below: Option<Rect>,
    ) -> (Vec<Label>, Vec<Target>) {
        let (Some(offer), Some(frame)) = (self.offer(), self.offer_frame(panes, below)) else {
            return (Vec::new(), Vec::new());
        };
        let row = |y: f64| Rect { y, height: style::TAP, ..frame };
        let (top, under) = (row(frame.y), row(frame.y + style::TAP));
        let close = Rect { x: top.x + top.width - style::TAP, width: style::TAP, ..top };
        let text = Rect { width: top.width - close.width, ..top };
        let (said, then) = match &offer.skew {
            Skew::Differs { daemon, app } => {
                let ends = match self.running() {
                    0 => String::new(),
                    1 => " · ends 1 running agent".into(),
                    n => format!(" · ends {n} running agents"),
                };
                let said = format!("slingerd is another build ({daemon}; this app is {app})");
                (said, format!("{RESTART}{ends}"))
            }
            Skew::Old { app } => (
                format!("slingerd is an older build (this app is {app})"),
                "stop it once with SIGTERM: slinger then starts its own".into(),
            ),
            Skew::Same => return (Vec::new(), Vec::new()),
        };
        let fill = Some(theme.focus);
        let label = |text, frame| Label { text, colour: theme.background, fill, frame };
        let close_label = label(super::notice::CLOSE.into(), close);
        let labels = vec![label(said, text), close_label, label(then, under)];
        let mut targets = vec![(close, Tap::Waive)];
        if !offer.old() {
            targets.push((under, Tap::Run(Command::RestartDaemon)));
        }
        (labels, targets)
    }
}

#[cfg(test)]
mod tests;
