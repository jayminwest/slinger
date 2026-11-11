//! `session_list`, `session_show`, `status_set`, `session_start`,
//! `session_stop`, `session_send` and `file_touched`: the sessions as they cross the socket.

use slinger_control::{self as control, RemoteError, Reply, SessionAsk};
use slinger_core::record::Body;
use slinger_core::session::Session;

use crate::processes::ProcessError;
use crate::sessions::SessionError;
use crate::store::Store;

pub fn answer(store: &Store, ask: SessionAsk) -> Result<Reply, RemoteError> {
    match ask {
        SessionAsk::SessionList => list(store),
        SessionAsk::SessionShow { agent } => show(store, &agent),
        SessionAsk::StatusSet { agent, status, note } => set(store, &agent, &status, note),
        SessionAsk::SessionStart { workspace } => {
            let session = store.sessions().start(store, &workspace);
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
    let running = shown.filter(|p| p.running);
    let (status, note) = s.status.map_or((None, None), |r| (Some(r.status.name().into()), r.note));
    Ok(control::Session {
        agent: s.agent,
        home: s.home,
        deliver: s.deliver.name().into(),
        status,
        note,
        process,
        pid: running.as_ref().map(|p| p.pid),
        pending: running.map(|p| p.pending),
        answer,
        session_id,
    })
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
