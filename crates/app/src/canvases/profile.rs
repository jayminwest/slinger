//! The profile menu's rows for a pane (`scene::pages::menu`, listing
//! [`List::Profiles`](crate::scene::pages::menu::List)) and a sling under
//! the one tapped. The agent's process starts under it first when it runs
//! under another ([`crate::backend::Slinger`]). Runs on the main thread and
//! never waits.

use super::{Canvases, canvas_of};
use crate::layout::{PaneId, Rect};
use crate::panes::Panes;
use crate::scene::sidebar::Status;
use crate::scene::sling::Profile;

/// The profile menu's first row: the agent within its own limits.
pub const OWN: &str = "no profile";

impl Canvases {
    /// The rows of the profile menu for `pane` of `panes`: [`OWN`], then
    /// its agent's profiles; the one its process runs under marked, none
    /// while it does not run. `None` when the pane shows no canvas, or its
    /// agent has no profiles.
    pub fn profile_names(
        &self,
        panes: &Panes,
        pane: PaneId,
    ) -> Option<(Vec<String>, Option<usize>)> {
        let agent = self.agent_of(canvas_of(panes, pane)?)?;
        if agent.profiles.is_empty() {
            return None;
        }
        let names = std::iter::once(OWN.to_string()).chain(agent.profiles.iter().cloned());
        let running = agent.status != Some(Status::Stopped);
        let shown = match &agent.profile {
            Some(profile) => agent.profiles.iter().position(|p| p == profile).map(|i| i + 1),
            None => Some(0),
        };
        Some((names.collect(), shown.filter(|_| running)))
    }

    /// A tap on the profile menu's row `index` for the focused pane of
    /// `panes` laid in `area`: slings as `sling-now` does, under that
    /// profile ([`OWN`]: the agent's own limits). A row no longer there
    /// does nothing; false then, and as `sling-now`.
    pub fn sling_under(&mut self, panes: &Panes, area: Rect, index: usize) -> bool {
        let Some(names) = self.profile_names(panes, panes.focus()).map(|(names, _)| names) else {
            return false;
        };
        let profile = match names.get(index) {
            None => return false,
            Some(_) if index == 0 => None,
            Some(name) => Some(name.clone()),
        };
        self.sling_now_under(panes, area, Profile::Picked(profile))
    }
}

#[cfg(test)]
mod tests {
    use slinger_control::Client;

    use super::*;
    use crate::backend::testing::{delivered, fake_claude, user_stroke, wait_until};
    use crate::backend::tests::{open_notes, with_store};
    use crate::config::Config;
    use crate::layout::WINDOW;
    use crate::panes::{Kind, Pane};
    use crate::scene::sidebar::Agent;

    fn notes() -> Panes {
        Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
    }

    /// scout, at `status`, running under `profile`, with the profiles `dev`
    /// and `look`.
    fn scout(status: Option<Status>, profile: Option<&str>) -> Agent {
        let profiles = vec!["dev".into(), "look".into()];
        let profile = profile.map(str::to_string);
        Agent { name: "scout".into(), status, profile, profiles, ..Agent::default() }
    }

    #[test]
    fn the_rows_are_its_own_limits_then_its_profiles_the_running_one_marked() {
        let mut canvases = Canvases::new(Client::new("/nonexistent/x.sock"));
        let (panes, pane) = (notes(), notes().focus());
        assert_eq!(canvases.profile_names(&panes, pane), None, "nothing heard");
        let set = |canvases: &mut Canvases, agent| {
            canvases.agents(Some(vec![("notes".into(), agent)]));
        };
        set(&mut canvases, Some(Agent { name: "scout".into(), ..Agent::default() }));
        assert_eq!(canvases.profile_names(&panes, pane), None, "no profiles");
        let names: Vec<String> = [OWN, "dev", "look"].map(String::from).to_vec();
        for (agent, marked) in [
            (scout(Some(Status::Stopped), None), None),
            (scout(Some(Status::Working), Some("look")), Some(2)),
            (scout(None, None), Some(0)),
        ] {
            set(&mut canvases, Some(agent));
            assert_eq!(canvases.profile_names(&panes, pane), Some((names.clone(), marked)));
        }
    }

    #[test]
    fn a_tap_on_a_profile_slings_what_is_new_under_it() {
        let (bin, dev) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let _home = open_notes(daemon);
            let dir = Some(dev.path().to_string_lossy().into_owned());
            let profile = slinger_control::Profile { name: "dev".into(), dir, tools: vec![] };
            daemon.profile_set("notes", profile).unwrap();
            user_stroke(daemon, "notes");
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            canvases.agents(Some(vec![("notes".into(), Some(scout(Some(Status::Stopped), None)))]));
            let theme = Config::default().theme;
            wait_until(|| {
                canvases.poll();
                !canvases.show(&panes, WINDOW.rect(), &theme).is_empty()
            });
            assert!(!canvases.sling_under(&panes, WINDOW.rect(), 9), "no such row");
            assert!(canvases.sling_under(&panes, WINDOW.rect(), 1));
            let message = delivered(dev.path(), "canvas=notes");
            assert!(message.contains("c1 ink"), "{message}");
            assert_eq!(daemon.session_show("scout").unwrap().profile, Some("dev".into()));
            daemon.session_stop("notes").unwrap();
        });
    }
}
