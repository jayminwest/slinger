//! The agents' listeners (SPEC section 5): the slings that wait for a
//! session to take them, and the wait of the one that asks.
//!
//! The core decides what waits and who counts as there
//! ([`slinger_core::session::listen`]). This module holds one listener per
//! agent behind a lock, reads the clock, and holds an ask open until a sling
//! comes, the wait is over or the daemon stops. It also looks at the process
//! a listener named: when that ends while it works on a sling, the listener
//! is gone and the sling waits again.
//!
//! Each sling is kept in the record ([`Waiting`]) from the time it waits
//! until its listener deals with it, so a daemon that stops loses none: the
//! next one starts with every kept sling waiting, the one a listener worked
//! on too, since that listener's claim did not survive the stop.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slinger_core::session::listen::{Heard, Listener};

use crate::store::{StoreError, Waiting};

type Result<T> = std::result::Result<T, StoreError>;

/// What a session shows of its agent's listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listening {
    /// A listener is there now.
    pub attached: bool,
    /// The slings it has still to deal with.
    pub pending: u32,
}

/// The listeners, by agent. Safe to share between threads.
#[derive(Debug, Default)]
pub struct Listeners {
    by_agent: Mutex<HashMap<String, Listener>>,
    /// A sling came, or the daemon stops.
    came: Condvar,
    stopping: AtomicBool,
    /// The agents whose listener was found gone, since [`Listeners::freed`].
    freed: Mutex<HashSet<String>>,
    /// Where the slings are kept; none for listeners that keep nothing.
    kept: Option<Waiting>,
}

impl Listeners {
    /// The listeners of the slings `kept` holds, each one waiting, oldest
    /// first, and nobody there.
    ///
    /// # Errors
    ///
    /// The record's error when it cannot be read.
    pub fn kept(kept: Waiting) -> Result<Self> {
        let mut by_agent: HashMap<String, Listener> = HashMap::new();
        for (agent, sling) in kept.all()? {
            by_agent.entry(agent).or_default().push(sling);
        }
        Ok(Self { by_agent: Mutex::new(by_agent), kept: Some(kept), ..Self::default() })
    }

    /// Adds a sling for `agent`'s listener and wakes the one that asks.
    ///
    /// # Errors
    ///
    /// The record's error when the sling cannot be kept: it does not wait.
    pub fn push(&self, agent: &str, sling: Heard) -> Result<()> {
        let mut all = self.lock();
        if let Some(kept) = &self.kept {
            kept.keep(agent, &sling)?;
        }
        all.entry(agent.to_string()).or_default().push(sling);
        self.came.notify_all();
        Ok(())
    }

    /// The oldest sling that waits for `agent`'s listener, to work on in the
    /// process `by` when it names one. With none, waits up to `wait` for
    /// one, or until [`Listeners::stop`]. Asking deals with the sling the
    /// listener worked on.
    ///
    /// # Errors
    ///
    /// The record's error when the sling dealt with cannot be forgotten.
    pub fn listen(&self, agent: &str, wait: Duration, by: Option<u32>) -> Result<Option<Heard>> {
        let deadline = Instant::now() + wait;
        let mut all = self.lock();
        all.entry(agent.to_string()).or_default().asked();
        let sling = loop {
            let taken = match all.get_mut(agent) {
                Some(listener) => self.end_work(listener).map(|()| listener.take(by)),
                None => Ok(None),
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if !matches!(taken, Ok(None)) || left.is_zero() || self.stopping.load(Ordering::SeqCst)
            {
                break taken;
            }
            all = self.came.wait_timeout(all, left).unwrap_or_else(PoisonError::into_inner).0;
        };
        if let Some(listener) = all.get_mut(agent) {
            listener.left(Instant::now());
        }
        sling
    }

    /// Puts back a sling whose reply never reached the listener that took it.
    pub fn put_back(&self, agent: &str, sling: Heard) {
        self.lock().entry(agent.to_string()).or_default().put_back(sling);
        self.came.notify_all();
    }

    /// `agent`'s listener said it dealt with the sling it took.
    ///
    /// # Errors
    ///
    /// The record's error when that sling cannot be forgotten.
    pub fn done(&self, agent: &str) -> Result<()> {
        if let Some(listener) = self.lock().get_mut(agent) {
            self.end_work(listener)?;
            listener.done(Instant::now());
        }
        Ok(())
    }

    /// Whether a listener of `agent`'s is there now.
    #[must_use]
    pub fn attached(&self, agent: &str) -> bool {
        self.lock().get(agent).is_some_and(|l| l.attached(Instant::now()))
    }

    /// `agent`'s listener, when one is there or slings wait for one.
    #[must_use]
    pub fn shown(&self, agent: &str) -> Option<Listening> {
        let all = self.lock();
        let listener = all.get(agent)?;
        let shown =
            Listening { attached: listener.attached(Instant::now()), pending: listener.pending() };
        (shown.attached || shown.pending > 0).then_some(shown)
    }

    /// Forgets who listened for `agent`; the slings that wait stay.
    ///
    /// # Errors
    ///
    /// The record's error when the sling it worked on cannot be forgotten.
    pub fn release(&self, agent: &str) -> Result<()> {
        if let Some(listener) = self.lock().get_mut(agent) {
            self.end_work(listener)?;
            listener.release();
        }
        Ok(())
    }

    /// The agents whose listener was found gone since the last call: each
    /// one's process ended while it worked on a sling, and the sling waits
    /// again.
    pub fn freed(&self) -> Vec<String> {
        let _all = self.lock();
        let mut freed = self.freed.lock().unwrap_or_else(PoisonError::into_inner);
        freed.drain().collect()
    }

    /// Ends every wait, now and from now on: each ask gets its reply.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        // Under the lock, so an ask between its check and its wait still wakes.
        let _all = self.lock();
        self.came.notify_all();
    }

