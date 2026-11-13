//! Sending a sling (docs/service.md, "Slings"): a thread of its own sends
//! each [`Ask`] as one `sling_send` and hands back what the daemon said, so
//! the main thread never waits on the socket. The daemon picks the agent
//! (the agent of the open workspace whose canvas it is) and replies once
//! the message is in the agent's process, or waits for its listener. The
//! thread then follows the agent's session (`session_show`, every
//! [`EVERY`]) until the agent has no message pending or its process is
//! gone, handing back each change, so the window can say it works and when
//! it answered (SPEC P6). The session is read as the way the sling went
//! ([`progress`]): a process started while it waits for a listener does
//! not take it.
//!
//! Over a PDF, the sling first asks the daemon which areas of its pages lie
//! under the new ink (`sling_backdrops`), renders each with the [`Render`]
//! it was started with, from the PDF fetched into the directory it was
//! started with, and sends the files with the sling. An area that
//! does not render is left out: the sling still goes, that ink on white.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use slinger_control::{Backdrop, BackdropArea, Client, MAX_LINE, Session, SlingAsk};

use crate::backend::Problem;
use crate::mirror;
use crate::scene::sling::{Ask, Profile};
use crate::scene::slung::Progress;

/// How often the agent's session is read while a sling is out.
const EVERY: Duration = Duration::from_millis(500);

/// The slings this app has sent: each one's directory is its own.
static SLINGS: AtomicU64 = AtomicU64::new(0);

/// Renders one area of a PDF page for a sling (`platform::pdf::backdrop`):
/// the area as `sling_backdrops` gave it, the PDF's file, and the files to
/// write, the raw RGB and the words (left unwritten when the page has
/// none). False when it could not render.
pub type Render = fn(&BackdropArea, &Path, &Path, &Path) -> bool;

/// The bytes of one backdrop in a request besides its item and paths: its
/// keys, quotes and area, at most.
const BACKDROP_JSON: usize = 128;

/// What the thread hands back about the sling it sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// The sling is in this agent's process, and about how many tokens the
    /// agent reads for it (`None`: the daemon does not count).
    Went(String, Option<u64>),
    /// The sling did not go.
    Failed(Problem),
    /// The agent's session after the sling went, when it changed.
    Progress(Progress),
    /// The daemon could not be read while the agent worked.
    Lost(Problem),
}

/// The thread that sends slings. Dropping it ends the thread once the
/// sling in flight is answered.
#[derive(Debug)]
pub struct Slinger {
    daemon: Arc<Mutex<Client>>,
    asks: Sender<Ask>,
    heard: Receiver<Heard>,
}

impl Slinger {
    /// Starts sending through `daemon`, rendering the page under new ink
    /// with `render` from PDFs fetched into its directory (none: ink on
    /// white). Returns at once.
    pub fn start(daemon: Client, render: Option<(Render, PathBuf)>) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let (asks, to_send) = mpsc::channel::<Ask>();
        let (tell, heard) = mpsc::channel();
        let theirs = Arc::clone(&daemon);
        thread::spawn(move || {
            let client = || theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let mut next = to_send.recv().ok();
            while let Some(ask) = next.take() {
                let (agent, tokens, to) = match sling(&client(), ask, render.as_ref()) {
                    Ok(went) => went,
                    Err(problem) => {
                        if tell.send(Heard::Failed(problem)).is_err() {
                            return;
                        }
                        next = to_send.recv().ok();
                        continue;
                    }
                };
                if tell.send(Heard::Went(agent.clone(), tokens)).is_err() {
                    return;
                }
                next = match follow(&client, (&agent, to), &to_send, &tell) {
                    Some(Followed::Over) => to_send.recv().ok(),
                    Some(Followed::Next(ask)) => Some(ask),
                    None => return,
                };
            }
        });
        Self { daemon, asks, heard }
    }

    /// Sends through `daemon` from the next sling on.
    pub fn reconnect(&self, daemon: Client) {
        *self.daemon.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
    }

    /// Hands `ask` to the thread. Never waits. The sling followed until
    /// now is no longer followed.
    pub fn send(&self, ask: Ask) {
        // The thread lives as long as the slinger, so the send cannot fail.
        let _ = self.asks.send(ask);
    }

    /// What the thread heard since the last call, oldest first. Never
    /// waits.
    pub fn heard(&self) -> Vec<Heard> {
        self.heard.try_iter().collect()
    }
}

/// How following an agent ended.
enum Followed {
    /// It answered, its process ended, or the daemon could not be read.
    Over,
    /// A new sling came first.
    Next(Ask),
}

