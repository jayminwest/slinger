use super::*;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

const WINDOW: Rect = Rect { x: 0.0, y: 0.0, width: 960.0, height: 600.0 };

/// A workspace named for its agent, `<agent>-ws`, with that status.
fn ws(agent: &str, status: Option<Status>) -> Workspace {
    let agent = Agent { name: agent.into(), status, note: None };
    let canvas = format!("{}-canvas", agent.name);
    Workspace { name: format!("{}-ws", agent.name), canvas, agent: Some(agent) }
}

fn noted(agent: &str, status: Status, note: &str) -> Workspace {
    let mut w = ws(agent, Some(status));
    w.agent.as_mut().unwrap().note = Some(note.into());
    w
}

/// A sidebar that has heard `first`.
fn heard(first: Vec<Workspace>) -> Sidebar {
    let mut sidebar = Sidebar::new(true);
    assert!(sidebar.update(Ok(first)));
    sidebar
}

fn notice(sidebar: &Sidebar) -> Option<String> {
    sidebar.notice(&T, WINDOW).map(|l| l.text)
}

fn shown(labels: &[Label]) -> Vec<(&str, Rgb)> {
    labels.iter().map(|l| (l.text.as_str(), l.colour)).collect()
}

#[test]
fn reads_the_three_statuses_and_nothing_else() {
    assert_eq!(Status::parse("working"), Some(Status::Working));
    assert_eq!(Status::parse("blocked"), Some(Status::Blocked));
    assert_eq!(Status::parse("done"), Some(Status::Done));
    assert_eq!(Status::parse("Done"), None);
}

#[test]
fn open_it_lists_each_workspace_and_its_agents_mark() {
    let mut idle = ws("x", None);
    idle.name = "idle".into();
    let none = Workspace { name: "plain".into(), canvas: "plain".into(), agent: None };
    let sidebar = heard(vec![
        ws("ada", Some(Status::Working)),
        ws("bo", Some(Status::Blocked)),
        ws("cy", Some(Status::Done)),
        idle,
        none,
    ]);
    let rows = sidebar.rows(&T, WINDOW);
    assert_eq!(
        shown(&rows),
        [
            ("ada-ws", T.text),
            ("●", T.focus),
            ("ada", T.muted),
            ("bo-ws", T.text),
            ("●", T.error),
            ("bo", T.muted),
            ("cy-ws", T.text),
            ("✓", T.text),
            ("cy", T.muted),
            ("idle", T.text),
            ("○", T.muted),
            ("x", T.muted),
            ("plain", T.text),
        ]
    );
    // A mark and its agent share a line, indented under the workspace.
    let (name, mark, agent) = (rows[0].frame, rows[1].frame, rows[2].frame);
    assert!(mark.y > name.y);
    assert_eq!(mark.y.to_bits(), agent.y.to_bits());
    assert!(name.x < mark.x && mark.x + mark.width <= agent.x);
    assert!(rows.iter().all(|l| l.frame.x + l.frame.width <= WIDTH && l.fill.is_none()));
    assert_eq!(sidebar.panes(WINDOW), Rect { x: WIDTH, width: 960.0 - WIDTH, ..WINDOW });
}

#[test]
fn collapsed_it_shows_nothing_and_the_panes_take_the_window() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    sidebar.set_open(false);
    assert!(!sidebar.is_open());
    assert_eq!(sidebar.rows(&T, WINDOW), []);
    assert_eq!(sidebar.panes(WINDOW), WINDOW);
    sidebar.set_open(true);
    assert_eq!(sidebar.rows(&T, WINDOW).len(), 3);
}

#[test]
fn before_the_first_list_it_is_empty_and_with_none_it_says_so() {
    assert_eq!(Sidebar::new(true).rows(&T, WINDOW), []);
    assert_eq!(shown(&heard(vec![]).rows(&T, WINDOW)), [("no open workspaces", T.muted)]);
}

