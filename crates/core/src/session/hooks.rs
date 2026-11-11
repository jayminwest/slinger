//! Claude Code's hooks (SPEC section 5): how a session's process reports to
//! Slinger without the agent's help, and what the one payload Slinger reads
//! says.
//!
//! The server passes [`settings`] to each process with `--settings`, so the
//! hooks are the process's own and no settings file of the user's changes.
//! Each hook runs the `slinger` CLI, which takes the agent and the socket from
//! the process's environment (`SLINGER_AGENT`, `SLINGER_SOCKET`):
//!
//! - `UserPromptSubmit`: a turn starts, `status set working`.
//! - `Stop`: the turn ends, `status set done`.
//! - `Notification` (`permission_prompt`, `elicitation_dialog`,
//!   `elicitation_url_dialog`): it waits for the user, `status set blocked`.
//! - `PostToolUse` of a file tool: `file touched --hook`, which hands the
//!   hook's payload to the server; [`touched`] reads the file's path from it.
//!
//! No IO: the server starts the process and reads the file.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The tools that write a file, as a `PostToolUse` matcher.
const FILE_TOOLS: &str = "Write|Edit|MultiEdit|NotebookEdit";

/// The settings JSON, for `--settings`, whose hooks run `slinger` (the CLI's
/// path; a bare name is looked up on `PATH`).
#[must_use]
pub fn settings(slinger: &Path) -> String {
    let run = |args: &str| {
        // A hook that exits 2 blocks the prompt or keeps the turn going; any
        // failure of the CLI (2 is `usage`) is made a plain one, which Claude
        // Code shows and goes past.
        let command = format!("{} {args} || exit 1", quote(&slinger.to_string_lossy()));
        json!([{ "type": "command", "command": command }])
    };
    let on = |matcher: &str, args: &str| json!({ "matcher": matcher, "hooks": run(args) });
    let every = |args: &str| json!([{ "hooks": run(args) }]);
    json!({
        "hooks": {
            "UserPromptSubmit": every("status set working"),
            "Stop": every("status set done"),
            "Notification": [
                on("permission_prompt", "status set blocked --note 'waits for permission'"),
                on("elicitation_dialog|elicitation_url_dialog", "status set blocked --note 'waits for input'"),
            ],
            "PostToolUse": [on(FILE_TOOLS, "file touched --hook")],
        }
    })
    .to_string()
}

/// The file a `PostToolUse` hook's payload says a tool wrote: its tool
/// input's `file_path` (`notebook_path` for a notebook), against the
/// payload's `cwd` when relative. `None` for a payload that names no file.
#[must_use]
pub fn touched(payload: &str) -> Option<PathBuf> {
    let payload: Value = serde_json::from_str(payload).ok()?;
    let input = &payload["tool_input"];
    let path = input["file_path"].as_str().or_else(|| input["notebook_path"].as_str());
    let path = Path::new(path.filter(|p| !p.is_empty())?);
    match payload["cwd"].as_str() {
        Some(cwd) if path.is_relative() => Some(Path::new(cwd).join(path)),
        _ => Some(path.to_owned()),
    }
}

/// `text` as one word for a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The commands of `event`'s hooks, by their matcher ("" for none).
    fn commands(settings: &Value, event: &str) -> Vec<(String, String)> {
        let groups = settings["hooks"][event].as_array().unwrap();
        groups
            .iter()
            .flat_map(|g| {
                let matcher = g["matcher"].as_str().unwrap_or_default().to_string();
                g["hooks"].as_array().unwrap().iter().map(move |h| {
                    assert_eq!(h["type"], "command");
                    (matcher.clone(), h["command"].as_str().unwrap().to_string())
                })
            })
            .collect()
    }

    #[test]
    fn each_hook_runs_the_cli_for_its_event_and_never_exits_2() {
        let settings: Value =
            serde_json::from_str(&settings(Path::new("/opt/it's/slinger"))).unwrap();
        let cli = r"'/opt/it'\''s/slinger'";
        let one = |args: &str| format!("{cli} {args} || exit 1");
        assert_eq!(
            commands(&settings, "UserPromptSubmit"),
            [(String::new(), one("status set working"))]
        );
        assert_eq!(commands(&settings, "Stop"), [(String::new(), one("status set done"))]);
        assert_eq!(
            commands(&settings, "Notification"),
            [
                (
                    "permission_prompt".into(),
                    one("status set blocked --note 'waits for permission'")
                ),
                (
                    "elicitation_dialog|elicitation_url_dialog".into(),
                    one("status set blocked --note 'waits for input'")
                ),
            ]
        );
        assert_eq!(
            commands(&settings, "PostToolUse"),
            [("Write|Edit|MultiEdit|NotebookEdit".into(), one("file touched --hook"))]
        );
        assert_eq!(settings["hooks"].as_object().unwrap().len(), 4, "no other event");
    }

    #[test]
    fn a_payload_names_its_file_or_its_notebook_against_its_cwd() {
        let write = r#"{"hook_event_name":"PostToolUse","cwd":"/w","tool_name":"Write","tool_input":{"file_path":"/w/a.txt","content":"x"}}"#;
        assert_eq!(touched(write), Some(PathBuf::from("/w/a.txt")));
        let notebook =
            r#"{"cwd":"/w","tool_name":"NotebookEdit","tool_input":{"notebook_path":"n.ipynb"}}"#;
        assert_eq!(touched(notebook), Some(PathBuf::from("/w/n.ipynb")));
        for nothing in [
            "",
            "not json",
            "[]",
            r#"{"tool_input":{"command":"ls"}}"#,
            r#"{"tool_input":{"file_path":""}}"#,
        ] {
            assert_eq!(touched(nothing), None, "{nothing}");
        }
    }
}
