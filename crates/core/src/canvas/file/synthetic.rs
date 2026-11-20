//! A synthetic canvas file of realistic shape, for measuring (slinger-6c0f10).
//!
//! The owner's real canvases are many times the largest one on a test
//! machine and hold private data, so none can be copied. [`Synthetic`] makes
//! one instead, the same bytes for the same settings: pages of handwriting
//! (clusters of strokes, most short, some with long point arrays), typed and
//! agent texts, boxes, bound arrows, folded boxes, a PDF's pages with ink
//! over them, and the churn of real use (lasso moves, erases, text edits,
//! undo and redo). Its shape follows the real board measured in
//! `docs/design/canvas-file.md` (strokes of 21 points at the median, some of
//! hundreds). It is written through [`Log`], so it is a file the daemon wrote.
//!
//! At the default 120 sessions the file is a little over 5 MB; the size
//! grows with the sessions.

use serde_json::{Value, json};

use super::Log;
use crate::Result;
use crate::canvas::{Author, Time};

/// What to make. [`Synthetic::default`] is [`SESSIONS`] sessions, seed 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synthetic {
    /// How many sittings of writing on the same pages: the size of the file.
    pub sessions: u32,
    /// The seed of the generator: another seed, another canvas of the same shape.
    pub seed: u64,
    /// The blob id of the PDF whose pages the canvas shows. The default names
    /// no blob: the canvas reads, the app draws an empty page.
    pub pdf: String,
}

impl Default for Synthetic {
    fn default() -> Self {
        Self { sessions: SESSIONS, seed: 1, pdf: format!("sha256:{}", "0".repeat(64)) }
    }
}

/// The pages after the first, which every canvas has (`p1`, "Page 1").
const PAGES: [&str; 5] = ["ideas", "spec", "paper", "sketches", "scratch"];
/// The page the PDF's pages are on (`p4`, "paper").
const PAPER: usize = 3;
/// How many pages of the PDF the canvas shows.
const PDF_PAGES: i64 = 4;
/// The default sessions: a file a little over 5 MB.
pub const SESSIONS: u32 = 120;
/// The first operation's time: 2025-11-10T16:00:00Z.
const START: u64 = 1_762_790_400_000;

const COLOURS: [&str; 4] = ["black", "blue", "#cc0000", "#2a7a2a"];
const WIDTHS: [i64; 4] = [15, 20, 30, 60];
const WORDS: [&str; 24] = [
    "plan", "the", "storage", "canvas", "switch", "why", "slow", "agent", "page", "sling", "draw",
    "fold", "box", "ink", "later", "ship", "ask", "owner", "read", "file", "log", "next", "fix",
    "test",
];

impl Synthetic {
    /// The bytes of the canvas file `id`: the header and every line.
    ///
    /// # Errors
    ///
    /// [`crate::Error::Invalid`] when `id` is not a canvas id.
    pub fn file(&self, id: &str) -> Result<String> {
        let (log, bytes) = Log::create(id)?;
        let mut maker = Maker {
            log,
            bytes,
            rng: Rng(self.seed),
            at: START,
            pages: vec![Page::default(); PAGES.len() + 1],
            may_undo: false,
        };
        maker.pages(&self.pdf)?;
        for _ in 0..self.sessions {
            maker.session()?;
        }
        Ok(maker.bytes)
    }
}

/// splitmix64: small, fast, and the same numbers everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `lo..hi` (`hi` above `lo`).
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = hi.abs_diff(lo).max(1);
        lo.saturating_add_unsigned(self.next() % span)
    }

    /// A number in `0.0..1.0`.
    #[expect(clippy::cast_precision_loss, reason = "53 bits of a u64 are plenty")]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn pick<'a, T>(&mut self, list: &'a [T]) -> &'a T {
        let len = i64::try_from(list.len()).unwrap_or(i64::MAX);
        &list[usize::try_from(self.range(0, len)).unwrap_or(0)]
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

/// What a page holds that later operations may name.
#[derive(Debug, Clone, Default)]
struct Page {
    /// The live strokes, cluster by cluster.
    clusters: Vec<Vec<String>>,
    /// The live texts and who made each.
    texts: Vec<(String, bool)>,
    /// The live boxes and texts an arrow may point at.
    targets: Vec<String>,
}

/// Writes the lines, keeping what later lines may name.
struct Maker {
    log: Log,
    bytes: String,
    rng: Rng,
    at: u64,
    pages: Vec<Page>,
    /// Whether the user's last step was a move or an erase, which an undo
    /// may take back for good: nothing tracked then names a gone item.
    may_undo: bool,
}

