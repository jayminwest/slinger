//! `ui_put`, `ui_list`, `ui_show` and `ui_export`: the catalog of agent UIs
//! as it crosses the socket. The core decides the versions; the record
//! holds them as `ui_put` events and the code as blobs. `ui_put` decides
//! its version inside the store's write (`Store::append_if`), so two puts
//! cannot take one number.

use std::fs;

use slinger_control::{
    self as control, Exported, RemoteError, UiExportAsk, UiListAsk, UiPutAsk, UiShowAsk, Uis,
};
use slinger_core::record::{Body, Event, Tags};
use slinger_core::ui;

use super::{absolute, missing, remote, stored, unreadable, unusable};
use crate::store::{Query, Store};

/// Stores the code as a blob, then appends the next version unless the
/// name's latest already holds it. The append decides on the log as it is
/// inside the write, so two puts at once cannot take one version number.
pub(super) fn put(store: &Store, ask: UiPutAsk) -> Result<control::UiPut, RemoteError> {
    let UiPutAsk { name, path, source } = ask;
    let bytes = fs::read(absolute(&path)?).map_err(|e| unreadable(&path, &e))?;
    ui::check(&name, &bytes).map_err(|e| remote(&e))?;
    let hash = store.put_blob(&bytes).map_err(|e| stored(&e))?;
    let size = bytes.len() as u64;
    let put = store
        .append_if(&ui_events(), |events| match ui::Catalog::new(events).put(&name, hash, size) {
            ui::Put::New(body) => Ok(Event::new(0, source, Tags::on(""), Body::UiPut(body))),
            ui::Put::Same(version) => Err(version),
        })
        .map_err(|e| stored(&e))?;
    let (version, new) = match put {
        Ok(event) => (ui::Version::of(&event), true),
        Err(version) => (Some(version), false),
    };
    let version = version.ok_or_else(|| RemoteError::new("internal", "ui_put not stored", None))?;
    Ok(control::UiPut { ui: wire_ui(&version), new })
}

pub(super) fn list(store: &Store, _: UiListAsk) -> Result<Uis, RemoteError> {
    Ok(uis(catalog(store)?.latest()))
}

pub(super) fn show(store: &Store, UiShowAsk { name }: UiShowAsk) -> Result<Uis, RemoteError> {
    Ok(uis(catalog(store)?.versions(&name).map_err(|e| remote(&e))?))
}

/// Writes the version `ask.ui` names into `ask.dir`, creating it.
pub(super) fn export(
    store: &Store,
    UiExportAsk { ui, dir }: UiExportAsk,
) -> Result<Exported, RemoteError> {
    let catalog = catalog(store)?;
    let version = catalog.resolve(&ui).map_err(|e| remote(&e))?;
    let hash = version.hash;
    let bytes = store.blob(hash).map_err(|e| stored(&e))?.ok_or_else(|| missing(hash))?;
    fs::create_dir_all(absolute(&dir)?).map_err(|e| unusable(&dir, &e))?;
    let path = dir.join(version.file_name());
    fs::write(&path, &bytes).map_err(|e| unusable(&path, &e))?;
    Ok(Exported { ui: wire_ui(version), path })
}

fn ui_events() -> Query {
    Query { kind: Some("ui_put".into()), ..Query::default() }
}

pub(super) fn catalog(store: &Store) -> Result<ui::Catalog, RemoteError> {
    Ok(ui::Catalog::new(&store.events(&ui_events()).map_err(|e| stored(&e))?))
}

fn uis<'a>(versions: impl IntoIterator<Item = &'a ui::Version>) -> Uis {
    Uis { uis: versions.into_iter().map(wire_ui).collect() }
}

pub(super) fn wire_ui(v: &ui::Version) -> control::Ui {
    control::Ui {
        name: v.name.clone(),
        version: v.version,
        hash: v.hash.to_string(),
        bytes: v.bytes,
        source: v.source.clone(),
        time: v.time,
    }
}
