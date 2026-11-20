//! `profile_list`, `profile_set` and `profile_remove`: a workspace agent's
//! profiles as they cross the socket. The core checks a profile and changes
//! the workspace; the file is written whole before the reply.

use slinger_control::{
    self as control, ProfileListAsk, ProfileRemoveAsk, ProfileSetAsk, Profiles, RemoteError,
};
use slinger_core::workspace::Profile;

use super::{kept, remote};
use crate::store::Store;
use crate::workspaces::Opened;

type Reply = Result<Profiles, RemoteError>;

pub(super) fn list(store: &Store, ProfileListAsk { workspace }: ProfileListAsk) -> Reply {
    Ok(wire(store.workspaces().show(&workspace).map_err(|e| kept(&e))?))
}

pub(super) fn set(store: &Store, ProfileSetAsk { workspace, profile }: ProfileSetAsk) -> Reply {
    let control::Profile { name, dir, tools } = profile;
    // Checked first, so a relative dir is refused before it is looked for.
    let checked = Profile::new(name, dir, tools).map_err(|e| remote(&e))?;
    let dir = checked.dir.as_deref().map(directory).transpose()?;
    let profile = Profile::new(checked.name, dir, checked.tools).map_err(|e| remote(&e))?;
    let edited = store.workspaces().edit(&workspace, |ws| ws.set_profile(profile));
    Ok(wire(edited.map_err(|e| kept(&e))?.0))
}

pub(super) fn remove(
    store: &Store,
    ProfileRemoveAsk { workspace, name }: ProfileRemoveAsk,
) -> Reply {
    let edited = store.workspaces().edit(&workspace, |ws| ws.remove_profile(&name));
    Ok(wire(edited.map_err(|e| kept(&e))?.0))
}

/// A profile's directory as one path: there, a directory, links resolved.
fn directory(dir: &str) -> Result<String, RemoteError> {
    let real = std::fs::canonicalize(dir).ok().filter(|d| d.is_dir()).ok_or_else(|| {
        let hint = "pass a directory that is there on slingerd's machine".to_string();
        RemoteError::new("not_found", format!("directory {dir} not found"), Some(hint))
    })?;
    real.to_str().map(str::to_string).ok_or_else(|| {
        let hint = "rename the directory, or pass another".to_string();
        RemoteError::new("invalid", format!("{} is not UTF-8", real.display()), Some(hint))
    })
}

fn wire(Opened { workspace: ws, .. }: Opened) -> Profiles {
    let profiles = ws.profiles().iter().map(|p| control::Profile {
        name: p.name.clone(),
        dir: p.dir.clone(),
        tools: p.tools.clone(),
    });
    Profiles { workspace: ws.name.clone(), profiles: profiles.collect() }
}
