//! `canvas_read`: a canvas page's packet, stored as blobs (R7) and written
//! into a directory for an agent to read.

use std::fs;
use std::path::Path;

use slinger_control::{CanvasPacket, PacketAsk, PacketImage, RemoteError, Reply};
use slinger_core::packet::Packet;
use slinger_core::record::{BlobId, Body};

use super::{absolute, drawn, stored, unusable};
use crate::store::{Query, Store};

/// The name of the packet's text in its directory.
const TEXT_FILE: &str = "packet.txt";

/// Builds the packet, stores its text (without `at`) and each image as
/// blobs, then writes the text (with `at`) and the images into `ask.out` or
/// the store's directory for it.
pub(super) fn packet(store: &Store, ask: PacketAsk) -> Result<Reply, RemoteError> {
    let after = match ask.after {
        Some(after) => after,
        None => last_sling(store, &ask.canvas)?,
    };
    let canvases = store.canvases();
    let mut packet = canvases
        .packet(&ask.canvas, ask.page.as_deref(), after, &ask.select, &[])
        .map_err(|e| drawn(&e))?;
    let id = store.put_blob(packet.text().as_bytes()).map_err(|e| stored(&e))?;
    let mut images = Vec::with_capacity(packet.images.len());
    for image in &packet.images {
        let blob = store.put_blob(&image.png).map_err(|e| stored(&e))?;
        images.push(PacketImage { name: image.name.clone(), id: blob.to_string() });
    }
    let dir = match ask.out {
        Some(out) => absolute(&out)?.to_owned(),
        None => store.packet_dir(id),
    };
    packet.header.at = Some(dir.to_string_lossy().into_owned());
    let text = packet.text();
    write(&dir, &packet, &text)?;
    Ok(Reply::CanvasRead(CanvasPacket { packet: id.to_string(), dir, text, images }))
}

/// Writes the text and the images into `dir`, creating it.
pub(super) fn write(dir: &Path, packet: &Packet, text: &str) -> Result<(), RemoteError> {
    fs::create_dir_all(dir).map_err(|e| unusable(dir, &e))?;
    let files = packet.images.iter().map(|i| (i.name.as_str(), i.png.as_slice()));
    for (name, bytes) in files.chain([(TEXT_FILE, text.as_bytes())]) {
        let path = dir.join(name);
        fs::write(&path, bytes).map_err(|e| unusable(&path, &e))?;
    }
    Ok(())
}

/// Where the canvas's last sling ended: the `to` of the packet its
/// `sling_sent` event names. 0 (everything is new) before the first sling.
pub(super) fn last_sling(store: &Store, canvas: &str) -> Result<u64, RemoteError> {
    let query =
        Query { kind: Some("sling_sent".into()), canvas: Some(canvas.into()), ..Query::default() };
    let events = store.events(&query).map_err(|e| stored(&e))?;
    let Some(Body::SlingSent(sling)) = events.last().map(|e| &e.body) else { return Ok(0) };
    let bytes = store.blob(sling.packet).map_err(|e| stored(&e))?;
    let text = bytes.as_deref().map(String::from_utf8_lossy);
    match text.as_deref().map(Packet::read) {
        Some(Ok(packet)) => Ok(packet.header.to),
        Some(Err(e)) => Err(unreadable(canvas, sling.packet, &e.to_string())),
        None => Err(unreadable(canvas, sling.packet, "the record does not hold it")),
    }
}

fn unreadable(canvas: &str, packet: BlobId, why: &str) -> RemoteError {
    let what = format!("the last sling on canvas {canvas} names packet {packet}: {why}");
    let hint = "pass --after with the operation number to read from".into();
    RemoteError::new("invalid", what, Some(hint))
}
