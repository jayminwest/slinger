//! Sending the pen's strokes (SPEC P7): a thread of its own applies each
//! stroke to its canvas, in the order it was handed them, and hands back the
//! daemon's answer. The main thread hands strokes in with [`Outbox::send`]
//! and takes the answers with [`Outbox::answers`]; it never waits on the
//! socket. Which stroke goes when is `scene::ink`'s to decide.
//!
//! A daemon that does not answer is told of after a pause of [`RETRY`], so
//! a stroke sent again at once is not sent many times a second.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::json;
use slinger_control::{CanvasOps, Client};

use crate::mirror;
use crate::scene::ink::{Answer, Stroke};

/// How long to wait before saying a daemon did not answer.
const RETRY: Duration = Duration::from_secs(1);

/// The thread that sends strokes. Dropping it ends the thread once the
/// stroke in flight is answered.
#[derive(Debug)]
pub struct Outbox {
    daemon: Arc<Mutex<Client>>,
    strokes: Sender<(u64, Stroke)>,
    answers: Receiver<(u64, Answer)>,
}

impl Outbox {
    /// Starts sending through `daemon`. Returns at once.
    pub fn start(daemon: Client) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let (strokes, to_send) = mpsc::channel::<(u64, Stroke)>();
        let (answer, answers) = mpsc::channel();
        let theirs = Arc::clone(&daemon);
        thread::spawn(move || {
            for (key, stroke) in to_send {
                let client = theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
                if answer.send((key, apply(&client, &stroke))).is_err() {
                    return;
                }
            }
        });
        Self { daemon, strokes, answers }
    }

    /// Sends through `daemon` from the next stroke on.
    pub fn reconnect(&self, daemon: Client) {
        *self.daemon.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
    }

    /// Hands `stroke` to the thread, under `key`. Never waits.
    pub fn send(&self, key: u64, stroke: Stroke) {
        // The thread lives as long as the outbox, so the send cannot fail.
        let _ = self.strokes.send((key, stroke));
    }

    /// The answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<(u64, Answer)> {
        self.answers.try_iter().collect()
    }
}

/// Applies `stroke` to its canvas as one `add` by the user: one undo step.
fn apply(daemon: &Client, stroke: &Stroke) -> Answer {
    let ops = json!({"op": "add", "item": mirror::stroke_object(stroke)});
    let request = CanvasOps { canvas: stroke.canvas.clone(), by: "user".into(), ops };
    match daemon.canvas_apply(request) {
        Ok(change) => Answer::Applied(change.seq.last().copied().unwrap_or(0)),
        Err(e) if e.code() == "unavailable" => {
            thread::sleep(RETRY);
            Answer::Unreachable(e.into())
        }
        Err(e) => Answer::Refused(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::backend::tests::with_daemon;
    use crate::config::Rgb;
    use crate::layout::Point;

    fn stroke(canvas: &str, x: f64) -> Stroke {
        Stroke {
            canvas: canvas.into(),
            page: "p1".into(),
            colour: Rgb { r: 0, g: 0, b: 255 },
            width: 2.0,
            points: vec![Point { x, y: 1.0 }, Point { x: x + 1.0, y: 2.0 }],
        }
    }

    /// The next `n` answers, waiting up to five seconds for them.
    fn wait(outbox: &Outbox, n: usize) -> Vec<(u64, Answer)> {
        let start = Instant::now();
        let mut got = Vec::new();
        while got.len() < n {
            got.extend(outbox.answers());
            assert!(start.elapsed() < Duration::from_secs(5), "only {got:?}");
            thread::sleep(Duration::from_millis(10));
        }
        got
    }

    #[test]
    fn strokes_are_written_in_the_order_given_and_a_lost_daemon_is_unreachable() {
        let outbox = Outbox::start(Client::new("/nonexistent/x.sock"));
        outbox.send(1, stroke("main", 0.0));
        let lost = wait(&outbox, 1);
        let [(1, Answer::Unreachable(problem))] = lost.as_slice() else { panic!("{lost:?}") };
        assert!(problem.hint.as_deref().unwrap().starts_with("start it with '"));
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            outbox.reconnect(daemon.clone());
            for key in 1..=3 {
                outbox.send(key, stroke("main", f64::from(u32::try_from(key).unwrap())));
            }
            outbox.send(4, stroke("gone", 0.0));
            let answers = wait(&outbox, 4);
            assert_eq!(
                answers[..3],
                [(1, Answer::Applied(1)), (2, Answer::Applied(2)), (3, Answer::Applied(3))]
            );
            assert!(matches!(&answers[3], (4, Answer::Refused(p)) if p.message.contains("gone")));
            let page = daemon.canvas_show("main", None).unwrap();
            let xs: Vec<_> = page.items.iter().map(|i| i["pts"][0].clone()).collect();
            assert_eq!(xs, [10, 20, 30]);
            assert_eq!(page.items[0]["by"], "user");
        });
    }
}
