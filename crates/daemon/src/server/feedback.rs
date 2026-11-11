//! `feedback_add`, `feedback_list`, `feedback_resolve` and `feedback_path`:
//! notes about slinger as they cross the socket. The core decides what a
//! note and a resolution are; the store's feedback file holds them.

use slinger_control::{
    FeedbackAdded, FeedbackAsk, FeedbackEntries, FeedbackEntry, FeedbackFile, FeedbackNote,
    FeedbackResolution, FeedbackResolved, RemoteError, Reply,
};
use slinger_core::feedback::{Context, Entry};

use crate::feedback::FeedbackError;
use crate::store::Store;

pub fn answer(store: &Store, ask: FeedbackAsk) -> Result<Reply, RemoteError> {
    let feedback = store.feedback();
    let path = feedback.path().to_path_buf();
    match ask {
        FeedbackAsk::FeedbackAdd(note) => {
            let (text, context) = context(note);
            let entry = wire(feedback.add(&text, context).map_err(|e| noted(&e))?);
            Ok(Reply::FeedbackAdd(FeedbackAdded { path, entry }))
        }
        FeedbackAsk::FeedbackList { all, limit } => {
            let limit = limit.map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
            let entries = feedback.list(all, limit).map_err(|e| noted(&e))?;
            let entries = entries.into_iter().map(wire).collect();
            Ok(Reply::FeedbackList(FeedbackEntries { path, entries }))
        }
        FeedbackAsk::FeedbackResolve { ts, note } => {
            let done = feedback.resolve(&ts, note).map_err(|e| noted(&e))?;
            let (resolves, ts, note) = (done.resolves, done.resolved.ts, done.resolved.note);
            Ok(Reply::FeedbackResolve(FeedbackResolution { path, resolves, ts, note }))
        }
        FeedbackAsk::FeedbackPath => Ok(Reply::FeedbackPath(FeedbackFile { path })),
    }
}

fn context(note: FeedbackNote) -> (String, Context) {
    let FeedbackNote { text, workspace, canvas, page, tool, selected } = note;
    (text, Context { workspace, canvas, page, tool, selected })
}

fn wire(entry: Entry) -> FeedbackEntry {
    let Context { workspace, canvas, page, tool, selected } = entry.context;
    let resolved = entry.resolved.map(|r| FeedbackResolved { ts: r.ts, note: r.note });
    let (ts, text, build) = (entry.ts, entry.text, entry.build);
    FeedbackEntry { ts, text, build, workspace, canvas, page, tool, selected, resolved }
}

/// A feedback error as it crosses the socket.
fn noted(error: &FeedbackError) -> RemoteError {
    RemoteError::new(error.code(), error.to_string(), error.hint())
}