    /// Forgets the sling `listener` works on, which it is about to deal
    /// with, from the record. Its process may have ended: then it is not
    /// dealt with, and the lock has already found the listener gone.
    fn end_work(&self, listener: &Listener) -> Result<()> {
        match (&self.kept, listener.working()) {
            (Some(kept), Some(event)) => kept.forget(event),
            _ => Ok(()),
        }
    }

    /// The listeners, each one whose process ended while it worked found gone.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Listener>> {
        let mut all = self.by_agent.lock().unwrap_or_else(PoisonError::into_inner);
        let gone: Vec<&String> = all
            .iter_mut()
            .filter(|(_, listener)| listener.worker().is_some_and(|pid| !alive(pid)))
            .map(|(agent, listener)| {
                listener.gone();
                agent
            })
            .collect();
        if !gone.is_empty() {
            let mut freed = self.freed.lock().unwrap_or_else(PoisonError::into_inner);
            freed.extend(gone.into_iter().cloned());
            self.came.notify_all();
        }
        all
    }
}

/// Whether the process `pid` is there. One of another user's is.
#[must_use]
pub fn alive(pid: u32) -> bool {
    let pid = i32::try_from(pid).ok().and_then(rustix::process::Pid::from_raw);
    pid.is_some_and(|pid| match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(e) => e != rustix::io::Errno::SRCH,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    const LONG: Duration = Duration::from_secs(30);

    fn sling(event: u64) -> Heard {
        Heard { event, canvas: "idea".into(), dir: "/p".into(), text: format!("sling {event}") }
    }

    fn listen(listeners: &Listeners, agent: &str, wait: Duration) -> Option<Heard> {
        listeners.listen(agent, wait, None).unwrap()
    }

    #[test]
    fn an_ask_takes_a_waiting_sling_at_once_and_an_empty_wait_ends() {
        let listeners = Listeners::default();
        assert_eq!(listeners.shown("scout"), None);
        listeners.push("scout", sling(1)).unwrap();
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: false, pending: 1 }));
        assert_eq!(listen(&listeners, "ada", Duration::ZERO), None, "another agent's sling");
        assert_eq!(listen(&listeners, "scout", LONG), Some(sling(1)));
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: true, pending: 1 }));
        let began = Instant::now();
        assert_eq!(listen(&listeners, "scout", Duration::from_millis(30)), None);
        assert!(began.elapsed() >= Duration::from_millis(30));
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: true, pending: 0 }));
        listeners.release("scout").unwrap();
        assert!(!listeners.attached("scout"));
    }

    #[test]
    fn an_open_ask_wakes_for_a_sling_and_for_a_stop() {
        let listeners = Listeners::default();
        std::thread::scope(|scope| {
            let ask = scope.spawn(|| listen(&listeners, "scout", LONG));
            while !listeners.attached("scout") {
                std::thread::yield_now();
            }
            listeners.push("scout", sling(7)).unwrap();
            assert_eq!(ask.join().unwrap(), Some(sling(7)));
            let ask = scope.spawn(|| listen(&listeners, "scout", LONG));
            while listeners.shown("scout").is_none_or(|s| s.pending > 0) {
                std::thread::yield_now();
            }
            listeners.stop();
            assert_eq!(ask.join().unwrap(), None);
        });
        assert_eq!(listen(&listeners, "scout", LONG), None, "stopped for good");
        listeners.put_back("scout", sling(7));
        assert_eq!(listen(&listeners, "scout", Duration::ZERO), Some(sling(7)));
    }

    #[test]
    fn the_slings_not_dealt_with_wait_again_after_a_restart_the_one_worked_on_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), "test-machine").unwrap();
        let listeners = store.sessions().listeners();
        for event in 1..=5 {
            listeners.push("scout", sling(event)).unwrap();
        }
        listeners.push("ada", sling(6)).unwrap();
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(1)));
        listeners.done("scout").unwrap();
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(2)));
        // Asking again deals with 2; 3 is put back unread; 4 is released.
        let taken = listen(listeners, "scout", Duration::ZERO).unwrap();
        listeners.put_back("scout", taken);
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(3)));
        listeners.done("scout").unwrap();
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(4)));
        listeners.release("scout").unwrap();
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(5)), "worked on");
        drop(store);

        let store = Store::open(dir.path(), "test-machine").unwrap();
        let listeners = store.sessions().listeners();
        let waiting = Some(Listening { attached: false, pending: 1 });
        assert_eq!(listeners.shown("scout"), waiting, "nobody works on it");
        assert_eq!(listen(listeners, "scout", Duration::ZERO), Some(sling(5)));
        assert_eq!(listen(listeners, "scout", Duration::ZERO), None);
        assert_eq!(listen(listeners, "ada", Duration::ZERO), Some(sling(6)));
        drop(store);
        let store = Store::open(dir.path(), "test-machine").unwrap();
        assert_eq!(store.sessions().listeners().shown("scout"), None, "asking dealt with it");
        assert!(store.sessions().listeners().shown("ada").is_some(), "ada works on hers");
    }
}
