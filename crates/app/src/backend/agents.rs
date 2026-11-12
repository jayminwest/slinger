//! The open workspaces and their agents as the sidebar shows them: each
//! agent's status read against its process, the profile it runs under,
//! and the names of the profiles it has.

use slinger_control::Client;

use crate::backend::Problem;
use crate::scene::sidebar::{Agent, Process, Status, Workspace};

/// The open workspaces, by name, each with its agent's status from the
/// daemon's sessions: what the agent last said, read against its process
/// ([`Status::of`]); the profile a running process runs under, and the
/// agent's profiles. Blocks until the daemon answers or its client gives up.
pub fn workspaces(daemon: &Client) -> Result<Vec<Workspace>, Problem> {
    let sessions = daemon.session_list()?;
    let open = daemon.workspace_list()?;
    let agent = |name: String, profiles: Vec<String>| {
        let session = sessions.iter().find(|s| s.agent == name);
        let said = session.and_then(|s| Status::parse(s.status.as_deref()?));
        let started = session.is_some_and(|s| s.session_id.is_some());
        let process = Process::named(session.and_then(|s| s.process.as_deref()), started);
        let pending = session.and_then(|s| s.pending).unwrap_or(0);
        let answered = session.is_some_and(|s| s.answer.is_some());
        let running = process == Process::Running;
        Agent {
            status: Status::of(said, process, pending, answered),
            note: session.and_then(|s| s.note.clone()),
            profile: session.and_then(|s| s.profile.clone()).filter(|_| running),
            profiles,
            name,
        }
    };
    Ok(open
        .into_iter()
        .map(|w| Workspace {
            // Richer display of a broken file: slinger-44766e follow-up.
            name: if w.error.is_some() { format!("{} (invalid file)", w.name) } else { w.name },
            canvas: w.canvas,
            agent: w.agent.map(|a| agent(a.name, a.profiles)),
        })
        .collect())
}