/// Follows `agent`'s session through the daemon `client` gives, as the
/// sling went `to` it, telling each change to `tell`, until it is over or
/// a new ask comes on `asks`. `None`: the slinger was dropped.
fn follow(
    client: &impl Fn() -> Client,
    (agent, to): (&str, Option<To>),
    asks: &Receiver<Ask>,
    tell: &Sender<Heard>,
) -> Option<Followed> {
    let mut last = None;
    // The first look is at once: a sling nobody listens for says so from the start.
    let mut pause = Duration::ZERO;
    loop {
        let waited = asks.recv_timeout(pause);
        pause = EVERY;
        match waited {
            Ok(ask) => return Some(Followed::Next(ask)),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let (heard, over) = match client().session_show(agent) {
            Ok(session) => {
                let now = progress(&session, to);
                let over = !now.running || now.pending == 0;
                if last.as_ref() == Some(&now) {
                    continue;
                }
                last = Some(now.clone());
                (Heard::Progress(now), over)
            }
            Err(e) => (Heard::Lost(Problem::from(e)), true),
        };
        tell.send(heard).ok()?;
        if over {
            return Some(Followed::Over);
        }
    }
}

/// Sends `ask` as one `sling_send`: the canvas, the page, the selected
/// items by the canvas file's ids, and the prompt. Under a picked profile
/// it first starts the agent of the canvas's workspace under it (the
/// daemon leaves a process running under it as it is, and restarts one
/// under another), and the sling goes only to that profile. Returns the
/// agent it went to and the tokens it costs. With `render` and the PDFs'
/// directory, the page under the new ink goes with it ([`backdrops`]).
/// Returns the agent it went to, the tokens it costs and the way it went
/// (`None`: the daemon does not say).
fn sling(
    daemon: &Client,
    ask: Ask,
    render: Option<&(Render, PathBuf)>,
) -> Result<(String, Option<u64>, Option<To>), Problem> {
    let profile = match ask.profile {
        Profile::Running => None,
        Profile::Picked(profile) => {
            let open = daemon.workspace_list()?;
            let workspace = open.iter().find(|w| w.canvas == ask.canvas);
            // A canvas no open workspace has: the sling says so itself.
            if let Some(workspace) = workspace {
                daemon.session_start_with(&workspace.name, profile.as_deref(), false)?;
            }
            profile
        }
    };
    let select = ask.ids.into_iter().map(mirror::item_ref).collect();
    let (canvas, page, prompt) = (ask.canvas, Some(ask.page), ask.prompt);
    let n = SLINGS.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("slinger-app-sling-{}-{n}", std::process::id()));
    let backdrops = render
        .map(|(r, pdfs)| backdrops(daemon, &canvas, page.as_deref(), *r, pdfs, &dir))
        .unwrap_or_default();
    let request = SlingAsk { canvas, page, select, prompt, profile, backdrops };
    let slung = daemon.sling_send(request);
    let _ = fs::remove_dir_all(&dir);
    let slung = slung.map_err(Problem::from)?;
    let to = slung.deliver.map(|d| if d == "listen" { To::Listener } else { To::Process });
    Ok((slung.agent, slung.tokens, to))
}

/// The page under the new ink of `canvas`'s `page`, rendered with `render`
/// from the PDFs fetched into `pdfs`, into `dir`: one backdrop per area the
/// daemon names that renders. A daemon that does not say, a PDF not
/// fetched or an area not rendered leaves it out, and so do the areas past
/// half a request line.
fn backdrops(
    daemon: &Client,
    canvas: &str,
    page: Option<&str>,
    render: Render,
    pdfs: &Path,
    dir: &Path,
) -> Vec<Backdrop> {
    let areas = daemon.sling_backdrops(canvas, page).unwrap_or_default();
    if areas.is_empty() || fs::create_dir_all(dir).is_err() {
        return Vec::new();
    }
    let (mut out, mut room) = (Vec::new(), MAX_LINE / 2);
    for (n, area) in areas.iter().enumerate() {
        let Ok(pdf) = super::pdf::fetch(daemon, &area.hash, pdfs) else {
            continue;
        };
        let file = |ext: &str| -> PathBuf { dir.join(format!("{n}.{ext}")) };
        let (image, words) = (file("rgb"), file("json"));
        // The request is one line: half of it is the backdrops' at most,
        // each its paths and item plus its keys and numbers.
        let size = area.item.len() + 2 * image.as_os_str().len() + BACKDROP_JSON;
        if size > room {
            break;
        }
        if render(area, &pdf, &image, &words) {
            room -= size;
            let words = words.is_file().then_some(words);
            out.push(Backdrop { item: area.item.clone(), area: area.area, image, words });
        }
    }
    out
}

/// The way a sling went to its agent (`deliver` in the sling's reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum To {
    Process,
    /// It waits for a listener to take it.
    Listener,
}

/// What the window wants of the agent's `session` while a sling that went
/// `to` it is out (`None`: as the session says its next sling goes). The
/// session tells of what takes its next sling: when that is not what took
/// this one, the session does not count this sling's pending.
fn progress(session: &Session, to: Option<To>) -> Progress {
    let blocked = session.status.as_deref() == Some("blocked");
    let shown = if session.deliver == "listen" { To::Listener } else { To::Process };
    let to = to.unwrap_or(shown);
    // A sling a listener takes has no process to end: it is out until taken and answered.
    let listen = to == To::Listener;
    let running = listen || session.process.as_deref() == Some("running");
    // Not the session's to count: a process started since, which a sling
    // waiting for a listener still waits through, nobody listening; or a
    // listener came while the process works on it.
    let pending = if to == shown { session.pending.unwrap_or(0) } else { u32::from(running) };
    let listening = if to == shown { session.listening } else { Some(false) };
    Progress {
        running,
        pending,
        unheard: listen && pending > 0 && listening == Some(false),
        blocked: blocked.then(|| session.note.clone().unwrap_or_default()),
        answer: session.answer.clone(),
        profile: session.profile.clone(),
        // A process's context is not what a listener reads.
        tokens: session.context.filter(|_| to == shown),
    }
}

#[cfg(test)]
mod tests;
