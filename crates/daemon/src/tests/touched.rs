//! `file_touched` through the real socket: a file by its path or by a Claude
//! Code hook's payload is stored as its path and hash, and its contents go
//! nowhere in the data directory.

use std::path::Path;

use serde_json::json;
use slinger_control::{EventFilter, Touched};
use tempfile::TempDir;

use super::Daemon;

/// Whether any file under `dir` holds `needle`.
fn holds(dir: &Path, needle: &[u8]) -> bool {
    std::fs::read_dir(dir).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        if path.is_dir() {
            return holds(&path, needle);
        }
        let bytes = std::fs::read(&path).unwrap_or_default();
        bytes.windows(needle.len()).any(|w| w == needle)
    })
}

#[test]
fn a_touched_file_is_stored_as_its_path_and_hash_never_its_contents() {
    let (data, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let contents = b"the agent's secret draft, 7f3a9c";
    let file = work.path().canonicalize().unwrap().join("draft.txt");
    std::fs::write(&file, contents).unwrap();

    let touched = client.file_touched("scout", Touched::Path(&file)).unwrap();
    assert_eq!(touched.path.as_deref(), Some(file.as_path()));
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "cwd": work.path().canonicalize().unwrap(),
        "tool_name": "Edit",
        "tool_input": { "file_path": "draft.txt", "old_string": "a", "new_string": "b" },
    });
    let hooked = client.file_touched("scout", Touched::Hook(&payload.to_string())).unwrap();
    assert_eq!(
        (hooked.path.as_ref(), hooked.hash.as_ref()),
        (touched.path.as_ref(), touched.hash.as_ref())
    );
    assert!(hooked.event > touched.event);

    let filter = EventFilter { kind: Some("file_touched".into()), ..EventFilter::default() };
    let events = client.record_list(filter).unwrap();
    assert_eq!(events.len(), 2);
    for event in &events {
        let want = json!({ "path": file, "hash": touched.hash });
        assert_eq!((&event["payload"], &event["source"]), (&want, &json!("scout")));
        assert_eq!(event["tags"]["agent"], "scout");
    }
    assert!(!holds(data.path(), contents), "the record holds no contents");
    let id = client.blob_put(&file).unwrap();
    assert_eq!(touched.hash, Some(id), "the hash is the file's blob id");
    daemon.stop();
}

#[test]
fn a_hook_naming_no_file_touches_none_and_a_bad_path_is_refused() {
    let (data, work) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let bash = json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }).to_string();
    let none = client.file_touched("scout", Touched::Hook(&bash)).unwrap();
    assert_eq!((none.path, none.hash, none.event), (None, None, None));
    assert_eq!(client.record_list(EventFilter::default()).unwrap().len(), 0);

    let missing = work.path().join("gone.txt");
    let err = client.file_touched("scout", Touched::Path(&missing)).unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
    let err = client.file_touched("scout", Touched::Path(Path::new("rel.txt"))).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    let err = client.file_touched(" ", Touched::Path(&missing)).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    let both = r#"{"v":1,"op":"file_touched","agent":"scout","path":"/a","hook":"{}"}"#;
    assert!(daemon.raw(format!("{both}\n").as_bytes()).contains("\"invalid\""));
    daemon.stop();
}
