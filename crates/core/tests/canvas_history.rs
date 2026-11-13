//! The undo history kept in the canvas file (slinger-2628e5): each line says
//! where it stands (`step`, `undoes`, `redoes`), so a log read again from its
//! bytes undoes and redoes exactly as the log that wrote them would have.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{Log, Written};
use slinger_core::canvas::{Author, Canvas, Time};

const AT: Time = Time(1_762_790_400_000);

fn ada() -> Author {
    Author::Agent("ada".into())
}

fn bx(page: &str, x: i64) -> Value {
    json!({ "op": "add", "item": { "page": page, "kind": "box", "x": x, "y": 0, "w": 100, "h": 50 } })
}

/// A canvas and its file, read again from the file after each change, as a
/// daemon that restarts between every request would.
struct Restarting {
    log: Log,
    file: String,
}

impl Restarting {
    fn new() -> Self {
        let (log, file) = Log::create("main").unwrap();
        Self { log, file }
    }

    fn keep(&mut self, written: &Written) -> Vec<u64> {
        self.file.push_str(&written.bytes);
        self.log = Log::open(self.file.as_bytes()).unwrap().0;
        written.seq.clone()
    }

    fn apply(&mut self, by: &Author, ops: &Value) -> Vec<u64> {
        let written = self.log.apply(by, AT, ops).unwrap();
        self.keep(&written)
    }

    fn undo(&mut self, by: &Author) -> Vec<u64> {
        let written = self.log.undo(by, AT);
        self.keep(&written)
    }

    fn redo(&mut self, by: &Author) -> Vec<u64> {
        let written = self.log.redo(by, AT);
        self.keep(&written)
    }

    fn pages(&self) -> Vec<String> {
        self.log.canvas().pages().map(|p| p.name().to_owned()).collect()
    }

    fn items(&self, page: &str) -> usize {
        self.log.show(Some(page)).map_or(0, |shown| shown.items.len())
    }
}

#[test]
fn a_page_removed_before_a_restart_comes_back_with_its_items() {
    let mut c = Restarting::new();
    c.apply(&Author::User, &json!({ "op": "page.add", "name": "ideas" }));
    c.apply(&Author::User, &json!([bx("p2", 0), bx("p2", 500)]));
    c.apply(&Author::User, &json!({ "op": "page.remove", "page": "p2" }));
    assert_eq!(c.pages(), ["Page 1"]);
    assert_eq!(c.undo(&Author::User), [5]);
    assert_eq!((c.pages(), c.items("p2")), (vec!["Page 1".into(), "ideas".into()], 2));
    assert_eq!(c.redo(&Author::User), [6]);
    assert_eq!(c.pages(), ["Page 1"]);
    assert_eq!(c.undo(&Author::User), [7]);
    assert_eq!(c.undo(&Author::User), [8, 9], "the two boxes went in one step");
    assert_eq!(c.undo(&Author::User), [10]);
    assert_eq!(c.pages(), ["Page 1"]);
    assert_eq!(c.undo(&Author::User), [0_u64; 0], "nothing is left to undo");
    assert_eq!(c.redo(&Author::User), [11]);
    assert_eq!(c.redo(&Author::User), [12, 13]);
    assert_eq!(c.items("p2"), 2);
}

#[test]
fn each_author_undoes_only_their_own_steps_after_a_restart() {
    let mut c = Restarting::new();
    c.apply(&Author::User, &bx("p1", 0));
    c.apply(&ada(), &bx("p1", 500));
    c.apply(&Author::User, &bx("p1", 1000));
    assert_eq!(c.undo(&ada()), [4]);
    assert_eq!(c.undo(&ada()), [0_u64; 0]);
    assert_eq!(c.undo(&Author::User), [5]);
    assert_eq!(c.undo(&Author::User), [6]);
    assert_eq!(c.items("p1"), 0);
    assert_eq!(c.redo(&ada()), [7]);
    assert_eq!(c.log.show(None).unwrap().items[0]["by"], "agent:ada");
}

#[test]
fn a_new_step_after_a_restart_ends_what_could_be_redone() {
    let mut c = Restarting::new();
    c.apply(&Author::User, &bx("p1", 0));
    c.undo(&Author::User);
    c.apply(&Author::User, &bx("p1", 500));
    assert_eq!(c.redo(&Author::User), [0_u64; 0]);
    assert_eq!(c.undo(&Author::User), [4]);
    assert_eq!(c.undo(&Author::User), [0_u64; 0]);
}

