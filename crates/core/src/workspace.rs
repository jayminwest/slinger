//! Workspaces (SPEC.md section 10): what one is, its definition file, and the
//! rules a set of them must keep.
//!
//! This module owns the definition file's format (`docs/workspace.md`). No
//! other module knows it is TOML. It does no file IO: callers read and write
//! the bytes and hand the text here. [`panes`] owns the layout's text.

pub mod panes;
pub mod profile;
mod rules;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};
pub use profile::Profile;
pub use rules::{Rejected, check, check_reserved};

/// The definition file's name, in a workspace's primary root.
pub const FILE_NAME: &str = "slinger-workspace.toml";

/// The format version this module writes for a new workspace.
pub const VERSION: u32 = 1;

/// The canvas a workspace opens on when its file names none.
pub const DEFAULT_CANVAS: &str = "main";

/// The longest id [`slug`] makes: a canvas id's limit.
const SLUG_MAX: usize = 64;

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
    /// How the panes are arranged: read and changed through
    /// [`Workspace::panes`] and [`Workspace::set_panes`].
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
    /// Whether its session loads the owner's own Claude Code settings and
    /// instructions (the user scope), as a session started by hand does. On
    /// by default (slinger-a1c1db), so only `false` is written.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub user_config: bool,
    /// More directories on this machine its session may read, beyond the
    /// workspace's roots. Absolute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dirs: Vec<String>,
    /// More Claude Code permission rules its session may use, beyond the
    /// `slinger` CLI: `Edit`, `Bash(git *)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Named limits its session can run under instead: a working directory
    /// and rules of their own ([`profile`]).
    #[serde(default, rename = "profile", skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
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

/// The pane layout as the file stores it: a version and its fields. The
/// default is "unset": version 0 and nothing in it. Version
/// [`panes::VERSION`] holds the panes' text in `panes`; any other field, and
/// any other version, is kept and written back unread.
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
        Self {
            name: name.into(),
            durable,
            user_config: true,
            dirs: Vec::new(),
            tools: Vec::new(),
            profiles: Vec::new(),
            extra: toml::Table::new(),
        }
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

    /// A new workspace as Slinger makes one: like [`Workspace::new`], but its
    /// default canvas is [`slug`] of its name, so two workspaces get two
    /// canvases.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        Self { canvas: slug(&name), ..Self::new(name) }
    }

    /// A new workspace for the directory `path` on `machine`, its primary
    /// root, as [`Workspace::named`]. Its name is `name`, or else the
    /// directory's last part.
    #[must_use]
    pub fn for_dir(machine: &str, path: &str, name: Option<&str>) -> Self {
        let trimmed = path.trim_end_matches('/');
        let path = if trimmed.is_empty() { "/" } else { trimmed };
        let last = path.rsplit('/').next().filter(|part| !part.is_empty()).unwrap_or(path);
        let mut ws = Self::named(name.unwrap_or(last));
        ws.roots.push(Root::primary(machine, path));
        ws
    }

    /// Reads a definition file's text. The result keeps the rules for one
    /// workspace (see [`check`]).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] when the text is not a workspace file, or breaks a
    /// rule.
    pub fn parse(text: &str) -> Result<Self> {
        Ok(Self::read(text)?)
    }

    /// [`Workspace::parse`], but a refusal also names the field that broke
    /// a rule, when one did.
    ///
    /// # Errors
    ///
    /// As [`Workspace::parse`].
    pub fn read(text: &str) -> std::result::Result<Self, Rejected> {
        let mut ws: Self = toml::from_str(text)
            .map_err(|e| Rejected { field: None, error: invalid(&syntax(text, &e)) })?;
        if ws.version == 0 {
            return Err(Rejected::at("version", invalid("version must be 1 or more")));
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

    /// The workspace's panes: the layout's, or one pane on its canvas when
    /// the layout is unset or not one this version reads (another version,
    /// or text that is not a layout).
    #[must_use]
    pub fn panes(&self) -> panes::Layout {
        let text = self.layout.value.get("panes").and_then(toml::Value::as_str);
        let read = text.filter(|_| self.layout.version == panes::VERSION);
        read.and_then(panes::Layout::decode).unwrap_or_else(|| {
            panes::Layout::new(panes::Pane {
                kind: panes::Kind::Canvas,
                target: self.canvas.clone(),
            })
        })
    }

    /// Stores `layout` as the workspace's panes. The layout's other fields
    /// stay when it is the version this module writes; a layout of another
    /// version is replaced whole.
    pub fn set_panes(&mut self, layout: &panes::Layout) {
        if self.layout.version != panes::VERSION {
            self.layout = PaneLayout { version: panes::VERSION, value: toml::Table::new() };
        }
        self.layout.value.insert("panes".into(), layout.encode().into());
    }

    /// The primary root, if the workspace has one.
    #[must_use]
    pub fn primary_root(&self) -> Option<&Root> {
        self.roots.iter().find(|r| r.primary)
    }
}

/// What a definition file says its name and default canvas are, read
/// leniently: from a file [`Workspace::read`] refuses too, so the workspace
/// can still be listed and named. Only what the file says: no default. Never
/// serve a workspace from it.
#[must_use]
pub fn peek(text: &str) -> (Option<String>, Option<String>) {
    let Ok(table) = toml::from_str::<toml::Table>(text) else { return (None, None) };
    let field = |key: &str| table.get(key).and_then(toml::Value::as_str).map(str::to_string);
    (field("name"), field("canvas"))
}

/// The tag of the workspace an event belongs to: the workspace whose default
/// canvas is `canvas`, else the one whose agent is `agent`. `None` when no
/// workspace matches, or more than one does.
#[must_use]
pub fn tag_for<'a>(
    workspaces: &'a [Workspace],
    canvas: Option<&str>,
    agent: Option<&str>,
) -> Option<&'a str> {
    let only = |mut found: Vec<&'a Workspace>| match (found.pop(), found.is_empty()) {
        (Some(ws), true) => Some(ws.tag.as_str()),
        _ => None,
    };
    let on_canvas =
        canvas.and_then(|c| only(workspaces.iter().filter(|w| w.canvas == c).collect()));
    on_canvas.or_else(|| {
        let agent = agent?;
        only(
            workspaces
                .iter()
                .filter(|w| w.agent.as_ref().is_some_and(|a| a.name == agent))
                .collect(),
        )
    })
}

/// An id made from a workspace's name, for its default canvas and for the
/// directory of a workspace with no root: `a-z`, `0-9`, `-` and `_`, at most
/// 64 long. A run of other characters becomes one `-`. A name with none of
/// them gives `workspace`.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().map(|c| c.to_ascii_lowercase()) {
        if out.len() >= SLUG_MAX {
            break;
        }
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() { "workspace".into() } else { out.to_owned() }
}

/// A TOML error as one line, with the line of the file it is on.
fn syntax(text: &str, e: &toml::de::Error) -> String {
    let message = e.message().trim();
    let Some(span) = e.span() else { return message.to_string() };
    let line = text.get(..span.start).map_or(0, |before| before.matches('\n').count()) + 1;
    format!("line {line}: {message}")
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

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests;
