//! The panes the server keeps for the window's workspace (SPEC section 5:
//! the window holds no durable state). Every change goes to the server, and
//! the window follows the server's layout, whoever changed it: a
//! [`super::Watch`] asks for [`layout`] on its thread, and the window sends
//! its own splits and closes with [`open`] and [`close`]. Each blocks until
//! the daemon answers or its client gives up.

use slinger_control::{Client, PaneKind, PaneLayout, PaneTree};

use super::Problem;
use crate::panes::{Kind, Pane};

/// The server's panes for `wanted` (the config's workspace), else for the
/// first open workspace.
///
/// # Errors
///
/// The daemon's, and a problem when no workspace is open.
pub fn layout(daemon: &Client, wanted: Option<&str>) -> Result<PaneLayout, Problem> {
    let workspace = if let Some(wanted) = wanted {
        wanted.to_string()
    } else {
        let first = daemon.workspace_list()?.into_iter().next().ok_or_else(|| {
            let hint = Some("open one: 'slinger workspace open <DIR>'");
            Problem::new("no workspace is open".into(), hint)
        })?;
        first.name
    };
    Ok(daemon.pane_list(&workspace)?)
}

/// Opens a pane on `workspace` that shows `pane`, beside pane number
/// `beside` (below it with `below`); the new layout and the new pane's
/// number.
///
/// # Errors
///
/// The daemon's.
pub fn open(
    daemon: &Client,
    workspace: &str,
    pane: &Pane,
    beside: u32,
    below: bool,
) -> Result<(PaneTree, u32), Problem> {
    let kind = match pane.kind {
        Kind::Canvas => PaneKind::Canvas,
        Kind::Ui => PaneKind::Ui,
    };
    let layout = daemon.pane_open(workspace, kind, &pane.name, Some(beside), below)?;
    let number = layout.pane.unwrap_or(beside);
    Ok((layout.layout, number))
}

/// Closes pane number `number` of `workspace`; the new layout.
///
/// # Errors
///
/// The daemon's.
pub fn close(daemon: &Client, workspace: &str, number: u32) -> Result<PaneTree, Problem> {
    Ok(daemon.pane_close(workspace, number)?.layout)
}

#[cfg(test)]
mod tests {
    use super::super::tests::with_daemon;
    use super::*;

    fn words(layout: &PaneTree) -> Vec<String> {
        layout.panes().iter().map(|p| format!("{}:{}", p.pane, p.target)).collect()
    }

    #[test]
    fn follows_the_servers_panes_and_sends_its_own_changes() {
        with_daemon(|d| {
            d.workspace_open(None, Some("notes")).unwrap();
            let first = layout(d, None).unwrap();
            assert_eq!(
                (first.workspace.as_str(), words(&first.layout)),
                ("notes", vec!["1:notes".to_string()])
            );

            // Another client, as `slinger pane open` from a terminal.
            d.pane_open("notes", PaneKind::Canvas, "notes", None, false).unwrap();
            assert_eq!(words(&layout(d, Some("notes")).unwrap().layout), ["1:notes", "2:notes"]);

            let pane = Pane { kind: Kind::Canvas, name: "notes".into() };
            let (tree, number) = open(d, "notes", &pane, 1, true).unwrap();
            assert_eq!((words(&tree).len(), number), (3, 2));
            assert_eq!(layout(d, None).unwrap().layout, tree, "the server keeps it");
            assert_eq!(words(&close(d, "notes", 2).unwrap()).len(), 2);
            assert_eq!(words(&close(d, "notes", 9).unwrap()).len(), 2, "already gone");
        });
    }

    #[test]
    fn a_workspace_that_is_not_open_or_none_at_all_is_a_problem() {
        with_daemon(|d| {
            assert_eq!(layout(d, None).unwrap_err().message, "no workspace is open");
            let problem = layout(d, Some("nope")).unwrap_err();
            assert_eq!(problem.message, "open workspace \"nope\" not found");
            assert!(problem.hint.is_some());
        });
    }

    #[test]
    fn no_daemon_is_a_problem_that_says_how_to_start_it() {
        let problem = layout(&Client::new("/nonexistent/x.sock"), None).unwrap_err();
        assert!(problem.hint.unwrap().starts_with("start it with '"));
    }
}