#[test]
fn a_canvas_made_with_its_ops_is_one_step_and_an_import_is_none() {
    let pages = json!([bx("p1", 0), bx("p1", 0), bx("p1", 0)]);
    let (_, bytes) = Log::create_with("pdf", &Author::User, AT, &pages).unwrap();
    let mut log = Log::open(bytes.as_bytes()).unwrap().0;
    assert_eq!(log.undo(&Author::User, AT).seq, [4, 5, 6], "a PDF's open is one undo");
    assert!(log.canvas().items(log.canvas().pages().next().unwrap().id()).next().is_none());

    let mut canvas = Canvas::new();
    let (made, _) = Log::create_with("x", &Author::User, AT, &bx("p1", 0)).unwrap();
    canvas.clone_from(made.canvas());
    let (_, bytes) = Log::import("y", &canvas, AT).unwrap();
    let mut log = Log::open(bytes.as_bytes()).unwrap().0;
    assert_eq!(log.undo(&Author::User, AT).seq, [0_u64; 0], "an import is no undo step");
}

#[test]
fn lines_without_a_step_are_no_undo_step_and_a_bad_mark_is_none() {
    let header = "{\"slinger\":\"canvas\",\"v\":1,\"id\":\"old\",\"first\":1}\n";
    let add = |n: u64, tail: &str| {
        format!(
            "{{\"n\":{n},\"t\":\"2025-11-10T16:00:00.000Z\",\"by\":\"user\",\"op\":\"add\",\"item\":{{\"id\":\"i{n}\",\"page\":\"p1\",\"kind\":\"box\",\"color\":\"black\",\"x\":0,\"y\":0,\"w\":10,\"h\":10}}{tail}}}\n"
        )
    };
    let file =
        format!("{header}{}{}{}", add(1, ""), add(2, ",\"step\":\"x\""), add(3, ",\"step\":0"));
    let mut log = Log::open(file.as_bytes()).unwrap().0;
    assert_eq!(log.undo(&Author::User, AT).seq, [0_u64; 0]);
    assert_eq!(log.canvas().items(log.canvas().pages().next().unwrap().id()).count(), 3);

    let file = format!("{file}{}", add(4, ",\"step\":4"));
    let mut log = Log::open(file.as_bytes()).unwrap().0;
    let undone = log.undo(&Author::User, AT);
    assert_eq!(undone.seq, [5]);
    assert!(undone.bytes.ends_with(",\"undoes\":4}\n"), "{}", undone.bytes);
}

/// A tiny generator, so the walk below is the same on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// The ids of the items `by` made, on every page.
fn own(log: &Log, by: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for page in log.canvas().pages() {
        let shown = log.show(Some(&format!("p{}", page.id().get()))).unwrap();
        let mine = shown.items.iter().filter(|i| i["by"] == by);
        ids.extend(mine.map(|i| i["id"].as_str().unwrap().to_owned()));
    }
    ids
}

#[test]
fn a_log_read_again_at_any_point_undoes_and_redoes_as_the_one_held_open() {
    let mut rng = Rng(0x2628_e5ca_f00d_d00d);
    for _ in 0..40 {
        let (mut held, mut file) = Log::create("walk").unwrap();
        for _ in 0..rng.below(60) {
            // Each author changes only what it made, so no step of one is
            // left with nothing to take back by the other.
            let by = if rng.below(2) == 0 { Author::User } else { ada() };
            let read = Log::open(file.as_bytes()).unwrap().0;
            for walk in [Log::undo, Log::redo] {
                let (mut a, mut b) = (held.clone(), read.clone());
                assert_eq!(walk(&mut a, &by, AT), walk(&mut b, &by, AT), "{file}");
            }
            let mine = own(&held, if by == Author::User { "user" } else { "agent:ada" });
            let kind = rng.below(9);
            let mut pick = || mine[usize::try_from(rng.below(mine.len() as u64)).unwrap()].clone();
            let ops = match kind {
                0 | 1 => {
                    file.push_str(&held.undo(&by, AT).bytes);
                    continue;
                }
                2 => {
                    file.push_str(&held.redo(&by, AT).bytes);
                    continue;
                }
                3 | 4 => json!([bx("p1", 10), bx("p1", 20)]),
                5 if !mine.is_empty() => {
                    json!({ "op": "move", "ids": [pick(), pick()], "dx": 5, "dy": 0 })
                }
                6 if !mine.is_empty() => json!({ "op": "remove", "ids": [pick()] }),
                7 if !mine.is_empty() => {
                    json!({ "op": "set", "id": pick(), "fields": { "w": 300 } })
                }
                _ => continue,
            };
            file.push_str(&held.apply(&by, AT, &ops).unwrap().bytes);
        }
        assert_eq!(Log::open(file.as_bytes()).unwrap().0.canvas(), held.canvas());
    }
}
