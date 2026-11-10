//! The agent's canvas verbs as they cross the socket: `canvas_put`, and the
//! `reply_landed` a put appends when it answers a sling.

use slinger_control::{self as control, AgentAsk, RemoteError, Reply};
use slinger_core::canvas::file::{Placed, Put};
use slinger_core::record::{Event, Tags};
use slinger_core::{session, sling};

use super::{drawn, remote, stored};
use crate::store::{Query, Store};

pub(super) fn answer(store: &Store, ask: AgentAsk) -> Result<Reply, RemoteError> {
    match ask {
        AgentAsk::CanvasPut(put) => {
            let what = match put.what {
                control::Put::Text(text) => Put::Text(text),
                control::Put::Box([w, h]) => Put::Box { w, h },
                control::Put::Arrow([from, to]) => Put::Arrow { from, to },
            };
            canvas_put(store, &put.canvas, &put.agent, &put.beside, &what)
        }
    }
}

/// Puts the item, then appends `reply_landed` when the agent was slung
/// something on this canvas (see [`sling::landed`]).
fn canvas_put(
    store: &Store,
    canvas: &str,
    agent: &str,
    beside: &str,
    what: &Put,
) -> Result<Reply, RemoteError> {
    session::check_agent(agent).map_err(|e| remote(&e))?;
    let placed = store.canvases().put(canvas, agent, beside, what).map_err(|e| drawn(&e))?;
    let Placed { written, item, page, frame: [x, y, w, h] } = placed;
    let tags = Tags { agent: Some(agent.into()), canvas: Some(canvas.into()), ..Tags::on("") };
    let query = Query {
        kind: Some("sling_sent".into()),
        agent: tags.agent.clone(),
        canvas: tags.canvas.clone(),
        ..Query::default()
    };
    let slings = store.events(&query).map_err(|e| stored(&e))?;
    let event = match sling::landed(&slings, vec![item.clone()]) {
        Some(body) => {
            store.append(Event::new(0, agent, tags, body)).map_err(|e| stored(&e))?.number
        }
        None => None,
    };
    let seq = written.seq.first().copied().unwrap_or_default();
    Ok(Reply::CanvasPut(control::CanvasPlaced { item, page, x, y, w, h, seq, event }))
}
