//! `ui_open`, `ui_shown` and `ui_reply`: showings of agent UIs as they
//! cross the socket. The core decides what a showing is and what a reply
//! sends; a showing lives in the record as its `ui_opened` event, and its
//! pane names it by id.

use serde_json::Value;
use slinger_control::{self as control, RemoteError, Reply, UiAsk, UiOpen};
use slinger_core::record::{Body, Event, Tags};
use slinger_core::session::{self, stream};
use slinger_core::ui::showing::{self, Showing};
use slinger_core::workspace::panes::{Axis, Kind, Pane};

use super::{catalog, kept, pane, remote, stored, wire_ui};
use crate::store::{Query, Store};

pub fn answer(store: &Store, ask: UiAsk) -> Result<Reply, RemoteError> {
    match ask {
        UiAsk::UiOpen(ask) => open(store, ask),
        UiAsk::UiShown { ui } => shown(store, &ui),
        UiAsk::UiReply { open, value } => reply(store, &open, value),
    }
}

/// Records the showing, then opens its pane. Everything that could refuse
/// the pane is checked first, so a refused open records nothing.
fn open(store: &Store, ask: UiOpen) -> Result<Reply, RemoteError> {
    session::check_agent(&ask.agent).map_err(|e| remote(&e))?;
    let version = catalog(store)?.resolve(&ask.ui).map_err(|e| remote(&e))?.clone();
    let body = showing::open(&version, &ask.host, ask.data).map_err(|e| remote(&e))?;
    let (opened, mut layout, ()) =
        store.workspaces().panes(&ask.workspace, |_| Ok(())).map_err(|e| kept(&e))?;
    let axis = if ask.below { Axis::Column } else { Axis::Row };
    let trial = Pane::new(Kind::Ui, &version.name).map_err(|e| remote(&e))?;
    layout.open(trial, ask.beside.map(|n| n as usize), axis).map_err(|e| remote(&e))?;
    let tags = Tags {
        agent: Some(ask.agent.clone()),
        workspace: Some(opened.workspace.tag),
        ..Tags::on("")
    };
    let event = Event::new(0, ask.agent, tags, Body::UiOpened(body));
    let event = store.append(event).map_err(|e| stored(&e))?;
    let showing = Showing::of(&event).ok_or_else(|| internal("ui_opened not numbered"))?;
    let shows = Pane::new(Kind::Ui, &showing.open).map_err(|e| remote(&e))?;
    let (opened, _, pane, _) = pane::open(store, &ask.workspace, shows, ask.beside, ask.below)?;
    Ok(Reply::UiOpen(control::UiOpened {
        open: showing.open,
        ui: wire_ui(&version),
        workspace: opened.workspace.name,
        pane,
        event: event.number.unwrap_or_default(),
    }))
}

/// What a pane's UI target shows: a showing, with its data and agent, or
/// the version a UI reference names.
fn shown(store: &Store, target: &str) -> Result<Reply, RemoteError> {
    let catalog = catalog(store)?;
    let (reference, showing) = match showing::number(target) {
        Some(number) => {
            let showing = find(store, target, number)?;
            (format!("{}@{}", showing.name, showing.version), Some(showing))
        }
        None => (target.to_string(), None),
    };
    let version = catalog.resolve(&reference).map_err(|e| remote(&e))?;
    let (open, data, agent) = match showing {
        Some(s) => (Some(s.open), s.data, Some(s.agent)),
        None => (None, Value::Null, None),
    };
    Ok(Reply::UiShown(control::UiShown { ui: wire_ui(version), open, data, agent }))
}

/// Records the reply, counted against its showing inside the record's
/// write, then delivers its line to the agent's session if it runs.
fn reply(store: &Store, open: &str, value: Value) -> Result<Reply, RemoteError> {
    let number = showing::number(open).ok_or_else(|| {
        let hint = "a showing's id is <name>#<number>, as 'slinger ui open' prints it".into();
        RemoteError::new("invalid", format!("`{open}` is not a showing's id"), Some(hint))
    })?;
    let showing = find(store, open, number)?;
    let tags = Tags {
        agent: Some(showing.agent.clone()),
        workspace: showing.workspace.clone(),
        ..Tags::on("")
    };
    let replies = Query { kind: Some("ui_reply".into()), ..Query::default() };
    let event = store
        .append_if(&replies, |events| {
            let body = showing.reply(showing.sent(events), value)?;
            Ok(Event::new(0, "user", tags, Body::UiReply(body)))
        })
        .map_err(|e| stored(&e))?
        .map_err(|e: slinger_core::Error| remote(&e))?;
    let Body::UiReply(sent) = &event.body else { return Err(internal("ui_reply not stored")) };
    // The reply is in the record either way; an agent whose session does
    // not run gets none of it now, and the app says so.
    let line = stream::message(&showing::line(sent)).map_err(|e| remote(&e))?;
    let delivered = store.sessions().processes().send(&showing.agent, &line).is_ok();
    Ok(Reply::UiReply(control::UiReplied {
        event: event.number.unwrap_or_default(),
        agent: showing.agent,
        delivered,
    }))
}

/// The showing `open` names: event `number`, a `ui_opened` of that name.
fn find(store: &Store, open: &str, number: u64) -> Result<Showing, RemoteError> {
    let query = Query { from: Some(number), to: Some(number), ..Query::default() };
    let events = store.events(&query).map_err(|e| stored(&e))?;
    let found = events.iter().filter_map(Showing::of).find(|s| s.open == open);
    found.ok_or_else(|| {
        let what = format!("showing {open}");
        let hint = Some("an agent opens one with 'slinger ui open'".into());
        remote(&slinger_core::Error::NotFound { what, hint })
    })
}

fn internal(what: &str) -> RemoteError {
    RemoteError::new("internal", what, None)
}
