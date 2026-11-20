//! Who starts a daemon: a bare `slinger-app`, a driven window and an app
//! whose config names a socket never do; a bundled app starts its own
//! `slingerd` only when none answers and none was stopped on purpose, one at
//! a time; one stopped on purpose stays stopped until asked; a restart
//! starts the bundled daemon once the old one is gone.

use super::*;

const DOWN: Seen = Seen { answers: false, stopped: false };
const UP: Seen = Seen { answers: true, stopped: false };
const MARKED: Seen = Seen { answers: false, stopped: true };
const LOG: &str = "/data/slingerd.log";

fn shown(launch: &Launch) -> Option<Daemon> {
    launch.shown(Path::new(LOG))
}

fn failed(code: Option<i32>, said: Option<&str>) -> Daemon {
    Daemon::Failed { code, said: said.map(str::to_string), log: LOG.into() }
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn only_an_app_inside_a_bundle_has_a_bundled_daemon() {
    let app = Path::new("/Applications/slinger.app/Contents/MacOS/slinger-app");
    let daemon = PathBuf::from("/Applications/slinger.app/Contents/MacOS/slingerd");
    assert_eq!(bundled(app), Some(daemon));
    for bare in [
        "target/debug/slinger-app",
        "/repo/target/release/slinger-app",
        "/x/Contents/MacOS/slinger-app",
        "/x.app/Contents/Resources/slinger-app",
        "/x.app/Stuff/MacOS/slinger-app",
        "/x.app/Contents/MacOS/slinger",
        "slinger-app",
        "/",
    ] {
        assert_eq!(bundled(Path::new(bare)), None, "{bare}");
    }
}

fn found() -> Found {
    Found {
        exe: "/A/slinger.app/Contents/MacOS/slinger-app".into(),
        socket_set: false,
        driven: false,
        data_dir: Some("/data".into()),
    }
}

#[test]
fn a_bundled_app_with_no_socket_set_manages_the_default_record() {
    let managed = managed(found(), |p| p == Path::new("/A/slinger.app/Contents/MacOS/slingerd"));
    let program = "/A/slinger.app/Contents/MacOS/slingerd".into();
    let want = Managed { program, data_dir: "/data".into() };
    assert_eq!(managed, Some(want.clone()));
    assert_eq!(want.log(), PathBuf::from(LOG));
}

#[test]
fn a_bare_binary_a_set_socket_a_driven_window_or_no_daemon_file_never_starts_one() {
    let file = |_: &Path| true;
    let bare = Found { exe: "/repo/target/debug/slinger-app".into(), ..found() };
    assert_eq!(managed(bare, file), None, "a bare slinger-app");
    assert_eq!(managed(Found { socket_set: true, ..found() }, file), None, "socket set");
    assert_eq!(managed(Found { driven: true, ..found() }, file), None, "driven");
    assert_eq!(managed(Found { data_dir: None, ..found() }, file), None, "no data dir");
    assert_eq!(managed(found(), |_| false), None, "no slingerd in the bundle");
}

#[test]
fn a_daemon_that_answers_is_left_alone() {
    let (mut launch, now) = (Launch::new(), Instant::now());
    assert_eq!(shown(&launch), None, "not looked yet");
    assert!(!launch.look(UP, now));
    assert_eq!(shown(&launch), None);
}

#[test]
fn none_answering_starts_one_and_waits_until_it_answers() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(DOWN, t0), "start");
    assert_eq!(shown(&launch), Some(Daemon::Starting));
    assert!(!launch.look(DOWN, t0 + secs(1)), "one at a time");
    assert!(!launch.look(DOWN, t0 + secs(9)));
    assert_eq!(shown(&launch), Some(Daemon::Starting));
    assert!(!launch.look(UP, t0 + secs(9)));
    assert_eq!(shown(&launch), None);
}

