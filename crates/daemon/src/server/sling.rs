//! `sling_send`: the packet of a canvas's new ink and the user's prompt, as
//! one message to the agent of the canvas's workspace (SPEC section 6, D10).
//!
//! The marker is the canvas's last sling ([`read::last_sling`]). The packet
//! is built, the message made (`sling::message`) and delivered; only then is
//! it stored (R7) and `sling_sent` appended, which moves the marker.
//!
//! A sling goes one of two ways (SPEC section 5, [`Sessions::route`]). To
//! the agent's listener, when one is there: the packet's images are written
//! to its directory, and the message waits as text until a `session_listen`
//! takes it. Else to the agent's process, which the sling starts when none
//! runs (D10); when it cannot be spawned the sling waits for a listener, so
//! it is never refused for want of one. One that cannot be delivered leaves
//! no trace, so the next one sends the same ink. A sling that names a
//! profile goes only to a process under that profile (started under it when
//! none runs, `conflict` under another): it never reaches an agent with
//! other limits, and a listener has none of Slinger's.
//!
//! [`Sessions::route`]: crate::sessions::Sessions::route
//!
//! `sling_backdrops` comes first from a client that can render PDFs: the
//! areas of `pdf` items under the new ink. `sling_send` then reads the
//! files it rendered ([`under`]); the images are drawn over them and their
//! words end the ink's lines. Ink drawn between the two shows on white.

use std::fs;

use slinger_control::{Backdrop, BackdropArea, Backdrops, RemoteError, Reply, SlingAsk, Slung};
use slinger_core::canvas::file::{backdrop, rect_tenths, size_tenths};
use slinger_core::record::{BlobId, Body, Event, NamedBlob, SlingSent, Tags};
use slinger_core::render;
use slinger_core::session::listen::Heard;
use slinger_core::session::{Delivery, stream};
use slinger_core::sling::{self, Listener, Sling, Verb};

use super::{absolute, drawn, read, remote as core, stored, unreadable};
use crate::sessions::SessionError;
use crate::store::{Query, Store};

/// The agent canvas verbs this daemon serves, which the message's note
/// names. A test ties each one to a request the daemon answers.
pub(crate) const VERBS: [Verb; 3] = [Verb::Put, Verb::Crop, Verb::Ui];

pub(super) fn send(store: &Store, ask: SlingAsk) -> Result<Reply, RemoteError> {
    let sessions = store.sessions();
    let _one = sessions.slinging();
    let (workspace, agent) = sessions.on_canvas(store, &ask.canvas).map_err(|e| remote(&e))?;
    let after = read::last_sling(store, &ask.canvas)?;
    let under = under(&ask.backdrops)?;
    let mut packet = store
        .canvases()
        .packet(&ask.canvas, ask.page.as_deref(), after, &ask.select, &under)
        .map_err(|e| drawn(&e))?;
    // The packet is stored as it is here: without `at`.
    let text = packet.text();
    let route = sessions.route(store, &workspace, &agent, ask.profile.as_deref());
    let listens = route.map_err(|e| remote(&e))? == Delivery::Listen;
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
        let sling = Heard { event, canvas, dir, text: message.text() };
        sessions.listeners().push(&agent, sling).map_err(|e| stored(&e))?;
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

/// The areas of PDF pages under the new ink a sling of `canvas` would draw
/// now, after the canvas's last sling.
pub(super) fn backdrops(
    store: &Store,
    canvas: &str,
    page: Option<&str>,
) -> Result<Reply, RemoteError> {
    let after = read::last_sling(store, canvas)?;
    let areas = store.canvases().backdrops(canvas, page, after).map_err(|e| drawn(&e))?;
    let backdrops = areas
        .into_iter()
        .map(|a| BackdropArea {
            item: format!("i{}", a.item.get()),
            hash: a.hash,
            pdfpage: a.pdf_page,
            pagesize: size_tenths(a.page_size),
            crop: a.crop.map(rect_tenths),
            frame: rect_tenths(a.frame),
            area: rect_tenths(a.area),
            width: a.width,
            height: a.height,
        })
        .collect();
    Ok(Reply::SlingBackdrops(Backdrops { backdrops }))
}

/// The backdrops a sling names, read from their files. Nothing is stored
/// when one fails: `not_found` for a file that is not there, `invalid` for
/// pixels of the wrong length or words that do not parse.
fn under(given: &[Backdrop]) -> Result<Vec<render::Backdrop>, RemoteError> {
    let read = |path: &std::path::Path| fs::read(absolute(path)?).map_err(|e| unreadable(path, &e));
    given
        .iter()
        .map(|b| {
            let rgb = read(&b.image)?;
            let words = b.words.as_deref().map(read).transpose()?;
            backdrop(&b.item, b.area, rgb, words.as_deref()).map_err(|e| core(&e))
        })
        .collect()
}

/// The agent's slings, from every canvas: a conversation is the agent's.
fn of_agent(agent: &str) -> Query {
    Query { kind: Some("sling_sent".into()), agent: Some(agent.into()), ..Query::default() }
}

fn remote(error: &SessionError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
