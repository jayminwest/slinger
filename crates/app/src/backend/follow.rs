//! Following a canvas (docs/service.md, "The change feed"): a thread of its
//! own shows one page, then waits on the change feed and hands each change
//! to the main thread, which takes it with [`Follower::news`] and never
//! waits. The daemon holds each feed request until a change or [`WAIT`]
//! passes, and each news wakes the main thread ([`Wake`]), so a change
//! reaches the window at once.
//!
//! A follower never gives up: when the daemon does not answer, it says so
//! and tries again every [`RETRY`], starting over from the page, so the pane
//! comes back by itself when the daemon does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

use slinger_control::{CanvasFeed, CanvasPage, Client, RemoteError};

use super::Problem;
use super::wake::{Teller, Wake};

/// How long one feed request waits for a change.
const WAIT: Duration = Duration::from_secs(25);
/// How long to wait before asking a daemon that did not answer again.
const RETRY: Duration = Duration::from_secs(1);

/// What a follower heard, in order.
#[derive(Debug)]
pub enum News {
    /// The page, whole: replaces any copy held; and when it came (for the
    /// switch's trace, [`crate::trace`]).
    Shown(CanvasPage, Instant),
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
    /// `canvas` through `daemon`, waking the main thread with each news.
    /// Returns at once.
    pub fn start(daemon: Client, canvas: String, page: Option<String>, wake: &Wake) -> Self {
        let (send, news) = wake.channel();
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
    send: &Teller<News>,
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
                if send.send(News::Shown(shown, Instant::now())).is_err() {
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
            let asked = Instant::now();
            let follower = Follower::start(daemon.clone(), "main".into(), None, &Wake::none());
            let News::Shown(page, came) = next(&follower) else { panic!("not shown") };
            assert!(asked <= came && came <= Instant::now(), "stamped when it came");
            assert_eq!((page.page.as_str(), page.next, page.items.len()), ("p1", 1, 0));
            let item = json!({"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10});
            let ops = json!({"op": "add", "item": item});
            daemon
                .canvas_apply(CanvasOps {
                    canvas: "main".into(),
                    by: "user".into(),
                    ops,
                    once: None,
                })
                .unwrap();
            let News::Changed(feed) = next(&follower) else { panic!("no change") };
            assert_eq!((feed.next, feed.ops.len()), (2, 1));
            assert_eq!(feed.ops[0]["item"]["id"], "i1");
            assert!(follower.news().is_empty());
        });
    }

    #[test]
    fn the_page_and_each_change_wake_the_main_thread() {
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            let (wake, posts) = crate::backend::wake::tests::counting();
            let posted = || posts.load(std::sync::atomic::Ordering::SeqCst);

            let follower = Follower::start(daemon.clone(), "main".into(), None, &wake);
            assert!(matches!(next(&follower), News::Shown(..)));
            crate::backend::testing::wait_until(|| posted() == 1);
            wake.taken();
            let item = json!({"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10});
            let ops = json!({"op": "add", "item": item});
            let by = "user".into();
            daemon.canvas_apply(CanvasOps { canvas: "main".into(), by, ops, once: None }).unwrap();
            assert!(matches!(next(&follower), News::Changed(_)));
            crate::backend::testing::wait_until(|| posted() == 2);
        });
    }

    #[test]
    fn a_missing_canvas_or_daemon_is_a_problem_and_it_tries_again() {
        with_daemon(|daemon| {
            let follower = Follower::start(daemon.clone(), "nothing".into(), None, &Wake::none());
            let News::Problem(problem) = next(&follower) else { panic!("no problem") };
            assert!(problem.hint.is_some());
            daemon.canvas_create("nothing").unwrap();
            assert!(matches!(next(&follower), News::Shown(..)), "it did not try again");
        });
        let none = Wake::none();
        let gone = Follower::start(Client::new("/nonexistent/x.sock"), "main".into(), None, &none);
        let News::Problem(problem) = next(&gone) else { panic!("no problem") };
        assert!(problem.hint.unwrap().starts_with("start it with '"));
    }
}
