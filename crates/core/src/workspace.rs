//! Workspaces (SPEC.md section 10): what one is, its definition file, and the
//! rules a set of them must keep.
//!
//! This module owns the definition file's format (`docs/workspace.md`). No
//! other module knows it is TOML. It does no file IO: callers read and write
//! the bytes and hand the text here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The definition file's name, in a workspace's primary root.
pub const FILE_NAME: &str = "slinger-workspace.toml";

/// The format version this module writes for a new workspace.
pub const VERSION: u32 = 1;

/// The canvas a workspace opens on when its file names none.
pub const DEFAULT_CANVAS: &str = "main";

const HINT: &str = "see docs/workspace.md for the workspace file format";

/// A workspace: a name, a pane layout, zero or more directories, an optional
/// agent, its tag in the record and its default canvas.
///
/// Fields a reader does not know are kept and written back unchanged, so an
/// older Slinger never drops what a newer one wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    version: u32,
    /// Shown to the user. Not blank.
    pub name: String,
    /// The workspace's tag on events in the record. Defaults to the name.
    #[serde(default)]
    pub tag: String,
    /// The canvas where you talk to the workspace's agent.
    #[serde(default = "default_canvas")]
    pub canvas: String,
    /// The agent that works here, if any. It belongs to the primary root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
    /// The directories, on any machine. At most one is primary.
    #[serde(default, rename = "root", skip_serializing_if = "Vec::is_empty")]
    pub roots: Vec<Root>,
    /// How the panes are arranged. Opaque here.
    #[serde(default, skip_serializing_if = "PaneLayout::is_unset")]
    pub layout: PaneLayout,
    #[serde(flatten)]
    extra: toml::Table,
}

/// The agent a workspace talks to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    /// The agent's name, as the server's session list knows it.
    pub name: String,
    /// Whether a harness keeps the agent alive across restarts (D20). A
    /// durable agent's home is the workspace's primary root.
    #[serde(default, skip_serializing_if = "is_false")]
    pub durable: bool,
    #[serde(flatten)]
    extra: toml::Table,
}

/// One directory of a workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Root {
    /// The machine the directory is on.
    pub machine: String,
    /// An absolute path on that machine.
    pub path: String,
    /// The primary root holds the definition file and is the agent's home.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
    #[serde(flatten)]
    extra: toml::Table,
}

/// The pane layout: a versioned value whose meaning the app shell owns. The
/// core only stores it and writes it back. The default is "unset": version
/// 0 and nothing in it, so the app picks a layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaneLayout {
    version: u32,
    #[serde(flatten)]
    value: toml::Table,
}

impl PaneLayout {
    /// The layout's own format version. 0 when unset.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    fn is_unset(&self) -> bool {
        *self == Self::default()
    }
}

impl Agent {
    /// An agent with this name.
    #[must_use]
    pub fn new(name: impl Into<String>, durable: bool) -> Self {
        Self { name: name.into(), durable, extra: toml::Table::new() }
    }
}

impl Root {
    /// A directory that is not the primary root.
    #[must_use]
    pub fn new(machine: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            machine: machine.into(),
            path: path.into(),
            primary: false,
            extra: toml::Table::new(),
        }
    }

    /// The primary root.
    #[must_use]
    pub fn primary(machine: impl Into<String>, path: impl Into<String>) -> Self {
        Self { primary: true, ..Self::new(machine, path) }
    }

    /// Two roots are the same directory when the machine and the path match.
    /// A trailing `/` does not count.
    #[must_use]
    pub fn same_dir(&self, other: &Self) -> bool {
        self.key() == other.key()
    }

    fn key(&self) -> (&str, &str) {
        let path = self.path.trim_end_matches('/');
        (self.machine.as_str(), if path.is_empty() { "/" } else { path })
    }
}

impl Workspace {
    /// A workspace with no roots, no agent and an unset layout. Its tag is
    /// its name and it opens on [`DEFAULT_CANVAS`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            version: VERSION,
            tag: name.clone(),
            name,
            canvas: default_canvas(),
            agent: None,
            roots: Vec::new(),
            layout: PaneLayout::default(),
            extra: toml::Table::new(),
        }
    }

    /// Reads a definition file's text. The result keeps the rules for one
    /// workspace (see [`check`]).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when the text is not a workspace file, or breaks a
    /// rule.
    pub fn parse(text: &str) -> Result<Self> {
        let mut ws: Self = toml::from_str(text).map_err(|e| invalid(e.message().trim()))?;
        if ws.version == 0 {
            return Err(invalid("version must be 1 or more"));
        }
        if ws.tag.is_empty() {
            ws.tag.clone_from(&ws.name);
        }
        ws.check_one()?;
        Ok(ws)
    }

    /// The definition file's text. Unknown fields read by [`Workspace::parse`]
    /// are written back.
    ///
    /// # Panics
    ///
    /// Never: every field is a string, a bool, an integer or a TOML table.
    #[must_use]
    pub fn to_toml(&self) -> String {
        #[expect(
            clippy::expect_used,
            reason = "strings, bools, ints and TOML tables always serialize"
        )]
        toml::to_string(self).expect("a workspace serializes to TOML")
    }

    /// The format version the file was written with.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The primary root, if the workspace has one.
    #[must_use]
    pub fn primary_root(&self) -> Option<&Root> {
        self.roots.iter().find(|r| r.primary)
    }

    /// The rules for one workspace on its own.
    fn check_one(&self) -> Result<()> {
        let ws = &self.name;
        for (field, value) in [("name", &self.name), ("tag", &self.tag), ("canvas", &self.canvas)] {
            if value.trim().is_empty() {
                return Err(invalid(&format!("workspace {ws:?}: {field} is empty")));
            }
        }
        for root in &self.roots {
            if root.machine.trim().is_empty() {
                return Err(invalid(&format!(
                    "workspace {ws:?}: root {:?} has no machine",
                    root.path
                )));
            }
            if !root.path.starts_with('/') {
                return Err(invalid(&format!(
                    "workspace {ws:?}: root {:?} is not an absolute path",
                    root.path
                )));
            }
        }
        let primaries = self.roots.iter().filter(|r| r.primary).count();
        if primaries > 1 {
            return Err(invalid(&format!(
                "workspace {ws:?} has {primaries} primary roots; at most one"
            )));
        }
        if let Some(agent) = &self.agent {
            if agent.name.trim().is_empty() {
                return Err(invalid(&format!("workspace {ws:?}: agent name is empty")));
            }
            if agent.durable && primaries != 1 {
                return Err(Error::Invalid {
                    message: format!("workspace {ws:?} has a durable agent but no primary root"),
                    hint: Some("mark the agent's home directory `primary = true`".into()),
                });
            }
        }
        Ok(())
    }
}

/// Checks the rules over a set of workspaces: each one keeps its own rules
/// (a durable agent has exactly one primary root, at most one root is
/// primary), and one directory is the primary root of at most one workspace.
///
/// To ask whether a new workspace may join, check the existing ones with it.
///
/// # Errors
///
/// [`Error::Invalid`] when one workspace breaks its own rules.
/// [`Error::Conflict`] when two workspaces share a primary root.
pub fn check(workspaces: &[Workspace]) -> Result<()> {
    let mut owners: HashMap<(&str, &str), &str> = HashMap::new();
    for ws in workspaces {
        ws.check_one()?;
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

fn invalid(message: &str) -> Error {
    Error::Invalid { message: message.to_string(), hint: Some(HINT.into()) }
}

fn default_canvas() -> String {
    DEFAULT_CANVAS.into()
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests;
