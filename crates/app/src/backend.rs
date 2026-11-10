//! The one module that talks to the daemon (docs/service.md). It turns the
//! daemon's errors into what the window can show.
use slinger_control::Client;

/// A failure as the window shows it: the message and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub hint: Option<String>,
}

/// Blocks until the daemon answers or its client gives up. Call it before the
/// run loop starts or from a worker thread, never from an `AppKit` callback.
pub fn greeting(daemon: &Client, name: &str) -> Result<String, Problem> {
    daemon
        .hello(name)
        .map_err(|e| Problem { message: e.to_string(), hint: e.hint().map(str::to_string) })
}

#[cfg(test)]
mod tests {
    use slingerd as daemon;

    use super::*;

    /// Runs `f` against a daemon that lives for this one call.
    fn with_daemon<T>(f: impl FnOnce(&Client) -> T) -> T {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let socket = std::env::temp_dir().join(format!("a-{}-{n}.sock", std::process::id()));
        let data = tempfile::tempdir().unwrap();
        let store = daemon::Store::open(data.path(), "my-laptop").unwrap();
        let server = daemon::Server::bind(&socket).unwrap();
        let stopper = server.stopper();
        let serving = std::thread::spawn(move || server.serve(&store));
        let result = f(&Client::new(&socket));
        stopper.stop();
        serving.join().unwrap().unwrap();
        std::fs::remove_file(format!("{}.lock", socket.display())).unwrap();
        result
    }

    #[test]
    fn returns_the_daemons_greeting() {
        assert_eq!(with_daemon(|d| greeting(d, "ada")), Ok("hello, ada".to_string()));
    }

    #[test]
    fn a_daemon_error_becomes_a_problem_with_its_hint() {
        let problem = with_daemon(|d| greeting(d, " ")).unwrap_err();
        assert_eq!(problem.message, "name is empty");
        assert!(problem.hint.is_some());
    }

    #[test]
    fn no_daemon_is_a_problem_that_says_how_to_start_it() {
        let problem = greeting(&Client::new("/nonexistent/x.sock"), "ada").unwrap_err();
        assert!(problem.hint.unwrap().starts_with("start it with '"));
    }
}
