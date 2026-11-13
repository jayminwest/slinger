//! The agents' listeners (SPEC section 5): the slings that wait for a
//! session to take them, and the wait of the one that asks.
//!
//! The core decides what waits and who counts as there
//! ([`slinger_core::session::listen`]). This module holds one listener per
//! agent behind a lock, reads the clock, and holds an ask open until a sling
//! comes, the wait is over or the daemon stops. It also looks at the process
//! a listener named: when that ends while it works on a sling, the listener
//! is gone and the sling waits again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slinger_core::session::listen::{Heard, Listener};

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
}

impl Listeners {
    /// Adds a sling for `agent`'s listener and wakes the one that asks.
    pub fn push(&self, agent: &str, sling: Heard) {
        self.lock().entry(agent.to_string()).or_default().push(sling);
        self.came.notify_all();
    }

    /// The oldest sling that waits for `agent`'s listener, to work on in the
    /// process `by` when it names one. With none, waits up to `wait` for
    /// one, or until [`Listeners::stop`].
    pub fn listen(&self, agent: &str, wait: Duration, by: Option<u32>) -> Option<Heard> {
        let deadline = Instant::now() + wait;
        let mut all = self.lock();
        all.entry(agent.to_string()).or_default().asked();
        let sling = loop {
            let taken = all.get_mut(agent).and_then(|listener| listener.take(by));
            let left = deadline.saturating_duration_since(Instant::now());
            if taken.is_some() || left.is_zero() || self.stopping.load(Ordering::SeqCst) {
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
    pub fn done(&self, agent: &str) {
        if let Some(listener) = self.lock().get_mut(agent) {
            listener.done(Instant::now());
        }
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

    /// Forgets who listened for `agent`; its slings stay.
    pub fn release(&self, agent: &str) {
        if let Some(listener) = self.lock().get_mut(agent) {
            listener.release();
        }
    }

    /// Ends every wait, now and from now on: each ask gets its reply.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        // Under the lock, so an ask between its check and its wait still wakes.
        let _all = self.lock();
        self.came.notify_all();
    }

    /// The listeners, each one whose process ended while it worked found gone.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Listener>> {
        let mut all = self.by_agent.lock().unwrap_or_else(PoisonError::into_inner);
        let mut freed = false;
        for listener in all.values_mut() {
            if listener.worker().is_some_and(|pid| !alive(pid)) {
                listener.gone();
                freed = true;
            }
        }
        if freed {
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

    fn sling(event: u64) -> Heard {
        Heard { event, canvas: "idea".into(), dir: "/p".into(), text: String::new() }
    }

    #[test]
    fn an_ask_takes_a_waiting_sling_at_once_and_an_empty_wait_ends() {
        let listeners = Listeners::default();
        assert_eq!(listeners.shown("scout"), None);
        listeners.push("scout", sling(1));
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: false, pending: 1 }));
        assert_eq!(listeners.listen("ada", Duration::ZERO, None), None, "another agent's sling");
        assert_eq!(listeners.listen("scout", Duration::from_secs(30), None), Some(sling(1)));
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: true, pending: 1 }));
        let began = Instant::now();
        assert_eq!(listeners.listen("scout", Duration::from_millis(30), None), None);
        assert!(began.elapsed() >= Duration::from_millis(30));
        assert_eq!(listeners.shown("scout"), Some(Listening { attached: true, pending: 0 }));
        listeners.release("scout");
        assert!(!listeners.attached("scout"));
    }

    #[test]
    fn an_open_ask_wakes_for_a_sling_and_for_a_stop() {
        let listeners = Listeners::default();
        std::thread::scope(|scope| {
            let ask = scope.spawn(|| listeners.listen("scout", Duration::from_secs(30), None));
            while !listeners.attached("scout") {
                std::thread::yield_now();
            }
            listeners.push("scout", sling(7));
            assert_eq!(ask.join().unwrap(), Some(sling(7)));
            let ask = scope.spawn(|| listeners.listen("scout", Duration::from_secs(30), None));
            while listeners.shown("scout").is_none_or(|s| s.pending > 0) {
                std::thread::yield_now();
            }
            listeners.stop();
            assert_eq!(ask.join().unwrap(), None);
        });
        assert_eq!(
            listeners.listen("scout", Duration::from_secs(30), None),
            None,
            "stopped for good"
        );
        listeners.put_back("scout", sling(7));
        assert_eq!(listeners.listen("scout", Duration::ZERO, None), Some(sling(7)));
    }
}
