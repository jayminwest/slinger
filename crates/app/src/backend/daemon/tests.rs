use std::fs;
use std::thread::{self, JoinHandle};

use slingerd as daemon;

use super::*;
use crate::backend::Watch;

/// How often the tests' launchers look.
const QUICK: Duration = Duration::from_millis(20);

/// The kept stand-in for slingerd (`daemon/slingerd`, which says what it does).
fn stand_in() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend/daemon/slingerd")
}

/// What `launcher` said, oldest first, until `done` holds for the newest or
/// ten seconds pass.
fn heard(launcher: &Launcher, done: impl Fn(&Option<Daemon>) -> bool) -> Vec<Option<Daemon>> {
    let (mut said, until) = (Vec::new(), Instant::now() + Duration::from_secs(10));
    while Instant::now() < until && !said.last().is_some_and(&done) {
        said.extend(launcher.news.try_iter().map(|s| s.0));
        thread::sleep(QUICK);
    }
    said
}

/// A record and a socket nobody serves, in fresh directories (short: socket
/// paths have a length limit).
fn places() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let (data, sockets) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let socket = sockets.path().join("d.sock");
    (data, sockets, socket)
}

#[test]
fn the_bundled_daemon_is_run_with_its_log_and_a_lost_race_is_no_failure() {
    let (data, _sockets, socket) = places();
    fs::write(data.path().join("exit"), "4").unwrap();
    let managed = Managed { program: stand_in(), data_dir: data.path().to_owned() };
    let launcher = Launcher::with(managed, socket, Box::new(spawn), QUICK, &Wake::none());
    let argv = data.path().join("argv");
    let until = Instant::now() + Duration::from_secs(10);
    let written = || fs::read_to_string(&argv).is_ok_and(|a| a.lines().count() == 2);
    while !written() && Instant::now() < until {
        thread::sleep(QUICK);
    }
    let log = data.path().join("slingerd.log");
    assert_eq!(fs::read_to_string(&argv).unwrap(), format!("--log\n{}\n", log.display()));
    // Several looks after it exited 4: still waiting for the daemon that won.
    thread::sleep(QUICK * 10);
    let said: Vec<_> = launcher.news.try_iter().map(|s| s.0).collect();
    assert_eq!(said, [Some(Daemon::Starting)], "never failed");
}

#[test]
fn a_daemon_that_exits_with_an_error_did_not_start_and_says_why() {
    let (data, _sockets, socket) = places();
    fs::write(data.path().join("exit"), "1").unwrap();
    fs::write(data.path().join("say"), "✗ cannot open the record\n  pass --data-dir\n").unwrap();
    let managed = Managed { program: stand_in(), data_dir: data.path().to_owned() };
    let log = managed.log().display().to_string();
    let launcher = Launcher::with(managed, socket, Box::new(spawn), QUICK, &Wake::none());
    let said = heard(&launcher, |d| matches!(d, Some(Daemon::Failed { .. })));
    let why = Some("✗ cannot open the record".to_string());
    assert_eq!(
        said,
        [Some(Daemon::Starting), Some(Daemon::Failed { code: Some(1), said: why, log })]
    );
}

#[test]
fn a_program_that_cannot_run_did_not_start() {
    let (data, _sockets, socket) = places();
    let program = data.path().join("no-slingerd");
    let managed = Managed { program, data_dir: data.path().to_owned() };
    let launcher = Launcher::with(managed, socket, Box::new(spawn), QUICK, &Wake::none());
    let said = heard(&launcher, |d| matches!(d, Some(Daemon::Failed { .. })));
    let Some(Some(Daemon::Failed { code: None, said: Some(_), .. })) = said.last() else {
        panic!("{said:?}")
    };
}

/// A daemon served by the library on a thread, as the bundled one would
/// serve in its own process.
struct Served(Option<JoinHandle<io::Result<()>>>);

impl Started for Served {
    fn exited(&mut self) -> Option<(Option<i32>, Option<String>)> {
        let ended = self.0.as_ref()?.is_finished();
        let ended = ended.then(|| self.0.take()?.join().ok())??;
        Some((Some(i32::from(ended.is_err())), None))
    }
}

#[test]
fn a_started_daemon_that_answers_is_up_and_the_window_hears_it() {
    let (data, _sockets, socket) = places();
    let record = tempfile::tempdir().unwrap();
    let stoppers = Arc::new(Mutex::new(Vec::new()));
    let (theirs, at, dir) = (Arc::clone(&stoppers), socket.clone(), record.path().to_owned());
    let in_process: Spawn = Box::new(move |_: &Managed| {
        let store = daemon::Store::open(&dir, "my-laptop").map_err(io::Error::other)?;
        let server = daemon::Server::bind(&at).map_err(io::Error::other)?;
        theirs.lock().unwrap().push(server.stopper());
        let serving = thread::spawn(move || server.serve(&store));
        Ok(Box::new(Served(Some(serving))) as Box<dyn Started>)
    });
    let managed = Managed { program: stand_in(), data_dir: data.path().to_owned() };
    let launcher = Launcher::with(managed, socket.clone(), in_process, QUICK, &Wake::none());
    assert_eq!(heard(&launcher, Option::is_none), [Some(Daemon::Starting), None]);
    let watch = Watch::start(socket, None, None, &Wake::none());
    let until = Instant::now() + Duration::from_secs(10);
    let mut workspaces = None;
    while workspaces.is_none() && Instant::now() < until {
        workspaces = watch.latest().filter(Result::is_ok);
        thread::sleep(QUICK);
    }
    assert_eq!(workspaces, Some(Ok(Vec::new())));
    drop(watch);
    drop(launcher);
    for stopper in stoppers.lock().unwrap().drain(..) {
        stopper.stop();
    }
    assert_eq!(stoppers.lock().unwrap().len(), 0);
}
