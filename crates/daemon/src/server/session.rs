//! `session_list`, `session_show` and `status_set`: the sessions as they
//! cross the socket.

use slinger_control::{self as control, RemoteError, Reply, SessionAsk};
use slinger_core::session::Session;

use crate::sessions::SessionError;
use crate::store::Store;

pub fn answer(store: &Store, ask: SessionAsk) -> Result<Reply, RemoteError> {
    match ask {
        SessionAsk::SessionList => list(store),
        SessionAsk::SessionShow { agent } => show(store, &agent),
        SessionAsk::StatusSet { agent, status, note } => set(store, &agent, &status, note),
    }
}

fn list(store: &Store) -> Result<Reply, RemoteError> {
    let sessions = store.sessions().list(store).map_err(|e| remote(&e))?;
    let sessions = sessions.into_iter().map(wire).collect();
    Ok(Reply::SessionList(control::Sessions { sessions }))
}

fn show(store: &Store, agent: &str) -> Result<Reply, RemoteError> {
    let session = store.sessions().show(store, agent).map_err(|e| remote(&e))?;
    Ok(Reply::SessionShow(control::SessionInfo { session: wire(session) }))
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

fn wire(s: Session) -> control::Session {
    let (status, note) = s.status.map_or((None, None), |r| (Some(r.status.name().into()), r.note));
    control::Session {
        agent: s.agent,
        home: s.home,
        deliver: s.deliver.name().into(),
        status,
        note,
    }
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
