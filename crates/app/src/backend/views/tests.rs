//! A pane's page, pan and zoom and the focus go to the daemon: one request
//! per burst, only what changed, at once on a switch, bounded at quit.

use std::time::Instant;

use slinger_control::PaneView;

use super::*;
use crate::backend::testing::wait_until;

/// What a test's send function was asked: the workspace, the ids of the
/// views and the focus, in order.
type Asked = Arc<Mutex<Vec<(String, Vec<u32>, Option<u32>)>>>;

/// A saver over a send that notes each request and answers `takes`.
fn noting(wait: Duration, takes: bool) -> (Saver, Asked) {
    let asked: Asked = Arc::default();
    let mine = Arc::clone(&asked);
    let send = move |w: &str, views: &[PaneViewed], focus| {
        let ids = views.iter().map(|v| v.id).collect();
        mine.lock().unwrap().push((w.to_string(), ids, focus));
        takes
    };
    (Saver::over(wait, Box::new(send)), asked)
}

fn view(id: u32, x: f64) -> PaneViewed {
    let view = PaneView { canvas: "main".into(), page: None, x, y: 0.0, zoom: 1.0 };
    PaneViewed { id, view }
}

fn looks(workspace: &str, views: Vec<PaneViewed>, focus: u32) -> Looks {
    Looks { workspace: workspace.into(), views, focus: Some(focus) }
}

fn requests(asked: &Asked) -> Vec<(String, Vec<u32>, Option<u32>)> {
    asked.lock().unwrap().clone()
}

#[test]
fn many_looks_inside_the_wait_make_one_request_with_only_the_changed_panes() {
    let (mut saver, asked) = noting(Duration::from_millis(200), true);
    saver.hand(looks("a", vec![view(1, 0.0), view(2, 0.0)], 1));
    wait_until(|| requests(&asked).len() == 1);
    assert_eq!(requests(&asked)[0], ("a".into(), vec![1, 2], Some(1)), "the first: all");
    for x in 1..20 {
        saver.hand(looks("a", vec![view(1, 0.0), view(2, f64::from(x))], 1));
    }
    saver.hand(looks("a", vec![view(1, 0.0), view(2, 19.0)], 1));
    wait_until(|| requests(&asked).len() == 2);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(requests(&asked)[1..], [("a".into(), vec![2], None)], "one, pane 2 only");
    saver.hand(looks("a", vec![view(1, 0.0), view(2, 19.0)], 2));
    wait_until(|| requests(&asked).len() == 3);
    assert_eq!(requests(&asked)[2], ("a".into(), vec![], Some(2)), "only the focus moved");
}

#[test]
fn looks_of_another_workspace_send_the_waiting_ones_at_once() {
    let (mut saver, asked) = noting(Duration::from_secs(30), true);
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    let switched = Instant::now();
    saver.hand(looks("b", vec![view(1, 0.0)], 1));
    wait_until(|| requests(&asked).len() == 1);
    assert!(switched.elapsed() < Duration::from_secs(10), "not the wait");
    assert_eq!(requests(&asked)[0].0, "a");
}

#[test]
fn a_failed_send_is_tried_again_with_the_next_looks() {
    let (mut saver, asked) = noting(Duration::from_millis(50), false);
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    wait_until(|| requests(&asked).len() == 1);
    saver.hand(looks("a", vec![view(1, 5.0), view(2, 0.0)], 1));
    wait_until(|| requests(&asked).len() == 2);
    assert_eq!(requests(&asked)[1], ("a".into(), vec![1, 2], Some(1)), "still unsent");
}

#[test]
fn the_flush_at_quit_sends_at_once_and_returns_within_its_bound_when_nobody_answers() {
    let (mut saver, asked) = noting(Duration::from_secs(30), true);
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    let asked_at = Instant::now();
    assert!(saver.flush(Duration::from_secs(10)));
    assert!(asked_at.elapsed() < Duration::from_secs(10));
    assert_eq!(requests(&asked).len(), 1);

    // A daemon that never answers: the send blocks.
    let hung = move |_: &str, _: &[PaneViewed], _| {
        std::thread::sleep(Duration::from_secs(60));
        true
    };
    let mut saver = Saver::over(Duration::from_secs(30), Box::new(hung));
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    let quit = Instant::now();
    assert!(!saver.flush(Duration::from_millis(200)));
    assert!(quit.elapsed() < Duration::from_secs(5), "bounded: {:?}", quit.elapsed());
}

#[test]
fn the_same_looks_are_handed_once() {
    let (mut saver, asked) = noting(Duration::from_millis(20), true);
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    assert!(saver.flush(Duration::from_secs(10)));
    saver.hand(looks("a", vec![view(1, 5.0)], 1));
    assert!(saver.flush(Duration::from_secs(10)));
    assert_eq!(requests(&asked).len(), 1);
    assert!(format!("{saver:?}").starts_with("Saver"));
}
