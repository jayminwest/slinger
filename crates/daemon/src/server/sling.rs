//! `sling_send`: the packet of a canvas's new ink and the user's prompt, as
//! one message to the agent of the canvas's workspace (SPEC section 6, D10).
//!
//! The marker is the canvas's last sling ([`read::last_sling`]). The packet
//! is built, the message made (`sling::message`) and delivered; only then is
//! it stored (R7) and `sling_sent` appended, which moves the marker.
//!
//! A sling goes one of two ways (SPEC section 5). To the agent's listener,
//! when one is there or no process of the agent's runs: the packet's images
//! are written to its directory, and the message waits as text until a
//! `session_listen` takes it, so a sling is never refused for want of a
//! listener. Else to the agent's running process: one that cannot be
//! delivered leaves no trace, so the next one sends the same ink. Slinger
//! never starts an agent for a sling (D10). A sling that names a profile
//! goes only to a process under that profile (`unavailable` with none
//! running, `conflict` under another): it never reaches an agent with other
//! limits, and a listener has none of Slinger's.

use slinger_control::{RemoteError, Reply, SlingAsk, Slung};
use slinger_core::record::{BlobId, Body, Event, NamedBlob, SlingSent, Tags};
use slinger_core::session::listen::Heard;
use slinger_core::session::{Delivery, stream};
use slinger_core::sling::{self, Listener, Sling, Verb};

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
    let mut packet = store
        .canvases()
        .packet(&ask.canvas, ask.page.as_deref(), after, &ask.select)
        .map_err(|e| drawn(&e))?;
    // The packet is stored as it is here: without `at`.
    let text = packet.text();
    let listens = ask.profile.is_none() && sessions.listens(&agent);
    let dir = store.packet_dir(BlobId::of(text.as_bytes()));
    let session_id = if listens {
        packet.header.at = Some(dir.to_string_lossy().into_owned());
        read::write(&dir, &packet, &packet.text())?;
        None
    } else {
        sessions.processes().session_id(&agent).map_err(|e| remote(&e.into()))?
    };
    let slings = store.events(&of_agent(&agent)).map_err(|e| stored(&e))?;
    // A listener's conversation is not Slinger's to know: each sling of its
    // has the whole note.
    let first = session_id.as_deref().is_none_or(|id| sling::first(&slings, id));
    let message = sling::message(&Sling {
        packet: &packet,
        prompt: &ask.prompt,
        selection: &packet.selected,
        first,
        verbs: &VERBS,
        listener: listens.then_some(Listener { agent: &agent, workspace: &workspace }),
    });
    let tokens = message.tokens();
    if !listens {
        let (line, profile) = (stream::sling(&message), ask.profile.as_deref());
        sessions.deliver(store, &workspace, &line, profile).map_err(|e| remote(&e))?;
    }

    let id = store.put_blob(text.as_bytes()).map_err(|e| stored(&e))?;
    let mut images = Vec::with_capacity(packet.images.len());
    for image in &packet.images {
        let hash = store.put_blob(&image.png).map_err(|e| stored(&e))?;
        images.push(NamedBlob { name: image.name.clone(), hash });
    }
    let session = session_id.clone();
    let body = Body::SlingSent(SlingSent { prompt: ask.prompt, packet: id, images, session });
    let canvas = ask.canvas;
    let tags = Tags { agent: Some(agent.clone()), canvas: Some(canvas.clone()), ..Tags::on("") };
    let event = store.append(Event::new(0, "slingerd", tags, body)).map_err(|e| stored(&e))?;
    let event = event.number.unwrap_or_default();
    if listens {
        sessions.listeners().push(&agent, Heard { event, canvas, dir, text: message.text() });
    }
    let deliver = if listens { Delivery::Listen } else { Delivery::Process };
    Ok(Reply::SlingSend(Slung {
        event,
        packet: id.to_string(),
        from: packet.header.from,
        to: packet.header.to,
        strokes: packet.new_strokes(),
        agent,
        session_id: session_id.unwrap_or_default(),
        deliver: Some(deliver.name().into()),
        tokens: Some(tokens),
    }))
}

/// The agent's slings, from every canvas: a conversation is the agent's.
fn of_agent(agent: &str) -> Query {
    Query { kind: Some("sling_sent".into()), agent: Some(agent.into()), ..Query::default() }
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
