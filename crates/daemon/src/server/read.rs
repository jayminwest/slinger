//! `canvas_read`: a canvas page's packet, stored as blobs (R7) and written
//! into a directory for an agent to read.

use std::fs;
use std::path::Path;

use slinger_control::{CanvasPacket, PacketAsk, PacketImage, RemoteError, Reply};
use slinger_core::packet::Packet;
use slinger_core::record::{BlobId, Body};
use slinger_core::session::listen::Heard;

use super::{absolute, drawn, missing, remote, stored, unusable};
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
    let files = packet.images.iter().map(|i| (i.name.as_str(), i.png.as_slice()));
    put(dir, files.chain([(TEXT_FILE, text.as_bytes())]))
}

/// Writes the packet of a listener's sling into its directory again from
/// the record, when a file of it is not there (the directory was deleted,
/// say while the daemon was down): the text, with `at`, and each image, as
/// `sling_sent` stored them. A packet whose files are all there is left as
/// it is.
pub(super) fn restore(store: &Store, sling: &Heard) -> Result<(), RemoteError> {
    let query = Query { from: Some(sling.event), to: Some(sling.event), ..Query::default() };
    let events = store.events(&query).map_err(|e| stored(&e))?;
    let Some(Body::SlingSent(sent)) = events.into_iter().next().map(|e| e.body) else {
        let what = format!("the record has no sling_sent {}", sling.event);
        return Err(RemoteError::new("internal", what, None));
    };
    let names = sent.images.iter().map(|i| i.name.as_str()).chain([TEXT_FILE]);
    if names.into_iter().all(|name| sling.dir.join(name).is_file()) {
        return Ok(());
    }
    let blob = |id: BlobId| store.blob(id).map_err(|e| stored(&e))?.ok_or_else(|| missing(id));
    let text = String::from_utf8_lossy(&blob(sent.packet)?).into_owned();
    let mut packet = Packet::read(&text).map_err(|e| remote(&e))?;
    packet.header.at = Some(sling.dir.to_string_lossy().into_owned());
    let mut images = Vec::with_capacity(sent.images.len());
    for image in &sent.images {
        images.push((image.name.as_str(), blob(image.hash)?));
    }
    let text = packet.text();
    let files = images.iter().map(|(name, png)| (*name, png.as_slice()));
    put(&sling.dir, files.chain([(TEXT_FILE, text.as_bytes())]))
}

/// Writes each named file into `dir`, creating it.
fn put<'a>(
    dir: &Path,
    files: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Result<(), RemoteError> {
    fs::create_dir_all(dir).map_err(|e| unusable(dir, &e))?;
    for (name, bytes) in files {
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