fn scout() -> Author {
    Author::Agent("scout".into())
}

impl Maker {
    /// Applies `ops` as one step of `by` some time after the last; the ids it made.
    fn apply(&mut self, by: &Author, ops: &Value) -> Result<Vec<String>> {
        self.at += u64::try_from(self.rng.range(150, 4000)).unwrap_or(0);
        let written = self.log.apply(by, Time(self.at), ops)?;
        if *by == Author::User {
            let kind = ops["op"].as_str();
            self.may_undo = matches!(kind, Some("move" | "remove"));
        }
        self.bytes.push_str(&written.bytes);
        Ok(written.made)
    }

    fn pages(&mut self, pdf: &str) -> Result<()> {
        for name in PAGES {
            self.apply(&Author::User, &json!({"op": "page.add", "name": name}))?;
        }
        for n in 0..PDF_PAGES {
            let item = json!({"page": page_id(PAPER), "kind": "pdf", "x": 0, "y": n * 21_000,
                "w": 16_000, "h": 20_706, "hash": pdf, "pdfpage": n + 1,
                "pagesize": [6120, 7920], "name": "paper.pdf"});
            self.apply(&Author::User, &json!({"op": "add", "item": item}))?;
        }
        Ok(())
    }

    /// One sitting: writing on a page, the agent's answers, then edits.
    fn session(&mut self) -> Result<()> {
        self.at += u64::try_from(self.rng.range(600_000, 20_000_000)).unwrap_or(0);
        // Most of the writing is on the first pages, as on a real board.
        let page = *self.rng.pick(&[0, 0, 0, 0, 1, 1, 1, 2, 2, PAPER, PAPER, 4, 5]);
        for _ in 0..self.rng.range(2, 7) {
            self.cluster(page)?;
        }
        for _ in 0..self.rng.range(1, 6) {
            self.text(page)?;
        }
        if self.rng.chance(0.6) {
            self.boxed(page)?;
        }
        if self.rng.chance(0.5) {
            self.arrow(page)?;
        }
        for _ in 0..self.rng.range(2, 9) {
            self.churn(page)?;
        }
        Ok(())
    }

    /// A word or a sketch: strokes near each other, each its own step.
    fn cluster(&mut self, page: usize) -> Result<()> {
        let (cx, cy) = self.place(page);
        let colour = *self.rng.pick(&COLOURS);
        let width = *self.rng.pick(&WIDTHS);
        let mut ids = Vec::new();
        for _ in 0..self.rng.range(3, 30) {
            let start = (cx + self.rng.range(-3000, 3000), cy + self.rng.range(-1200, 1200));
            let pts = self.points(start);
            let item = json!({"page": page_id(page), "kind": "stroke", "color": colour,
                "width": width, "pts": pts});
            ids.extend(self.apply(&Author::User, &json!({"op": "add", "item": item}))?);
        }
        self.pages[page].clusters.push(ids);
        Ok(())
    }

    /// Where something new goes on `page`, in tenths: on the PDF's pages
    /// for the paper, else anywhere on a wide board.
    fn place(&mut self, page: usize) -> (i64, i64) {
        if page == PAPER {
            (self.rng.range(1000, 15_000), self.rng.range(1000, PDF_PAGES * 21_000 - 1000))
        } else {
            (self.rng.range(0, 200_000), self.rng.range(0, 150_000))
        }
    }

    /// A stroke's delta points from `start`: a pen that turns as it goes.
    /// Most are short; one in twenty is long, as a sketch or a scribble is.
    #[expect(clippy::cast_possible_truncation, reason = "steps are tens of tenths")]
    fn points(&mut self, start: (i64, i64)) -> Vec<i64> {
        let roll = self.rng.unit();
        let count = if roll < 0.70 {
            self.rng.range(6, 32)
        } else if roll < 0.95 {
            self.rng.range(32, 120)
        } else {
            self.rng.range(120, 1500)
        };
        let mut pts = vec![start.0, start.1];
        let mut heading = self.rng.unit() * std::f64::consts::TAU;
        for _ in 1..count {
            heading += (self.rng.unit() - 0.5) * 0.9;
            let step = f64::from(u8::try_from(self.rng.range(8, 40)).unwrap_or(8));
            pts.push((heading.cos() * step).round() as i64);
            pts.push((heading.sin() * step).round() as i64);
        }
        pts
    }

