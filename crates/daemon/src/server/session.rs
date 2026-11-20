//! `session_list`, `session_show`, `status_set`, `session_start`,
//! `session_stop`, `session_send`, `session_listen` and `file_touched`: the
//! sessions as they cross the socket.

use std::time::Duration;

use serde_json::Value;
use slinger_control::{
    self as control, FileTouchedAsk, Listened, RemoteError, SessionListAsk, SessionListenAsk,
    SessionSendAsk, SessionShowAsk, SessionStartAsk, SessionStopAsk, StatusSetAsk, Taken,
};
use slinger_core::record::Body;
use slinger_core::session::listen::Heard;
use slinger_core::session::{Delivery, Session};

use crate::processes::ProcessError;
use crate::sessions::SessionError;
use crate::store::Store;

type Info = Result<control::SessionInfo, RemoteError>;

pub(super) fn start(store: &Store, ask: SessionStartAsk) -> Info {
    let SessionStartAsk { workspace, fresh, profile } = ask;
    info(store, store.sessions().start(store, &workspace, fresh, profile.as_deref()))
}

pub(super) fn stop(store: &Store, SessionStopAsk { workspace }: SessionStopAsk) -> Info {
    info(store, store.sessions().stop(store, &workspace))
}

pub(super) fn send(store: &Store, SessionSendAsk { workspace, text }: SessionSendAsk) -> Info {
    info(store, store.sessions().send(store, &workspace, &text))
}

pub(super) fn listen(store: &Store, ask: SessionListenAsk) -> Result<Listened, RemoteError> {
    let SessionListenAsk { workspace, wait, pid } = ask;
    let wait = Duration::from_millis(wait.min(control::MAX_WAIT_MS));
    let (agent, sling) =
        store.sessions().listen(store, &workspace, wait, pid).map_err(|e| remote(&e))?;
    // The sling is taken: one whose packet cannot be written again still
    // goes, its text naming what the listener cannot read.
    if let Some(sling) = &sling {
        let _restored = super::read::restore(store, sling);
    }
    let sling = sling.map(|Heard { event, canvas, dir, text }| Taken { event, canvas, dir, text });
    Ok(Listened { agent, sling })
}

pub(super) fn touched(
    store: &Store,
    FileTouchedAsk { agent, path, hook }: FileTouchedAsk,
) -> Result<control::FileTouched, RemoteError> {
    let touched = store.sessions().touched(store, &agent, path, hook.as_deref());
    let event = touched.map_err(|e| remote(&e))?;
    let file = event.as_ref().and_then(|e| match &e.body {
        Body::FileTouched(file) => Some(file),
        _ => None,
    });
    Ok(control::FileTouched {
        agent,
        path: file.map(|f| f.path.clone().into()),
        hash: file.map(|f| f.hash.to_string()),
        event: event.as_ref().and_then(|e| e.number),
    })
}

/// A `session_listen` reply that could not be written: the listener is gone,
/// so the sling it took waits again, the oldest.
pub(super) fn unheard(store: &Store, reply: &Result<Value, RemoteError>) {
    let listened = reply.as_ref().ok().and_then(|v| serde_json::from_value(v.clone()).ok());
    if let Some(Listened { agent, sling: Some(taken) }) = listened {
        let Taken { event, canvas, dir, text } = taken;
        store.sessions().listeners().put_back(&agent, Heard { event, canvas, dir, text });
    }
}

pub(super) fn list(store: &Store, _: SessionListAsk) -> Result<control::Sessions, RemoteError> {
    let sessions = store.sessions().list(store).map_err(|e| remote(&e))?;
    let sessions = sessions.into_iter().map(|s| wire(store, s)).collect::<Result<_, _>>()?;
    Ok(control::Sessions { sessions })
}

pub(super) fn show(store: &Store, SessionShowAsk { agent }: SessionShowAsk) -> Info {
    info(store, store.sessions().show(store, &agent))
}

fn info(
    store: &Store,
    session: Result<Session, SessionError>,
) -> Result<control::SessionInfo, RemoteError> {
    Ok(control::SessionInfo { session: wire(store, session.map_err(|e| remote(&e))?)? })
}

pub(super) fn set(store: &Store, ask: StatusSetAsk) -> Result<control::StatusSet, RemoteError> {
    let StatusSetAsk { agent, status, note } = ask;
    let (report, event) =
        store.sessions().set(store, &agent, &status, note).map_err(|e| remote(&e))?;
    Ok(control::StatusSet {
        agent,
        status: report.status.name().to_string(),
        note: report.note,
        event: event.and_then(|e| e.number),
    })
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
    // A listened-for agent's context is its listener's, which the daemon does not see.
    let context = shown.as_ref().and_then(|p| p.context).filter(|_| s.deliver != Delivery::Listen);
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
        context,
    })
}

pub(super) fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
