//! Feedback through the real socket: notes go to the data directory's
//! `feedback.jsonl` (the store the tests open names no other), come back
//! newest first, and leave the open list once resolved.

use slinger_control::FeedbackNote;

use super::Daemon;

#[test]
fn a_note_is_saved_listed_and_resolved_through_the_socket() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let path = client.feedback_path().unwrap();
    assert_eq!(path, daemon.data.as_ref().unwrap().path().join("feedback.jsonl"));
    assert_eq!(client.feedback_list(false, None).unwrap().entries, []);

    let note = FeedbackNote {
        text: " the lasso is slow ".into(),
        workspace: Some("notes".into()),
        tool: Some("lasso".into()),
        selected: Some(3),
        ..FeedbackNote::default()
    };
    let first = client.feedback_add(note).unwrap();
    assert_eq!(
        (first.path.as_path(), first.entry.text.as_str()),
        (path.as_path(), "the lasso is slow")
    );
    assert_eq!((first.entry.tool.as_deref(), first.entry.selected), (Some("lasso"), Some(3)));
    assert_eq!(first.entry.canvas, None);
    let plain = FeedbackNote { text: "second".into(), ..FeedbackNote::default() };
    client.feedback_add(plain).unwrap();
    let line = std::fs::read_to_string(&path).unwrap();
    let first_line = line.lines().next().unwrap();
    assert!(first_line.ends_with(r#""workspace":"notes","tool":"lasso","selected":3}"#), "{line}");

    let listed = client.feedback_list(false, Some(1)).unwrap().entries;
    assert_eq!(listed.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["second"]);
    let done = client.feedback_resolve(&first.entry.ts, Some("slinger-1234".into())).unwrap();
    assert_eq!(
        (done.resolves.as_str(), done.note.as_deref()),
        (first.entry.ts.as_str(), Some("slinger-1234"))
    );
    let open = client.feedback_list(false, None).unwrap().entries;
    assert!(open.iter().all(|e| e.resolved.is_none()), "{open:?}");
    let all = client.feedback_list(true, None).unwrap().entries;
    assert!(
        all.iter()
            .any(|e| e.resolved.as_ref().and_then(|r| r.note.as_deref()) == Some("slinger-1234"))
    );

    let again = client.feedback_resolve(&first.entry.ts, None).unwrap_err();
    assert_eq!(again.code(), "conflict");
    assert_eq!(client.feedback_resolve("never", None).unwrap_err().code(), "not_found");
    let blank = client.feedback_add(FeedbackNote::default()).unwrap_err();
    assert_eq!((blank.code(), blank.hint().is_some()), ("invalid", true));
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 3, "refusals add nothing");
    daemon.stop();
}