#[test]
fn each_canvas_has_the_status_of_its_workspaces_agent_for_its_button() {
    assert_eq!(Sidebar::new(false).statuses(), []);
    let none = Workspace { name: "plain".into(), canvas: "plain".into(), agent: None };
    let mut sidebar = heard(vec![ws("ada", Some(Status::Blocked)), ws("bo", None), none]);
    let blocked = ("ada-canvas".to_string(), Some(Status::Blocked));
    let quiet = [("bo-canvas".to_string(), None), ("plain".to_string(), None)];
    assert_eq!(sidebar.statuses(), [blocked, quiet[0].clone(), quiet[1].clone()]);
    sidebar.update(Err(crate::backend::Problem { message: "gone".into(), hint: None }));
    assert_eq!(sidebar.statuses(), [], "a lost daemon: no status");
}

#[test]
fn lines_that_do_not_fit_are_left_out() {
    let many: Vec<_> = (0..40).map(|i| ws(&format!("a{i}"), None)).collect();
    let rows = heard(many).rows(&T, WINDOW);
    assert!(rows.len() < 120);
    assert!(rows.iter().all(|l| l.frame.y + l.frame.height <= WINDOW.height));
}

#[test]
fn a_change_to_blocked_gives_a_notice_with_the_note() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    assert!(sidebar.update(Ok(vec![noted("ada", Status::Blocked, "which key?")])));
    let label = sidebar.notice(&T, WINDOW).unwrap();
    assert_eq!(label.text, "ada is blocked: which key?");
    assert_eq!((label.colour, label.fill), (T.background, Some(T.focus)));
    assert!(label.frame.y + label.frame.height <= WINDOW.height);
    assert!(label.frame.width <= NOTICE_WIDTH);
}

#[test]
fn a_change_to_working_gives_none() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    assert!(sidebar.update(Ok(vec![ws("ada", Some(Status::Working))])), "the mark changed");
    assert_eq!(notice(&sidebar), None);
}

#[test]
fn a_change_to_done_from_no_status_gives_a_notice() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert_eq!(notice(&sidebar).as_deref(), Some("ada is done"));
}

#[test]
fn what_was_so_at_the_first_list_or_for_a_new_agent_gives_none() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Blocked))]);
    assert_eq!(notice(&sidebar), None);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked)), ws("bo", Some(Status::Done))]));
    assert_eq!(notice(&sidebar), None);
}

#[test]
fn the_same_list_again_changes_nothing() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert!(!sidebar.update(Ok(vec![ws("ada", Some(Status::Done))])));
    assert_eq!(notice(&sidebar).as_deref(), Some("ada is done"));
}

#[test]
fn one_notice_names_each_agent_and_the_latest_change_wins() {
    let mut sidebar = heard(vec![ws("ada", None), ws("bo", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked)), ws("bo", None)]));
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked)), ws("bo", Some(Status::Done))]));
    assert_eq!(notice(&sidebar).as_deref(), Some("ada is blocked · bo is done"));
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done)), ws("bo", Some(Status::Done))]));
    assert_eq!(notice(&sidebar).as_deref(), Some("bo is done · ada is done"));
}

#[test]
fn an_agent_leaves_the_notice_when_its_status_moves_on() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked))]));
    sidebar.update(Ok(vec![ws("ada", Some(Status::Working))]));
    assert_eq!(notice(&sidebar), None);
}

#[test]
fn dismiss_hides_the_notice_until_the_next_change() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked))]));
    sidebar.dismiss();
    assert_eq!(notice(&sidebar), None);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert_eq!(notice(&sidebar).as_deref(), Some("ada is done"));
}

#[test]
fn a_lost_daemon_shows_its_problem_and_its_return_is_a_new_start() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Blocked))]));
    let lost = Problem { message: "no daemon".into(), hint: None };
    assert!(sidebar.update(Err(lost)));
    assert_eq!(notice(&sidebar), None);
    assert_eq!(shown(&sidebar.rows(&T, WINDOW)), [("✗ no daemon", T.error)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert_eq!(notice(&sidebar), None, "the first list after it");
    assert_eq!(sidebar.rows(&T, WINDOW).len(), 3);
}

#[test]
fn the_notice_sits_in_the_panes_area() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    let panes = sidebar.panes(WINDOW);
    let frame = sidebar.notice(&T, panes).unwrap().frame;
    assert!(frame.x >= panes.x && frame.x + frame.width <= panes.x + panes.width);
    let narrow = Rect { width: 10.0, ..WINDOW };
    assert!(sidebar.notice(&T, narrow).unwrap().frame.width.abs() < f64::EPSILON);
}
