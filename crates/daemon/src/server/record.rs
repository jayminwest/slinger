//! `record_append`, `record_list`, `blob_put` and `blob_get`: this
//! machine's record and its blobs as they cross the socket. Blob bytes
//! travel as files on this machine.

use std::fs;

use slinger_control::{
    Appended, BlobGetAsk, BlobPutAsk, EventFilter, Events, NewEvent, RemoteError, Stored, Written,
};
use slinger_core::record::{self, BlobId, Body, Event, Tags};

use super::{absolute, missing, remote, stored, unreadable, unusable};
use crate::store::{Query, Store};

pub(super) fn append(store: &Store, new: NewEvent) -> Result<Appended, RemoteError> {
    let event = store.append(event(new).map_err(|e| remote(&e))?).map_err(|e| stored(&e))?;
    Ok(Appended { event: record::to_value(&event) })
}

pub(super) fn list(store: &Store, filter: EventFilter) -> Result<Events, RemoteError> {
    let events = store.events(&query(filter)).map_err(|e| stored(&e))?;
    Ok(Events { events: events.iter().map(record::to_value).collect() })
}

pub(super) fn blob_put(
    store: &Store,
    BlobPutAsk { path }: BlobPutAsk,
) -> Result<Stored, RemoteError> {
    let bytes = fs::read(absolute(&path)?).map_err(|e| unreadable(&path, &e))?;
    let id = store.put_blob(&bytes).map_err(|e| stored(&e))?;
    Ok(Stored { id: id.to_string() })
}

pub(super) fn blob_get(
    store: &Store,
    BlobGetAsk { id, path }: BlobGetAsk,
) -> Result<Written, RemoteError> {
    let id: BlobId = id.parse().map_err(|e| remote(&e))?;
    let bytes = store.blob(id).map_err(|e| stored(&e))?.ok_or_else(|| missing(id))?;
    fs::write(absolute(&path)?, &bytes).map_err(|e| unusable(&path, &e))?;
    Ok(Written { size: bytes.len() as u64 })
}

/// The event a writer asks for. The store sets its number, time and machine.
fn event(new: NewEvent) -> slinger_core::Result<Event> {
    let body = Body::new(&new.kind, new.payload)?;
    let tags = Tags {
        machine: String::new(),
        workspace: new.tags.workspace,
        agent: new.tags.agent,
        canvas: new.tags.canvas,
    };
    Ok(Event::new(0, new.source, tags, body))
}

fn query(filter: EventFilter) -> Query {
    Query {
        from: filter.after.map(|n| n.saturating_add(1)),
        kind: filter.kind,
        machine: filter.machine,
        workspace: filter.workspace,
        agent: filter.agent,
        canvas: filter.canvas,
        ..Query::default()
    }
}
