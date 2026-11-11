//! Agent UIs and the daemon. [`fetch`] gets what a pane's UI target shows:
//! `ui_shown` says which version, and for a showing (`pick-slot#12`, made by
//! `ui open`) its id and data; `ui_export` writes that version's bytes to a
//! file of the app's own, which is read and removed (docs/service.md, "How a
//! blob's bytes travel"). [`reply`] sends a showing's reply to its agent.
//! Each blocks until the daemon answers or its client gives up.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::Value;
use slinger_control::Client;

use super::Problem;
use crate::ui::{Code, Shown};

/// What the pane target `target` shows: a showing's id or a UI reference
/// (`name`, `name@3` or a blob id).
///
/// # Errors
///
/// The daemon's, and a problem when its file cannot be read.
pub fn fetch(daemon: &Client, target: &str) -> Result<Shown, Problem> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let shown = daemon.ui_shown(target)?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("slinger-app-ui-{}-{n}", std::process::id()));
    let read = (|| {
        let exported = daemon.ui_export(&shown.ui.hash, &dir)?;
        fs::read(&exported.path).map_err(|e| unreadable(&exported.path, &e))
    })();
    // The directory is the app's own and only this call writes it.
    let _ = fs::remove_dir_all(&dir);
    let ui = shown.ui;
    let code = Code { name: ui.name, version: ui.version, hash: ui.hash, html: read? };
    Ok(Shown { code, open: shown.open, data: shown.data })
}

/// Sends the reply a page's bridge made, its `ui_reply` line, to the agent
/// that opened the showing it names. Returns that showing's id, and the
/// problem to show in its pane when the agent's session did not take it:
/// the reply is kept in the record either way.
///
/// # Errors
///
/// Why the reply cannot go on, for the page: a UI opened by name, which no
/// agent waits on, or the daemon's refusal.
pub fn reply(daemon: &Client, line: &str) -> Result<(String, Option<Problem>), String> {
    let line: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
    let Some(open) = line["open"].as_str() else {
        return Err("no agent opened this UI, so none takes its replies".into());
    };
    let replied = daemon.ui_reply(open, line["value"].clone()).map_err(|e| e.to_string())?;
    let unreachable = (!replied.delivered).then(|| Problem {
        message: format!("{} is not running: the reply is kept in the record", replied.agent),
        hint: Some("start its session with 'slinger session start <WORKSPACE>'".into()),
    });
    Ok((open.to_string(), unreachable))
}

fn unreadable(path: &Path, e: &std::io::Error) -> Problem {
    let hint = Some("check the temporary directory is readable");
    Problem::new(format!("{}: {e}", path.display()), hint)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use slinger_control::UiOpen;

    use super::super::tests::{open_notes, with_daemon};
    use super::*;

    #[test]
    fn fetches_the_version_a_reference_names_by_its_hash() {
        with_daemon(|d| {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("board.html");
            fs::write(&file, "<p>one</p>").unwrap();
            let first = d.ui_put("board", &file, "scout").unwrap().ui;
            fs::write(&file, "<p>two</p>").unwrap();
            d.ui_put("board", &file, "scout").unwrap();

            let latest = fetch(d, "board").unwrap();
            assert_eq!((latest.code.name.as_str(), latest.code.version), ("board", 2));
            assert_eq!(
                (latest.code.html, latest.open, latest.data),
                (b"<p>two</p>".to_vec(), None, Value::Null)
            );
            let old = fetch(d, &first.hash).unwrap().code;
            assert_eq!((old.version, old.hash, old.html), (1, first.hash, b"<p>one</p>".to_vec()));
            assert_eq!(fetch(d, "board@1").unwrap().code.html, b"<p>one</p>");
        });
    }

    #[test]
    fn a_ui_that_is_not_there_is_a_problem_with_a_hint() {
        with_daemon(|d| {
            let problem = fetch(d, "nope").unwrap_err();
            assert!(problem.message.contains("nope"), "{problem:?}");
            assert!(problem.hint.is_some());
        });
    }

    #[test]
    fn a_showing_brings_its_data_and_its_replies_are_kept_for_an_agent_not_running() {
        with_daemon(|d| {
            let _notes = open_notes(d);
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("pick.html");
            fs::write(&file, "<p>pick</p>").unwrap();
            d.ui_put("pick", &file, "scout").unwrap();
            let data = json!({"slots": ["Tue"]});
            let ask = UiOpen {
                ui: "pick".into(),
                workspace: "notes".into(),
                agent: "scout".into(),
                host: "pane".into(),
                data: data.clone(),
                ..UiOpen::default()
            };
            let open = d.ui_open(ask).unwrap().open;
            let shown = fetch(d, &open).unwrap();
            assert_eq!((shown.open.as_deref(), shown.data), (Some(open.as_str()), data));
            assert_eq!(shown.code.html, b"<p>pick</p>");

            let line = json!({"v": 1, "kind": "ui_reply", "open": open, "value": {"slot": "Tue"}});
            let (named, unreachable) = reply(d, &line.to_string()).unwrap();
            assert_eq!(named, open);
            let unreachable = unreachable.unwrap();
            assert!(unreachable.message.starts_with("scout is not running"), "{unreachable:?}");
            assert!(unreachable.hint.is_some());

            let by_name = json!({"v": 1, "open": null, "value": 1}).to_string();
            assert!(reply(d, &by_name).unwrap_err().contains("no agent opened"));
            let gone = json!({"v": 1, "open": "pick#999", "value": 1}).to_string();
            assert!(reply(d, &gone).unwrap_err().contains("not found"));
        });
    }
}
