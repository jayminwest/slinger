//! The `Launcher` with a stand-in `slingerd`: it starts one daemon at a time
//! only when none answers and none was stopped; `restart-daemon` stops a
//! daemon of another build and the bundled one answers in its place; one
//! that lacks `daemon_info` reads as old and is not stopped from the app.

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

/// What `launcher` said of the daemon, oldest first.
fn lines(launcher: &Launcher) -> Vec<Option<Daemon>> {
    let said = launcher.news.try_iter();
    said.filter_map(|s| if let Said::Daemon(d) = s { Some(d) } else { None }).collect()
}

/// What `launcher` said of the daemon, oldest first, until `done` holds for
/// the newest or ten seconds pass.
fn heard(launcher: &Launcher, done: impl Fn(&Option<Daemon>) -> bool) -> Vec<Option<Daemon>> {
    let (mut said, until) = (Vec::new(), Instant::now() + Duration::from_secs(10));
    while Instant::now() < until && !said.last().is_some_and(&done) {
        said.extend(lines(launcher));
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
    assert_eq!(lines(&launcher), [Some(Daemon::Starting)], "never failed");
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

/// All `launcher` told, oldest first, until one holds `done` or ten
/// seconds pass.
fn told(launcher: &Launcher, done: impl Fn(&Said) -> bool) -> Vec<Said> {
    let (mut said, until) = (Vec::new(), Instant::now() + Duration::from_secs(10));
    while Instant::now() < until && !said.iter().any(&done) {
        said.extend(launcher.news());
        thread::sleep(QUICK);
    }
    said
}

/// When the daemon on `socket` began to serve, once it answers.
fn started(socket: &Path) -> Option<u64> {
    Client::new(socket).daemon_info().ok().map(|i| i.started_ms)
}

#[test]
fn a_restart_stops_the_daemon_and_the_bundled_one_answers_in_its_place() {
    let (record, _sockets, socket) = places();
    let stoppers = Arc::new(Mutex::new(Vec::new()));
    let (theirs, at, dir) = (Arc::clone(&stoppers), socket.clone(), record.path().to_owned());
    let in_process: Spawn = Box::new(move |_: &Managed| {
        let store = daemon::Store::open(&dir, "my-laptop").map_err(io::Error::other)?;
        let server = daemon::Server::bind(&at).map_err(io::Error::other)?;
        theirs.lock().unwrap().push(server.stopper());
        let serving = thread::spawn(move || server.serve(&store));
        Ok(Box::new(Served(Some(serving))) as Box<dyn Started>)
    });
    let managed = Managed { program: stand_in(), data_dir: record.path().to_owned() };
    let launcher = Launcher::with(managed, socket.clone(), in_process, QUICK, &Wake::none());
    assert_eq!(heard(&launcher, Option::is_none), [Some(Daemon::Starting), None]);
    let first = started(&socket).expect("the first answers");
    thread::sleep(QUICK * 5);
    assert!(launcher.news().iter().all(|s| !matches!(s, Said::Offer(Some(_)))), "same build");

    launcher.restart();
    let until = Instant::now() + Duration::from_secs(10);
    let mut now = Some(first);
    while (now.is_none() || now == Some(first)) && Instant::now() < until {
        thread::sleep(QUICK);
        now = started(&socket);
    }
    assert!(now.is_some_and(|n| n != first), "a second daemon answers: {now:?} after {first}");
    assert!(control::stop_mark(record.path()).exists(), "the first stopped on purpose");
    assert_eq!(stoppers.lock().unwrap().len(), 2);
    drop(launcher);
    for stopper in stoppers.lock().unwrap().drain(..) {
        stopper.stop();
    }
}

/// A daemon older than `daemon_info`: it answers `bad_request` to every
/// request, on `socket`, until the test ends.
fn old_daemon(socket: &Path) {
    let listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            let mut reader = io::BufReader::new(&stream);
            if io::BufRead::read_line(&mut reader, &mut line).is_ok_and(|n| n > 0) {
                let reply = "{\"ok\":false,\"code\":\"bad_request\",\"error\":\"unknown op\"}\n";
                let _ = io::Write::write_all(&mut &stream, reply.as_bytes());
            }
        }
    });
}

#[test]
fn a_daemon_without_the_verb_reads_as_old_and_refuses_a_restart() {
    let (data, _sockets, socket) = places();
    old_daemon(&socket);
    let never: Spawn = Box::new(|_: &Managed| Err(io::Error::other("it answers: none is started")));
    let managed = Managed { program: stand_in(), data_dir: data.path().to_owned() };
    let launcher = Launcher::with(managed, socket, never, QUICK, &Wake::none());
    let said = told(&launcher, |s| matches!(s, Said::Offer(Some(_))));
    let old = Offer { pid: None, skew: Skew::Old { app: control::stamp() } };
    assert_eq!(said, [Said::Offer(Some(old))]);
    launcher.restart();
    let said = told(&launcher, |s| matches!(s, Said::Refused(_)));
    assert_eq!(said, [Said::Refused(said::OLD.into())]);
}

#[test]
fn the_offer_of_each_answer_to_daemon_info() {
    let info = |version: &str, build: &str| DaemonInfo {
        version: version.into(),
        build: build.into(),
        protocol: control::VERSION,
        pid: 42,
        started_ms: 1,
        exe: "/a/slingerd".into(),
        socket: "/s".into(),
        data_dir: "/d".into(),
        claude: None,
        slinger: "/a/slinger".into(),
        log: None,
    };
    let same = Ok(info(control::PACKAGE, &control::stamp()));
    assert_eq!(skew(&same), Some(Skew::Same));
    assert_eq!(offer(&same, Skew::Same), None);
    let other = Ok(info(control::PACKAGE, "0000000"));
    let differs = Skew::Differs { daemon: "0000000".into(), app: control::stamp() };
    assert_eq!(skew(&other), Some(differs.clone()));
    assert_eq!(offer(&other, differs.clone()), Some(Offer { pid: Some(42), skew: differs }));
    assert_eq!(skew(&Err(RemoteError::new("unavailable", "gone", None))), None);
}
