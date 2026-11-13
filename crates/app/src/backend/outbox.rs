//! Sending the pen's changes (SPEC P7): a thread of its own applies each
//! change (a stroke, an erase, a delete, a move, a page's change) to its
//! canvas, in the order it was handed them, as one `canvas_apply`, so one
//! undo step, or sends an undo or redo as `canvas_undo` or `canvas_redo`,
//! as the user, and hands back the daemon's answer. Each `canvas_apply`
//! goes under the key it is applied once under, so a change sent again
//! after its answer was lost is not written twice. The main thread hands changes in with
//! [`Outbox::send`] and takes the answers with [`Outbox::answers`]; it never
//! waits on the socket. Which change goes when is `scene::ink`'s to decide.
//!
//! A daemon that does not answer is told of after a pause of [`RETRY`], so
//! a change sent again at once is not sent many times a second.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slinger_control::{CanvasOps, CanvasStep, Client};

use crate::mirror;
use crate::scene::ink::{Answer, Change};
use crate::scene::pages::PageOp;

/// How long to wait before saying a daemon did not answer.
const RETRY: Duration = Duration::from_secs(1);

/// A name for this run of the app that no other run shares: its process id
/// and when it started, for the keys its changes are applied once under.
pub fn run() -> String {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}-{}.{}", std::process::id(), since.as_secs(), since.subsec_nanos())
}

/// The thread that sends changes. Dropping it ends the thread once the
/// change in flight is answered.
#[derive(Debug)]
pub struct Outbox {
    daemon: Arc<Mutex<Client>>,
    changes: Sender<(u64, String, Change)>,
    answers: Receiver<(u64, Answer)>,
}

impl Outbox {
    /// Starts sending through `daemon`. Returns at once.
    pub fn start(daemon: Client) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let (changes, to_send) = mpsc::channel::<(u64, String, Change)>();
        let (answer, answers) = mpsc::channel();
        let theirs = Arc::clone(&daemon);
        thread::spawn(move || {
            for (key, once, change) in to_send {
                let client = theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
                if answer.send((key, apply(&client, once, &change))).is_err() {
                    return;
                }
            }
        });
        Self { daemon, changes, answers }
    }

    /// Sends through `daemon` from the next change on.
    pub fn reconnect(&self, daemon: Client) {
        *self.daemon.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
    }

    /// Hands `change` to the thread, under `key`, to be applied once under
    /// `once`. Never waits.
    pub fn send(&self, key: u64, once: String, change: Change) {
        // The thread lives as long as the outbox, so the send cannot fail.
        let _ = self.changes.send((key, once, change));
    }

    /// The answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<(u64, Answer)> {
        self.answers.try_iter().collect()
    }
}

