//! `profile_list`, `profile_set` and `profile_remove`: a workspace agent's
//! profiles as they cross the socket. The core checks a profile and changes
//! the workspace; the file is written whole before the reply.

use slinger_control::{self as control, ProfileAsk, Profiles, RemoteError, Reply};
use slinger_core::workspace::Profile;

use super::{kept, remote};
use crate::store::Store;
use crate::workspaces::Opened;

pub fn answer(store: &Store, ask: ProfileAsk) -> Result<Reply, RemoteError> {
    let workspaces = store.workspaces();
    let opened = match ask {
        ProfileAsk::ProfileList { workspace } => {
            workspaces.show(&workspace).map_err(|e| kept(&e))?
        }
        ProfileAsk::ProfileSet { workspace, profile } => {
            let control::Profile { name, dir, tools } = profile;
            // Checked first, so a relative dir is refused before it is looked for.
            let checked = Profile::new(name, dir, tools).map_err(|e| remote(&e))?;
            let dir = checked.dir.as_deref().map(directory).transpose()?;
            let profile = Profile::new(checked.name, dir, checked.tools).map_err(|e| remote(&e))?;
            workspaces.edit(&workspace, |ws| ws.set_profile(profile)).map_err(|e| kept(&e))?.0
        }
        ProfileAsk::ProfileRemove { workspace, name } => {
            workspaces.edit(&workspace, |ws| ws.remove_profile(&name)).map_err(|e| kept(&e))?.0
        }
    };
    Ok(Reply::Profiles(wire(opened)))
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