    /// A typed text, or one the agent put beside the writing.
    fn text(&mut self, page: usize) -> Result<()> {
        let (x, y) = self.place(page);
        let agent = self.rng.chance(0.4);
        let words = self.rng.range(2, if agent { 40 } else { 12 });
        let text = self.words(words);
        let mut item = json!({"page": page_id(page), "kind": "text", "color": "black",
            "x": x, "y": y, "text": text});
        if self.rng.chance(0.2) {
            item["size"] = (*self.rng.pick(&[160, 360, 480])).into();
        }
        let by = if agent { scout() } else { Author::User };
        for id in self.apply(&by, &json!({"op": "add", "item": item}))? {
            self.pages[page].texts.push((id.clone(), agent));
            self.pages[page].targets.push(id);
        }
        Ok(())
    }

    fn words(&mut self, n: i64) -> String {
        let words: Vec<&str> = (0..n).map(|_| *self.rng.pick(&WORDS)).collect();
        words.join(" ")
    }

    /// A box around some of the writing; now and then the agent folds one.
    fn boxed(&mut self, page: usize) -> Result<()> {
        let (x, y) = self.place(page);
        let (w, h) = (self.rng.range(4000, 30_000), self.rng.range(3000, 20_000));
        let item = json!({"page": page_id(page), "kind": "box", "color": "black", "x": x, "y": y, "w": w, "h": h});
        let fold = self.rng.chance(0.1);
        let by = if fold { scout() } else { Author::User };
        for id in self.apply(&by, &json!({"op": "add", "item": item}))? {
            if fold {
                self.at += 1000;
                let placed = self.log.fold(&by, Time(self.at), &id, None)?;
                self.bytes.push_str(&placed.written.bytes);
            } else {
                self.pages[page].targets.push(id);
            }
        }
        Ok(())
    }

    /// An arrow between two things on the page, bound at both ends.
    fn arrow(&mut self, page: usize) -> Result<()> {
        let targets = &self.pages[page].targets;
        if targets.len() < 2 {
            return Ok(());
        }
        let from = self.rng.pick(targets).clone();
        let to = self.rng.pick(targets).clone();
        let (x, y) = self.place(page);
        let pts = [x, y, self.rng.range(-8000, 8000), self.rng.range(-8000, 8000)];
        let item = json!({"page": page_id(page), "kind": "arrow", "color": "black",
            "pts": pts, "from": from, "to": to});
        self.apply(&Author::User, &json!({"op": "add", "item": item}))?;
        Ok(())
    }

    /// One edit of what is there: a lasso move, an erase, a text edit, or
    /// an undo (with a redo now and then).
    fn churn(&mut self, page: usize) -> Result<()> {
        let roll = self.rng.unit();
        let clusters = self.pages[page].clusters.len();
        if roll < 0.40 && clusters > 0 {
            let n = usize::try_from(self.rng.range(0, i64::try_from(clusters).unwrap_or(1)));
            let ids = self.pages[page].clusters[n.unwrap_or(0)].clone();
            let (dx, dy) = (self.rng.range(-4000, 4000), self.rng.range(-4000, 4000));
            self.apply(&Author::User, &json!({"op": "move", "ids": ids, "dx": dx, "dy": dy}))?;
        } else if roll < 0.70 && clusters > 0 {
            let cluster = self.pages[page].clusters.len() - 1;
            let ids = &mut self.pages[page].clusters[cluster];
            let erased: Vec<String> = ids.drain(..ids.len().min(3)).collect();
            self.apply(&Author::User, &json!({"op": "remove", "ids": erased}))?;
        } else if roll < 0.90 && !self.pages[page].texts.is_empty() {
            let (id, agent) = self.rng.pick(&self.pages[page].texts).clone();
            let words = self.rng.range(2, 14);
            let text = self.words(words);
            let by = if agent { scout() } else { Author::User };
            self.apply(&by, &json!({"op": "set", "id": id, "fields": {"text": text}}))?;
        } else {
            self.at += 500;
            let undone = self.log.undo(&Author::User, Time(self.at));
            self.bytes.push_str(&undone.bytes);
            let for_good = std::mem::take(&mut self.may_undo) && self.rng.chance(0.5);
            if !for_good {
                self.at += 500;
                let redone = self.log.redo(&Author::User, Time(self.at));
                self.bytes.push_str(&redone.bytes);
            }
        }
        Ok(())
    }
}

fn page_id(index: usize) -> String {
    format!("p{}", index + 1)
}
