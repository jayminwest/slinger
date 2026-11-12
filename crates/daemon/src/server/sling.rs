//! `sling_send`: the packet of a canvas's new ink and the user's prompt, as
//! one message to the agent of the canvas's workspace (SPEC section 6, D10).
//!
//! The marker is the canvas's last sling ([`read::last_sling`]). The packet
//! is built, the message made (`sling::message`) and delivered to the
//! agent's running process; only then is it stored (R7) and `sling_sent`
//! appended, which moves the marker. A sling that cannot be delivered leaves
//! no trace, so the next one sends the same ink. Slinger never starts an
//! agent for a sling (D10): with no running process it is `unavailable`. A
//! sling that names a profile goes only to a process under that profile
//! (`conflict` otherwise): it never reaches an agent with other limits.

use slinger_control::{RemoteError, Reply, SlingAsk, Slung};
use slinger_core::record::{Body, Event, NamedBlob, SlingSent, Tags};
use slinger_core::session::stream;
use slinger_core::sling::{self, Sling, Verb};

use super::{drawn, read, stored};
use crate::sessions::SessionError;
use crate::store::{Query, Store};

/// The agent canvas verbs this daemon serves, which the message's note
/// names. A test ties each one to a request the daemon answers.
pub(crate) const VERBS: [Verb; 2] = [Verb::Put, Verb::Crop];

pub(super) fn send(store: &Store, ask: SlingAsk) -> Result<Reply, RemoteError> {
    let sessions = store.sessions();
    let _one = sessions.slinging();
    let (workspace, agent) = sessions.on_canvas(store, &ask.canvas).map_err(|e| remote(&e))?;
    let after = read::last_sling(store, &ask.canvas)?;
    let packet = store
        .canvases()
        .packet(&ask.canvas, ask.page.as_deref(), after, &ask.select)
        .map_err(|e| drawn(&e))?;
    let session_id = sessions.processes().session_id(&agent).map_err(|e| remote(&e.into()))?;
    let slings = store.events(&of_agent(&agent)).map_err(|e| stored(&e))?;
    let first = session_id.as_deref().is_none_or(|id| sling::first(&slings, id));
    let message = sling::message(&Sling {
        packet: &packet,
        prompt: &ask.prompt,
        selection: &packet.selected,
        first,
        verbs: &VERBS,
    });
    let line = stream::sling(&message);
    let profile = ask.profile.as_deref();
    sessions.deliver(store, &workspace, &line, profile).map_err(|e| remote(&e))?;

    let id = store.put_blob(packet.text().as_bytes()).map_err(|e| stored(&e))?;
    let mut images = Vec::with_capacity(packet.images.len());
    for image in &packet.images {
        let hash = store.put_blob(&image.png).map_err(|e| stored(&e))?;
        images.push(NamedBlob { name: image.name.clone(), hash });
    }
    let session = session_id.clone();
    let body = Body::SlingSent(SlingSent { prompt: ask.prompt, packet: id, images, session });
    let tags = Tags { agent: Some(agent.clone()), canvas: Some(ask.canvas), ..Tags::on("") };
    let event = store.append(Event::new(0, "slingerd", tags, body)).map_err(|e| stored(&e))?;
    Ok(Reply::SlingSend(Slung {
        event: event.number.unwrap_or_default(),
        packet: id.to_string(),
        from: packet.header.from,
        to: packet.header.to,
        strokes: packet.new_strokes(),
        agent,
        session_id: session_id.unwrap_or_default(),
    }))
}

/// The agent's slings, from every canvas: a conversation is the agent's.
fn of_agent(agent: &str) -> Query {
    Query { kind: Some("sling_sent".into()), agent: Some(agent.into()), ..Query::default() }
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