/// Applies `change` to its canvas as the user, in one `canvas_apply`: one
/// undo step, once under `once`. An undo or a redo takes back or puts back
/// the user's last step instead.
fn apply(daemon: &Client, once: String, change: &Change) -> Answer {
    let canvas = change.canvas().to_string();
    let written = if let Change::Step { redo, .. } = change {
        let step = CanvasStep { canvas, by: "user".into() };
        if *redo { daemon.canvas_redo(step) } else { daemon.canvas_undo(step) }
    } else {
        let ops = mirror::ops(change);
        daemon.canvas_apply(CanvasOps { canvas, by: "user".into(), ops, once: Some(once) })
    };
    match written {
        Ok(written) => {
            // No line (nothing to undo, ids already gone): nothing to wait for.
            let seq = written.seq.last().copied().unwrap_or(0);
            let mut made = written.made.into_iter();
            match (change, made.next()) {
                (Change::Page { op: PageOp::Add { .. }, .. }, Some(page)) => {
                    Answer::NewPage { seq, page }
                }
                // What adds says the ids it made, so the ink shows the
                // items under them before the feed echoes them.
                (Change::Add(_) | Change::Put { .. } | Change::Carry { .. }, Some(first)) => {
                    let made = std::iter::once(first).chain(made);
                    Answer::Made { seq, ids: made.filter_map(|id| mirror::item_id(&id)).collect() }
                }
                _ => Answer::Applied(seq),
            }
        }
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
    use crate::scene::canvas::ItemId;
    use crate::scene::ink::Stroke;

    fn stroke(canvas: &str, x: f64) -> Change {
        Change::Add(Stroke {
            canvas: canvas.into(),
            page: "p1".into(),
            colour: Rgb { r: 0, g: 0, b: 255 },
            width: 2.0,
            points: vec![Point { x, y: 1.0 }, Point { x: x + 1.0, y: 2.0 }],
        })
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

    /// slinger-fa4f97: two apps started in the same instant still name
    /// their runs apart: no two running apps share a process id.
    #[test]
    fn a_run_is_named_by_the_app_s_process_and_when_it_started() {
        let name = run();
        let (pid, when) = name.split_once('-').unwrap();
        assert_eq!(pid, std::process::id().to_string(), "{name}");
        let (secs, nanos) = when.split_once('.').unwrap();
        assert!(secs.parse::<u64>().is_ok() && nanos.parse::<u32>().is_ok(), "{name}");
    }

    #[test]
    fn strokes_are_written_in_the_order_given_and_a_lost_daemon_is_unreachable() {
        let outbox = Outbox::start(Client::new("/nonexistent/x.sock"));
        outbox.send(1, "t.1".into(), stroke("main", 0.0));
        let lost = wait(&outbox, 1);
        let [(1, Answer::Unreachable(problem))] = lost.as_slice() else { panic!("{lost:?}") };
        assert!(problem.hint.as_deref().unwrap().starts_with("start it with '"));
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            outbox.reconnect(daemon.clone());
            for key in 1..=3 {
                outbox.send(
                    key,
                    format!("t.{key}"),
                    stroke("main", f64::from(u32::try_from(key).unwrap())),
                );
            }
            outbox.send(4, "t.4".into(), stroke("gone", 0.0));
            let answers = wait(&outbox, 4);
            let made = |seq, id| Answer::Made { seq, ids: vec![ItemId(id)] };
            assert_eq!(answers[..3], [(1, made(1, 1)), (2, made(2, 2)), (3, made(3, 3))]);
            assert!(matches!(&answers[3], (4, Answer::Refused(p)) if p.message.contains("gone")));
            let page = daemon.canvas_show("main", None).unwrap();
            let xs: Vec<_> = page.items.iter().map(|i| i["pts"][0].clone()).collect();
            assert_eq!(xs, [10, 20, 30]);
            assert_eq!(page.items[0]["by"], "user");
        });
    }

    #[test]
    fn a_move_and_a_removal_are_one_step_each_and_reach_an_agent_s_items() {
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            let outbox = Outbox::start(daemon.clone());
            for key in 1..=3 {
                outbox.send(
                    key,
                    format!("t.{key}"),
                    stroke("main", f64::from(u32::try_from(key).unwrap())),
                );
            }
            wait(&outbox, 3);
            let agent = |ops| CanvasOps {
                canvas: "main".into(),
                by: "agent:scout".into(),
                ops,
                once: None,
            };
            let note = serde_json::json!({"op": "add", "item":
                {"page": "p1", "kind": "text", "x": 0, "y": 0, "text": "hi"}});
            let made = daemon.canvas_apply(agent(note)).unwrap().made;
            assert_eq!(made, ["i4"]);
            let (canvas, page) = ("main".to_string(), "p1".to_string());
            let ids = vec![ItemId(1), ItemId(4)];
            let by = Point { x: 1.5, y: -2.0 };
            outbox.send(
                5,
                "t.5".into(),
                Change::Move {
                    canvas: canvas.clone(),
                    page: page.clone(),
                    ids,
                    by,
                    follow: vec![],
                },
            );
            outbox.send(
                6,
                "t.6".into(),
                Change::Remove { canvas, page, ids: vec![ItemId(2), ItemId(4)] },
            );
            let answers = wait(&outbox, 2);
            // A line per item; the answer is the last, which the feed must pass.
            assert_eq!(answers, [(5, Answer::Applied(6)), (6, Answer::Applied(8))]);
            let shown = daemon.canvas_show("main", None).unwrap();
            let ids: Vec<_> = shown.items.iter().map(|i| i["id"].clone()).collect();
            assert_eq!(ids, ["i1", "i3"], "the agent's text went too");
            let pts = &shown.items[0]["pts"];
            assert_eq!((&pts[0], &pts[1]), (&25.into(), &(-10).into()), "i1 moved by whole tenths");
            let step = slinger_control::CanvasStep { canvas: "main".into(), by: "user".into() };
            daemon.canvas_undo(step).unwrap();
            let back = daemon.canvas_show("main", None).unwrap();
            assert_eq!(back.items.len(), 4, "one undo puts both back: one step");
        });
    }

    #[test]
    fn undo_and_redo_take_only_the_user_s_steps_in_turn_and_a_new_page_says_its_id() {
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            let outbox = Outbox::start(daemon.clone());
            outbox.send(1, "t.1".into(), stroke("main", 1.0));
            wait(&outbox, 1);
            let note = serde_json::json!({"op": "add", "item":
                {"page": "p1", "kind": "text", "x": 0, "y": 0, "text": "hi"}});
            let by = "agent:scout".to_string();
            daemon
                .canvas_apply(CanvasOps { canvas: "main".into(), by, ops: note, once: None })
                .unwrap();
            let step = |redo| Change::Step { canvas: "main".into(), redo };
            // Sent right after a stroke, an undo undoes that stroke.
            outbox.send(2, "t.2".into(), stroke("main", 2.0));
            outbox.send(3, "t.3".into(), step(false));
            outbox.send(4, "t.4".into(), step(false));
            outbox.send(5, "t.5".into(), step(true));
            let answers = wait(&outbox, 4);
            let written = |a: &Answer| match a {
                Answer::Applied(n) | Answer::Made { seq: n, .. } => *n > 0,
                _ => false,
            };
            assert!(answers.iter().all(|(_, a)| written(a)), "{answers:?}");
            let shown = daemon.canvas_show("main", None).unwrap();
            let left: Vec<_> = shown.items.iter().map(|i| i["kind"].clone()).collect();
            assert_eq!(
                left,
                ["stroke", "text"],
                "the second stroke is undone, the agent's text stays"
            );
            assert_eq!(shown.items[0]["pts"][0], 10, "the first stroke is back");
            outbox.send(6, "t.6".into(), step(true));
            outbox.send(60, "t.60".into(), step(true));
            let answers = wait(&outbox, 2);
            assert!(matches!(answers[0], (6, Answer::Applied(n)) if n > 0), "the second stroke");
            assert_eq!(answers[1], (60, Answer::Applied(0)), "nothing more to redo");

            let op = PageOp::Add { name: "Page 2".into() };
            outbox.send(7, "t.7".into(), Change::Page { canvas: "main".into(), op });
            let rename = PageOp::Rename { page: "p2".into(), name: "spec".into() };
            outbox.send(8, "t.8".into(), Change::Page { canvas: "main".into(), op: rename });
            let answers = wait(&outbox, 2);
            assert!(matches!(&answers[0], (7, Answer::NewPage { page, .. }) if page == "p2"));
            let pages = daemon.canvas_show("main", Some("spec")).unwrap().pages;
            assert_eq!(pages[1]["name"], "spec");
            outbox.send(
                9,
                "t.9".into(),
                Change::Page { canvas: "main".into(), op: PageOp::Remove { page: "p2".into() } },
            );
            wait(&outbox, 1);
            assert_eq!(daemon.canvas_show("main", None).unwrap().pages.len(), 1);
        });
    }

    #[test]
    fn a_text_a_box_and_a_bound_arrow_are_written_and_the_arrow_follows_in_one_step() {
        use crate::layout::Rect;
        use crate::scene::canvas::{Item, Shape};
        with_daemon(|daemon| {
            daemon.canvas_create("main").unwrap();
            let outbox = Outbox::start(daemon.clone());
            let colour = Rgb { r: 0, g: 0, b: 255 };
            let here = || ("main".to_string(), "p1".to_string());
            let frame = Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
            let at = |x, y| Point { x, y };
            let shapes = [
                Shape::Box { frame, width: 2.0 },
                Shape::Text {
                    frame: Rect { x: 50.0, ..frame },
                    text: "hi".into(),
                    size: 24.0,
                    column: None,
                },
                Shape::Arrow {
                    from: at(5.0, 5.0),
                    to: at(55.0, 5.0),
                    width: 2.0,
                    bound: [Some(ItemId(1)), Some(ItemId(2))],
                },
            ];
            for (key, shape) in (1..).zip(shapes) {
                let (canvas, page) = here();
                outbox.send(key, format!("t.{key}"), Change::Put { canvas, page, colour, shape });
            }
            let edited = Shape::Text {
                frame: Rect { x: 50.0, ..frame },
                text: "ho".into(),
                size: 24.0,
                column: None,
            };
            let (canvas, page) = here();
            outbox.send(
                4,
                "t.4".into(),
                Change::Set { canvas, page, id: ItemId(2), shape: edited },
            );
            let arrow = Shape::Arrow {
                from: at(6.0, 4.0),
                to: at(55.0, 5.0),
                width: 2.0,
                bound: [Some(ItemId(1)), Some(ItemId(2))],
            };
            let follow = vec![Item { id: ItemId(3), colour, shape: arrow }];
            let (canvas, page) = here();
            outbox.send(
                5,
                "t.5".into(),
                Change::Move { canvas, page, ids: vec![ItemId(1)], by: at(1.0, -1.0), follow },
            );
            let answers = wait(&outbox, 5);
            let made = |seq, id| Answer::Made { seq, ids: vec![ItemId(id)] };
            assert_eq!(answers[..3], [(1, made(1, 1)), (2, made(2, 2)), (3, made(3, 3))]);
            let rest = &answers[3..];
            assert!(rest.iter().all(|(_, a)| matches!(a, Answer::Applied(_))), "{answers:?}");
            let shown = daemon.canvas_show("main", None).unwrap();
            let [boxed, text, arrow] = &shown.items[..] else { panic!("{shown:?}") };
            assert_eq!(
                (&boxed["x"], &boxed["y"], &boxed["w"]),
                (&10.into(), &(-10).into(), &100.into())
            );
            assert_eq!((&text["text"], &text["color"]), (&"ho".into(), &"#0000ff".into()));
            assert_eq!(arrow["pts"], serde_json::json!([60, 40, 490, 10]), "its start followed");
            assert_eq!((&arrow["from"], &arrow["to"]), (&"i1".into(), &"i2".into()));
            let step = slinger_control::CanvasStep { canvas: "main".into(), by: "user".into() };
            daemon.canvas_undo(step).unwrap();
            let back = daemon.canvas_show("main", None).unwrap();
            assert_eq!(back.items[0]["x"], 0, "one undo puts the box back...");
            assert_eq!(
                back.items[2]["pts"],
                serde_json::json!([50, 50, 500, 0]),
                "...and the arrow"
            );
        });
    }
}
