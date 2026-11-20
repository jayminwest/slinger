//! `workspace_open`, `workspace_list`, `workspace_show` and
//! `workspace_close`: the open workspaces as they cross the socket. A
//! workspace whose file the daemon cannot use crosses with its error.

use slinger_control::{
    self as control, RemoteError, WorkspaceCloseAsk, WorkspaceClosed, WorkspaceInfo,
    WorkspaceListAsk, WorkspaceOpenAsk, WorkspaceOpened, WorkspaceShowAsk, Workspaces,
};
use slinger_core::workspace::FILE_NAME;

use super::{kept, session};
use crate::store::Store;
use crate::workspaces::{Broken, Listed, Opened};

pub(super) fn open(
    store: &Store,
    WorkspaceOpenAsk { dir, name }: WorkspaceOpenAsk,
) -> Result<WorkspaceOpened, RemoteError> {
    let workspaces = store.workspaces();
    let (opened, new) =
        workspaces.open(dir.as_deref(), name.as_deref(), store.canvases()).map_err(|e| kept(&e))?;
    Ok(WorkspaceOpened { workspace: wire(opened), new })
}

pub(super) fn list(store: &Store, _: WorkspaceListAsk) -> Result<Workspaces, RemoteError> {
    let all = store.workspaces().list().map_err(|e| kept(&e))?;
    let workspaces = all.into_iter().map(|w| match w {
        Listed::Served(opened) => wire(opened),
        Listed::Broken(broken) => wire_broken(broken),
    });
    Ok(Workspaces { workspaces: workspaces.collect() })
}

pub(super) fn show(
    store: &Store,
    WorkspaceShowAsk { workspace }: WorkspaceShowAsk,
) -> Result<WorkspaceInfo, RemoteError> {
    let opened = store.workspaces().show(&workspace).map_err(|e| kept(&e))?;
    Ok(WorkspaceInfo { workspace: wire(opened) })
}

pub(super) fn close(
    store: &Store,
    WorkspaceCloseAsk { workspace }: WorkspaceCloseAsk,
) -> Result<WorkspaceClosed, RemoteError> {
    let dir = store.sessions().close(store, &workspace).map_err(|e| session::remote(&e))?;
    Ok(WorkspaceClosed { dir })
}

fn wire(Opened { dir, workspace: ws }: Opened) -> control::Workspace {
    let agent = ws.agent.map(|a| control::WorkspaceAgent {
        profiles: a.profiles.into_iter().map(|p| p.name).collect(),
        name: a.name,
        durable: a.durable,
    });
    let roots = ws.roots.into_iter();
    let roots = roots.map(|r| control::WorkspaceRoot {
        machine: r.machine,
        path: r.path,
        primary: r.primary,
    });
    control::Workspace {
        name: ws.name,
        tag: ws.tag,
        canvas: ws.canvas,
        dir,
        agent,
        roots: roots.collect(),
        error: None,
    }
}

/// An open workspace whose file the daemon cannot use, with its error.
fn wire_broken(Broken { dir, name, canvas, error }: Broken) -> control::Workspace {
    let (file, field) = error.file().map_or_else(
        || (dir.join(FILE_NAME), None),
        |(file, field)| (file.to_owned(), field.map(str::to_string)),
    );
    let error = control::WorkspaceFileError {
        code: error.code().into(),
        message: error.to_string(),
        hint: error.hint(),
        file,
        field,
    };
    control::Workspace {
        name: name.unwrap_or_default(),
        tag: String::new(),
        canvas: canvas.unwrap_or_default(),
        dir,
        agent: None,
        roots: Vec::new(),
        error: Some(error),
    }
}
