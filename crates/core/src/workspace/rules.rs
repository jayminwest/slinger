//! The rules a workspace keeps: on its own, which [`Workspace::read`] checks,
//! and over a set ([`check`]). A broken rule names the field that broke it
//! ([`Rejected`]), so a front end can point at the line to fix.

use std::collections::{HashMap, HashSet};

use super::{FILE_NAME, Workspace, invalid, profile};
use crate::{Error, Result};

/// Why a definition file was refused: the error, and the field that broke a
/// rule (`agent.tools`, `root.path`) when one did. `None` for text that is
/// not a workspace file at all.
#[derive(Debug)]
pub struct Rejected {
    pub field: Option<&'static str>,
    pub error: Error,
}

impl Rejected {
    pub(super) fn at(field: &'static str, error: Error) -> Self {
        Self { field: Some(field), error }
    }
}

impl From<Rejected> for Error {
    fn from(rejected: Rejected) -> Self {
        rejected.error
    }
}

impl Workspace {
    /// The rules for one workspace on its own.
    pub(super) fn check_one(&self) -> std::result::Result<(), Rejected> {
        let ws = &self.name;
        for (field, value) in [("name", &self.name), ("tag", &self.tag), ("canvas", &self.canvas)] {
            if value.trim().is_empty() {
                return Err(Rejected::at(
                    field,
                    invalid(&format!("workspace {ws:?}: {field} is empty")),
                ));
            }
        }
        for root in &self.roots {
            if root.machine.trim().is_empty() {
                let message = format!("workspace {ws:?}: root {:?} has no machine", root.path);
                return Err(Rejected::at("root.machine", invalid(&message)));
            }
            if !root.path.starts_with('/') {
                let message =
                    format!("workspace {ws:?}: root {:?} is not an absolute path", root.path);
                return Err(Rejected::at("root.path", invalid(&message)));
            }
        }
        let primaries = self.roots.iter().filter(|r| r.primary).count();
        if primaries > 1 {
            let message = format!("workspace {ws:?} has {primaries} primary roots; at most one");
            return Err(Rejected::at("root.primary", invalid(&message)));
        }
        let Some(agent) = &self.agent else { return Ok(()) };
        if agent.name.trim().is_empty() {
            let message = format!("workspace {ws:?}: agent name is empty");
            return Err(Rejected::at("agent.name", invalid(&message)));
        }
        if agent.name.contains('#') {
            // `#` parts an agent from its profile in a session's key.
            return Err(Rejected::at(
                "agent.name",
                Error::Invalid {
                    message: format!("workspace {ws:?}: agent name {:?} has a '#'", agent.name),
                    hint: Some("rename the agent without '#' under [agent]".into()),
                },
            ));
        }
        if let Some(dir) = agent.dirs.iter().find(|d| !d.starts_with('/')) {
            let message = format!("workspace {ws:?}: agent dir {dir:?} is not an absolute path");
            return Err(Rejected::at("agent.dirs", invalid(&message)));
        }
        for rule in &agent.tools {
            profile::check_rule(rule).map_err(|e| {
                Rejected::at(
                    "agent.tools",
                    Error::Invalid {
                        message: format!("workspace {ws:?}: agent.tools: {e}"),
                        hint: e.hint().map(|h| format!("{h}; fix or drop it in {FILE_NAME}")),
                    },
                )
            })?;
        }
        profile::check_all(&agent.profiles).map_err(|e| {
            Rejected::at(
                "agent.profile",
                Error::Invalid {
                    message: format!("workspace {ws:?}: {e}"),
                    hint: e.hint().map(str::to_string),
                },
            )
        })?;
        if agent.durable && primaries != 1 {
            return Err(Rejected::at(
                "agent.durable",
                Error::Invalid {
                    message: format!("workspace {ws:?} has a durable agent but no primary root"),
                    hint: Some("mark the agent's home directory `primary = true`".into()),
                },
            ));
        }
        Ok(())
    }
}

/// Checks the rules over a set of workspaces: each one keeps its own rules
/// (a durable agent has exactly one primary root, at most one root is
/// primary); one directory is the primary root of at most one workspace; and
/// one name, and one default canvas, belong to at most one workspace.
///
/// To ask whether a new workspace may join, check the existing ones with it.
///
/// # Errors
///
/// [`Error::Invalid`] when one workspace breaks its own rules.
/// [`Error::Conflict`] when two workspaces share a primary root, a name or a
/// default canvas.
pub fn check(workspaces: &[Workspace]) -> Result<()> {
    let mut owners: HashMap<(&str, &str), &str> = HashMap::new();
    let mut names: HashSet<&str> = HashSet::new();
    let mut canvases: HashMap<&str, &str> = HashMap::new();
    for ws in workspaces {
        ws.check_one()?;
        if !names.insert(&ws.name) {
            return Err(Error::Conflict {
                message: format!("two workspaces are named {:?}", ws.name),
                hint: Some("give one another name: `name` in its file, or --name".into()),
            });
        }
        if let Some(owner) = canvases.insert(&ws.canvas, &ws.name) {
            return Err(Error::Conflict {
                message: format!(
                    "canvas {:?} is already the canvas of workspace {owner:?}",
                    ws.canvas
                ),
                hint: Some(format!("set another `canvas` in the file of workspace {:?}", ws.name)),
            });
        }
        let Some(root) = ws.primary_root() else { continue };
        if let Some(owner) = owners.insert(root.key(), &ws.name) {
            return Err(Error::Conflict {
                message: format!(
                    "{} on {} is already the primary root of workspace {owner:?}",
                    root.path, root.machine
                ),
                hint: Some(format!("open workspace {owner:?}, or pick another directory")),
            });
        }
    }
    Ok(())
}

/// Checks that `ws` may open beside an open workspace whose file cannot be
/// used, in `dir`: the `name` and the `canvas` that file states, when it
/// states them ([`super::peek`]), stay that workspace's.
///
/// # Errors
///
/// [`Error::Conflict`] when `ws` has that name or that default canvas.
pub fn check_reserved(
    ws: &Workspace,
    dir: &str,
    name: Option<&str>,
    canvas: Option<&str>,
) -> Result<()> {
    let taken = if name == Some(ws.name.as_str()) {
        format!("name {:?}", ws.name)
    } else if canvas == Some(ws.canvas.as_str()) {
        format!("canvas {:?}", ws.canvas)
    } else {
        return Ok(());
    };
    Err(Error::Conflict {
        message: format!("{taken} belongs to the open workspace in {dir}, whose file is broken"),
        hint: Some(format!(
            "fix that file, or close it: 'slinger workspace close {dir}'; or open this one under another name"
        )),
    })
}
