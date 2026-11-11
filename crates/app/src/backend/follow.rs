//! Following a canvas (docs/service.md, "The change feed"): a thread of its
//! own shows one page, then waits on the change feed and hands each change
//! to the main thread, which takes it with [`Follower::news`] and never
//! waits. The daemon holds each feed request until a change or [`WAIT`]
//! passes, so a change reaches the app as soon as the main thread looks.
//!
//! A follower never gives up: when the daemon does not answer, it says so
//! and tries again every [`RETRY`], starting over from the page, so the pane
//! comes back by itself when the daemon does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use slinger_control::{CanvasFeed, CanvasPage, Client, RemoteError};

use super::Problem;

/// How long one feed request waits for a change.
const WAIT: Duration = Duration::from_secs(25);
/// How long to wait before asking a daemon that did not answer again.
const RETRY: Duration = Duration::from_secs(1);

/// What a follower heard, in order.
#[derive(Debug)]
pub enum News {
    /// The page, whole: replaces any copy held.
    Shown(CanvasPage),
    /// The operations after the page, or after the last news.
    Changed(CanvasFeed),
    /// The daemon did not answer, or refused; the follower starts over.
    Problem(Problem),
}

/// One canvas page followed on its own thread. Dropping it ends the thread
/// once its request in flight returns (at most [`WAIT`]).
#[derive(Debug)]
pub struct Follower {
    news: Receiver<News>,
    stop: Arc<AtomicBool>,
}

impl Follower {
    /// Follows `page` (by id or name; the first page without one) of
    /// `canvas` through `daemon`. Returns at once.
    pub fn start(daemon: Client, canvas: String, page: Option<String>) -> Self {
        let (send, news) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let told = send.clone();
        let spawned = thread::Builder::new()
            .name(format!("follow {canvas}"))
            .spawn(move || follow(&daemon, &canvas, page.as_deref(), &send, &stopped));
        if let Err(e) = spawned {
            let hint = Some("quit some programs and open the app again");
            // The receiver is right here, so the send cannot fail.
            let _ = told.send(News::Problem(Problem::new(e.to_string(), hint)));
        }
        Self { news, stop }
    }

    /// What the follower heard since the last call, oldest first. Never waits.
    pub fn news(&self) -> Vec<News> {
        self.news.try_iter().collect()
    }
}

impl Drop for Follower {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The follower's thread: show, then follow the feed; on any error, say so,
/// wait and show again. Ends when stopped or when no one listens.
fn follow(
    daemon: &Client,
    canvas: &str,
    page: Option<&str>,
    send: &Sender<News>,
    stop: &AtomicBool,
) {
    let fail = |e: &RemoteError| {
        let told = send.send(News::Problem(Problem::new(e.to_string(), e.hint()))).is_ok();
        thread::sleep(RETRY);
        told
    };
    while !stop.load(Ordering::Relaxed) {
        let mut after = match daemon.canvas_show(canvas, page) {
            Ok(shown) => {
                let after = shown.next.saturating_sub(1);
                if send.send(News::Shown(shown)).is_err() {
                    return;
                }
                after
            }
            Err(e) if fail(&e) => continue,
            Err(_) => return,
        };
        while !stop.load(Ordering::Relaxed) {
            match daemon.canvas_changes(canvas, after, WAIT) {
                Ok(feed) => {
                    after = feed.next.saturating_sub(1);
                    if !feed.ops.is_empty() && send.send(News::Changed(feed)).is_err() {
                        return;
                    }
                }
                Err(e) if fail(&e) => break,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use serde_json::json;
    use slinger_control::CanvasOps;

    use super::*;
    use crate::backend::tests::with_daemon;

    /// The next news, waiting up to five seconds for it.
    fn next(follower: &Follower) -> News {
        let start = Instant::now();
        loop {
            if let Some(news) = follower.news.try_iter().next() {
                return news;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "no news");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn shows_the_page_then_hands_on_each_change() {
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            let follower = Follower::start(daemon.clone(), "main".into(), None);
            let News::Shown(page) = next(&follower) else { panic!("not shown") };
            assert_eq!((page.page.as_str(), page.next, page.items.len()), ("p1", 1, 0));
            let item = json!({"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10});
            let ops = json!({"op": "add", "item": item});
            daemon
                .canvas_apply(CanvasOps { canvas: "main".into(), by: "user".into(), ops })
                .unwrap();
            let News::Changed(feed) = next(&follower) else { panic!("no change") };
            assert_eq!((feed.next, feed.ops.len()), (2, 1));
            assert_eq!(feed.ops[0]["item"]["id"], "i1");
            assert!(follower.news().is_empty());
        });
    }

    #[test]
    fn a_missing_canvas_or_daemon_is_a_problem_and_it_tries_again() {
        with_daemon(|daemon| {
            let follower = Follower::start(daemon.clone(), "nothing".into(), None);
            let News::Problem(problem) = next(&follower) else { panic!("no problem") };
            assert!(problem.hint.is_some());
            daemon.canvas_create("nothing").unwrap();
            assert!(matches!(next(&follower), News::Shown(_)), "it did not try again");
        });
        let gone = Follower::start(Client::new("/nonexistent/x.sock"), "main".into(), None);
        let News::Problem(problem) = next(&gone) else { panic!("no problem") };
        assert!(problem.hint.unwrap().starts_with("start it with '"));
    }
}
