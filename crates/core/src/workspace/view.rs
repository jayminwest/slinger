//! Where each pane of a workspace looks, kept by the server on this machine
//! so a window opens again where it was left (docs/service.md, "Panes"):
//! the canvas and page a pane showed, the canvas point at its top left and
//! its zoom, and which pane had the focus.
//!
//! This module owns the views' rules and their text, the views file the
//! daemon keeps in its data directory: `{"v":1,"workspaces":{"<dir>":
//! {"focus":2,"panes":{"1":{"canvas":"notes","page":"p2","x":0,"y":0,
//! "zoom":1}}}}}`, keyed by the directory of each workspace's file and then
//! by pane id. It does no IO. Text that cannot be read is no views, never
//! an error: views are where a window looks, and losing them costs a pan.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// The version of the views text this module reads and writes.
pub const VERSION: u64 = 1;

/// Where one pane looks: the canvas and page it shows, the canvas point at
/// its top left, and its zoom (1 is 100%).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneView {
    pub canvas: String,
    /// The page; none for the canvas's first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl PaneView {
    /// Whether the view can be kept: finite numbers and a zoom above 0.
    ///
    /// # Errors
    ///
    /// `invalid`, with a hint, for a number that is not finite or a zoom of
    /// 0 or less.
    pub fn check(&self) -> Result<()> {
        if ![self.x, self.y, self.zoom].iter().all(|n| n.is_finite()) {
            return Err(invalid("a view's x, y and zoom must be finite numbers"));
        }
        if self.zoom <= 0.0 {
            return Err(invalid("a view's zoom must be above 0"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> Error {
    let hint = "send the canvas point at the pane's top left and a zoom such as 1 for 100%";
    Error::Invalid { message: message.into(), hint: Some(hint.into()) }
}

/// One workspace's views: the pane that has the focus, and each pane's
/// view, by pane id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Views {
    /// The id of the pane that has the focus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    panes: BTreeMap<u32, PaneView>,
}

impl Views {
    /// Keeps `view` as pane `id`'s, in place of the one it had.
    ///
    /// # Errors
    ///
    /// As [`PaneView::check`]; the views stay as they were.
    pub fn set(&mut self, id: u32, view: PaneView) -> Result<()> {
        view.check()?;
        self.panes.insert(id, view);
        Ok(())
    }

    /// Drops the views of the panes whose ids are not in `ids`, and the
    /// focus when it names none of them.
    pub fn keep(&mut self, ids: &[u32]) {
        self.panes.retain(|id, _| ids.contains(id));
        self.focus = self.focus.filter(|id| ids.contains(id));
    }

    /// Gives pane `to` the view pane `from` has, if it has one.
    pub fn copy(&mut self, from: u32, to: u32) {
        if let Some(view) = self.panes.get(&from).cloned() {
            self.panes.insert(to, view);
        }
    }

    /// Pane `id`'s view, only when it is of `canvas`, the canvas the pane
    /// shows now: a view of another canvas does not apply.
    #[must_use]
    pub fn view_of(&self, id: u32, canvas: &str) -> Option<&PaneView> {
        self.panes.get(&id).filter(|view| view.canvas == canvas)
    }

    /// Whether there is nothing to keep.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.focus.is_none() && self.panes.is_empty()
    }
}

/// The views of every workspace on this machine, by the directory of each
/// workspace's file: what the views file holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kept {
    workspaces: BTreeMap<String, Views>,
}

impl Kept {
    /// The views in `text`. Text of another version, or that cannot be
    /// read, is no views; a workspace or a view in it that cannot be read,
    /// or breaks a rule, is left out alone.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let Ok(doc) = serde_json::from_str::<Value>(text) else { return Self::default() };
        if doc["v"].as_u64() != Some(VERSION) {
            return Self::default();
        }
        let Some(all) = doc["workspaces"].as_object() else { return Self::default() };
        let workspaces = all.iter().filter_map(|(dir, views)| Some((dir.clone(), one(views)?)));
        Self { workspaces: workspaces.collect() }
    }

    /// The text of the views file: one line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let doc = serde_json::json!({ "v": VERSION, "workspaces": self.workspaces });
        format!("{doc}\n")
    }

    /// The views of the workspace whose file is in `dir`; none yet is
    /// empty.
    #[must_use]
    pub fn of(&self, dir: &str) -> Views {
        self.workspaces.get(dir).cloned().unwrap_or_default()
    }

    /// Keeps `views` as those of the workspace whose file is in `dir`.
    /// Empty views leave the workspace out.
    pub fn put(&mut self, dir: &str, views: Views) {
        if views.is_empty() {
            self.workspaces.remove(dir);
        } else {
            self.workspaces.insert(dir.to_string(), views);
        }
    }
}

/// One workspace's views from the file, each view that keeps the rules.
fn one(value: &Value) -> Option<Views> {
    let focus = value.get("focus").and_then(Value::as_u64).and_then(|f| u32::try_from(f).ok());
    let mut views = Views { focus, panes: BTreeMap::new() };
    for (id, view) in value.get("panes").and_then(Value::as_object).into_iter().flatten() {
        let view = serde_json::from_value::<PaneView>(view.clone()).ok();
        if let (Ok(id), Some(view)) = (id.parse(), view)
            && view.check().is_ok()
        {
            views.panes.insert(id, view);
        }
    }
    (!views.is_empty()).then_some(views)
}

#[cfg(test)]
mod tests;