#[test]
fn ten_seconds_with_no_answer_fails_and_thirty_more_try_again() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(DOWN, t0));
    assert!(!launch.look(DOWN, t0 + START_WAIT));
    assert_eq!(shown(&launch), Some(failed(None, None)));
    // Still running: waited for again, not started twice.
    assert!(!launch.look(DOWN, t0 + START_WAIT + RETRY));
    assert_eq!(shown(&launch), Some(Daemon::Starting));
    launch.exited(Some(1), None, t0 + secs(45));
    assert_eq!(shown(&launch), Some(failed(Some(1), None)));
    assert!(!launch.look(DOWN, t0 + secs(44) + RETRY), "too soon");
    assert!(launch.look(DOWN, t0 + secs(45) + RETRY), "again");
}

#[test]
fn an_exit_while_starting_fails_with_its_code_but_a_lost_race_waits() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(DOWN, t0));
    launch.exited(Some(LOCKED), Some("✗ taken".into()), t0 + secs(1));
    assert_eq!(shown(&launch), Some(Daemon::Starting), "the other daemon is coming");
    assert!(!launch.look(UP, t0 + secs(2)));
    assert_eq!(shown(&launch), None);

    let mut launch = Launch::new();
    assert!(launch.look(DOWN, t0));
    launch.exited(Some(1), Some("✗ bad data dir".into()), t0 + secs(1));
    assert_eq!(shown(&launch), Some(failed(Some(1), Some("✗ bad data dir"))));
    launch.exited(None, None, t0 + secs(2));
    assert_eq!(shown(&launch), Some(failed(Some(1), Some("✗ bad data dir"))), "not starting");
}

#[test]
fn a_daemon_that_died_is_started_again() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(!launch.look(UP, t0));
    assert!(launch.look(DOWN, t0 + secs(60)));
    assert_eq!(shown(&launch), Some(Daemon::Starting));
}

#[test]
fn a_daemon_stopped_on_purpose_stays_stopped_until_asked() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(!launch.look(UP, t0));
    assert!(!launch.look(MARKED, t0 + secs(1)));
    assert_eq!(shown(&launch), Some(Daemon::Stopped));
    assert!(!launch.look(MARKED, t0 + secs(100)));
    launch.ask();
    assert!(launch.look(MARKED, t0 + secs(101)), "start-slingerd");
    assert_eq!(shown(&launch), Some(Daemon::Starting));
}

#[test]
fn opening_the_app_asks_even_for_a_stopped_daemon() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(MARKED, t0));
    assert!(!launch.look(UP, t0 + secs(1)));
    assert!(!launch.look(MARKED, t0 + secs(2)), "used up once it answered");
    assert_eq!(shown(&launch), Some(Daemon::Stopped));
}

#[test]
fn a_mark_that_went_away_is_a_daemon_that_died() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(!launch.look(UP, t0));
    assert!(!launch.look(MARKED, t0 + secs(1)));
    assert!(launch.look(DOWN, t0 + secs(2)));
}

#[test]
fn asking_after_a_failure_tries_again_at_once() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(DOWN, t0));
    launch.exited(Some(1), None, t0 + secs(1));
    launch.ask();
    assert!(launch.look(DOWN, t0 + secs(2)));
}

#[test]
fn a_restart_starts_the_bundled_daemon_once_the_old_one_is_gone_despite_its_mark() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(!launch.look(UP, t0));
    assert!(launch.is_up());
    launch.restart();
    assert!(!launch.look(UP, t0 + secs(1)), "still stopping");
    assert!(launch.look(MARKED, t0 + secs(2)), "gone: start the bundled one");
    assert_eq!(shown(&launch), Some(Daemon::Starting));
    assert!(!launch.look(UP, t0 + secs(3)));
    assert!(!launch.look(MARKED, t0 + secs(4)), "a later stop on purpose stays stopped");
    assert_eq!(shown(&launch), Some(Daemon::Stopped));
}

#[test]
fn a_started_daemon_still_ending_is_reaped_before_the_next_start() {
    let (mut launch, t0) = (Launch::new(), Instant::now());
    assert!(launch.look(DOWN, t0));
    assert!(!launch.look(UP, t0 + secs(1)));
    launch.restart();
    assert!(!launch.look(MARKED, t0 + secs(2)), "its child has not exited");
    assert!(launch.is_up());
    launch.exited(Some(0), None, t0 + secs(3));
    assert!(launch.look(MARKED, t0 + secs(3)), "reaped: start");
    assert_eq!(shown(&launch), Some(Daemon::Starting));
}
