//! Closing a workspace: the one place that decides what happens to its
//! agent (slinger-0f6923). Every session verb finds the agent through an
//! open workspace, so a process left running past the close could no longer
//! be reached, stopped or listed.

use std::path::PathBuf;

use slinger_core::session;

use super::{Result, Sessions, rest};
use crate::store::Store;

impl Sessions {
    /// Closes the open workspace `which` names, as the workspaces'
    /// `close`, and returns the directory of its file. Its agent's process
    /// ends, as [`Sessions::stop`] ends it, unless another open workspace
    /// still has that agent: then it runs on, reachable through that one.
    /// A workspace whose file the daemon cannot read closes as well.
    ///
    /// # Errors
    ///
    /// `not_found` for no such open workspace, the error of writing the
    /// list, and the record's error when the sling the agent's listener
    /// worked on cannot be forgotten (the workspace is closed by then).
    pub fn close(&self, store: &Store, which: &str) -> Result<PathBuf> {
        let workspaces = store.workspaces();
        let agent = workspaces.show(which).ok().and_then(|o| {
            let agent = session::agent_of(&o.workspace).ok()?.to_string();
            let others = workspaces.usable().ok()?;
            let kept = others.iter().any(|other| {
                other.dir != o.dir && session::agent_of(&other.workspace).ok() == Some(&agent)
            });
            (!kept).then_some(agent)
        });
        let dir = workspaces.close(which)?;
        if let Some(agent) = agent {
            self.processes.stop(&agent);
            rest(&mut self.statuses(), &agent);
            self.listeners.release(&agent)?;
        }
        Ok(dir)
    }
}
