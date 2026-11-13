//! `session_list`, `session_show`, `status_set`, `session_start`,
//! `session_stop`, `session_send`, `session_listen` and `file_touched`: the
//! sessions as they cross the socket.

use std::time::Duration;

use slinger_control::{self as control, Listened, RemoteError, Reply, SessionAsk, Taken};
use slinger_core::record::Body;
use slinger_core::session::listen::Heard;
use slinger_core::session::{Delivery, Session};

use crate::processes::ProcessError;
use crate::sessions::SessionError;
use crate::store::Store;

pub fn answer(store: &Store, ask: SessionAsk) -> Result<Reply, RemoteError> {
    match ask {
        SessionAsk::SessionList => list(store),
        SessionAsk::SessionShow { agent } => show(store, &agent),
        SessionAsk::StatusSet { agent, status, note } => set(store, &agent, &status, note),
        SessionAsk::SessionStart { workspace, fresh, profile } => {
            let session = store.sessions().start(store, &workspace, fresh, profile.as_deref());
            Ok(Reply::SessionStart(info(store, session)?))
        }
        SessionAsk::SessionStop { workspace } => {
            let session = store.sessions().stop(store, &workspace);
            Ok(Reply::SessionStop(info(store, session)?))
        }
        SessionAsk::SessionSend { workspace, text } => {
            let session = store.sessions().send(store, &workspace, &text);
            Ok(Reply::SessionSend(info(store, session)?))
        }
        SessionAsk::SlingSend(ask) => super::sling::send(store, ask),
        SessionAsk::SlingBackdrops { canvas, page } => {
            super::sling::backdrops(store, &canvas, page.as_deref())
        }
        SessionAsk::SessionListen { workspace, wait } => {
            let wait = Duration::from_millis(wait.min(control::MAX_WAIT_MS));
            let (agent, sling) =
                store.sessions().listen(store, &workspace, wait).map_err(|e| remote(&e))?;
            let sling =
                sling.map(|Heard { event, canvas, dir, text }| Taken { event, canvas, dir, text });
            Ok(Reply::SessionListen(Listened { agent, sling }))
        }
        SessionAsk::FileTouched { agent, path, hook } => {
            let touched = store.sessions().touched(store, &agent, path, hook.as_deref());
            let event = touched.map_err(|e| remote(&e))?;
            let file = event.as_ref().and_then(|e| match &e.body {
                Body::FileTouched(file) => Some(file),
                _ => None,
            });
            Ok(Reply::FileTouched(control::FileTouched {
                agent,
                path: file.map(|f| f.path.clone().into()),
                hash: file.map(|f| f.hash.to_string()),
                event: event.as_ref().and_then(|e| e.number),
            }))
        }
    }
}

/// A `session_listen` reply that could not be written: the listener is gone,
/// so the sling it took waits again, the oldest.
pub(super) fn unheard(store: &Store, reply: &Result<Reply, RemoteError>) {
    if let Ok(Reply::SessionListen(Listened { agent, sling: Some(taken) })) = reply {
        let Taken { event, canvas, dir, text } = taken.clone();
        store.sessions().listeners().put_back(agent, Heard { event, canvas, dir, text });
    }
}

fn list(store: &Store) -> Result<Reply, RemoteError> {
    let sessions = store.sessions().list(store).map_err(|e| remote(&e))?;
    let sessions = sessions.into_iter().map(|s| wire(store, s)).collect::<Result<_, _>>()?;
    Ok(Reply::SessionList(control::Sessions { sessions }))
}

fn show(store: &Store, agent: &str) -> Result<Reply, RemoteError> {
    Ok(Reply::SessionShow(info(store, store.sessions().show(store, agent))?))
}

fn info(
    store: &Store,
    session: Result<Session, SessionError>,
) -> Result<control::SessionInfo, RemoteError> {
    Ok(control::SessionInfo { session: wire(store, session.map_err(|e| remote(&e))?)? })
}

fn set(
    store: &Store,
    agent: &str,
    status: &str,
    note: Option<String>,
) -> Result<Reply, RemoteError> {
    let (report, event) =
        store.sessions().set(store, agent, status, note).map_err(|e| remote(&e))?;
    Ok(Reply::StatusSet(control::StatusSet {
        agent: agent.to_string(),
        status: report.status.name().to_string(),
        note: report.note,
        event: event.and_then(|e| e.number),
    }))
}

/// A session as it crosses the socket, with its agent's process.
fn wire(store: &Store, s: Session) -> Result<control::Session, RemoteError> {
    let processes = store.sessions().processes();
    let failed = |e: ProcessError| remote(&SessionError::from(e));
    let session_id = processes.session_id(&s.agent).map_err(failed)?;
    let shown = processes.shown(&s.agent);
    let process = shown.as_ref().map(|p| if p.running { "running" } else { "exited" }.into());
    let answer = shown.as_ref().and_then(|p| p.answer.clone());
    let profile = shown.as_ref().and_then(|p| p.profile.clone());
    let running = shown.filter(|p| p.running);
    let (status, note) = s.status.map_or((None, None), |r| (Some(r.status.name().into()), r.note));
    // A listened-for agent's slings are its listener's, not its process's.
    let listener = store.sessions().listeners().shown(&s.agent);
    let listener = listener.filter(|_| s.deliver == Delivery::Listen);
    Ok(control::Session {
        agent: s.agent,
        home: s.home,
        deliver: s.deliver.name().into(),
        listening: listener.map(|l| l.attached),
        status,
        note,
        process,
        pid: running.as_ref().map(|p| p.pid),
        pending: listener.map(|l| l.pending).or(running.map(|p| p.pending)),
        answer,
        session_id,
        profile,
    })
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
