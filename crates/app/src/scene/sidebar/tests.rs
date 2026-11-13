use super::*;
use crate::layout::Point;
use crate::scene::glyphs;
use crate::scene::taps::Tap;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

const WINDOW: Rect = Rect { x: 0.0, y: 0.0, width: 960.0, height: 600.0 };

/// A workspace named for its agent, `<agent>-ws`, with that status.
fn ws(agent: &str, status: Option<Status>) -> Workspace {
    let agent = Agent { name: agent.into(), status, note: None, ..Agent::default() };
    let canvas = format!("{}-canvas", agent.name);
    Workspace {
        name: format!("{}-ws", agent.name),
        canvas,
        agent: Some(agent),
        ..Workspace::default()
    }
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
    sidebar.notice(&T, WINDOW).first().map(|l| l.text.clone())
}

/// Each row's text and colour, less the shown workspace's background.
fn shown(rows: &[Row]) -> Vec<(&str, Rgb)> {
    let texts = rows.iter().filter(|r| r.label.fill.is_none());
    texts.map(|r| (r.label.text.as_str(), r.label.colour)).collect()
}

#[test]
fn reads_the_three_statuses_and_nothing_else() {
    assert_eq!(Status::parse("working"), Some(Status::Working));
    assert_eq!(Status::parse("blocked"), Some(Status::Blocked));
    assert_eq!(Status::parse("done"), Some(Status::Done));
    assert_eq!(Status::parse("Done"), None);
}

#[test]
fn open_it_lists_each_workspace_under_a_heading_with_its_agents_mark_and_status() {
    let mut idle = ws("x", None);
    idle.name = "idle".into();
    let none = Workspace { name: "plain".into(), canvas: "plain".into(), ..Workspace::default() };
    let sidebar = heard(vec![
        noted("ada", Status::Working, "only a blocked note shows"),
        noted("bo", Status::Blocked, "which key?"),
        ws("cy", Some(Status::Done)),
        idle,
        none,
    ]);
    let rows = sidebar.rows(&T, WINDOW);
    assert_eq!(
        shown(&rows),
        [
            ("WORKSPACES", T.muted),
            ("ada-ws", T.text),
            ("●", T.focus),
            ("ada · working", T.muted),
            ("bo-ws", T.text),
            ("■", T.error),
            ("bo · blocked", T.muted),
            ("which key?", T.muted),
            ("cy-ws", T.text),
            ("●", T.done),
            ("cy · done", T.muted),
            ("idle", T.text),
            ("○", T.muted),
            ("x", T.muted),
            ("plain", T.text),
            ("no agent", T.muted),
        ]
    );
    let heading = rows.iter().find(|r| r.label.text == "WORKSPACES").unwrap().font;
    assert_eq!(heading, Font { size: Some(11.0), bold: false, spacing: 1.0 });
    // A mark and its agent share a line, set in under the workspace; the
    // note and `no agent` line up with the agent.
    let at = |text: &str| rows.iter().find(|r| r.label.text == text).unwrap().label.frame;
    let (name, mark, agent, why) = (at("bo-ws"), at("■"), at("bo · blocked"), at("which key?"));
    assert!(mark.y > name.y && why.y > agent.y);
    assert_eq!(mark.y.to_bits(), agent.y.to_bits());
    assert!(name.x <= mark.x && mark.x + mark.width <= agent.x);
    assert_eq!(
        (why.x.to_bits(), at("no agent").x.to_bits()),
        (agent.x.to_bits(), agent.x.to_bits())
    );
    assert!(rows.iter().all(|r| r.label.frame.x + r.label.frame.width <= WIDTH));
    assert_eq!(sidebar.panes(WINDOW), Rect { x: WIDTH, width: 960.0 - WIDTH, ..WINDOW });
}

#[test]
fn the_shown_workspace_is_bold_on_a_lighter_row() {
    let mut sidebar = heard(vec![noted("ada", Status::Blocked, "why"), ws("bo", None)]);
    let lit = |sidebar: &Sidebar| {
        let rows = sidebar.rows(&T, WINDOW);
        let fills: Vec<_> = rows.iter().filter(|r| r.label.fill.is_some()).cloned().collect();
        let bold: Vec<_> =
            rows.iter().filter(|r| r.font.bold).map(|r| r.label.text.clone()).collect();
        (rows, fills, bold)
    };
    let (rows, fills, bold) = lit(&sidebar);
    assert_eq!(bold, ["ada-ws"], "unset: the first");
    let [fill] = fills.as_slice() else { panic!("one row lit: {fills:?}") };
    assert_eq!((fill.label.text.as_str(), fill.label.fill), ("", Some(lighter(&T))));
    let theme = Theme::default();
    assert!(lighter(&theme).r > theme.background.r && lighter(&theme).r < theme.text.r);
    assert_eq!(rows[0], *fill, "drawn first, under its text");
    let row = |t: &str| rows.iter().find(|r| r.label.text == t).unwrap().label.frame;
    let (frame, why) = (fill.label.frame, row("why"));
    assert_eq!((frame.x, frame.width), (WINDOW.x, WIDTH));
    assert_eq!(frame.y.to_bits(), row("ada-ws").y.to_bits());
    assert_eq!((frame.y + frame.height).to_bits(), (why.y + why.height).to_bits(), "to the note");

    sidebar.show(Some("bo-ws".into()));
    assert_eq!(lit(&sidebar).2, ["bo-ws"]);
    sidebar.show(Some("gone".into()));
    let (_, fills, bold) = lit(&sidebar);
    assert!(fills.is_empty() && bold.is_empty());
}

#[test]
fn collapsed_it_shows_nothing_and_the_panes_take_the_window() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    sidebar.set_open(false);
    assert!(!sidebar.is_open());
    assert_eq!(sidebar.rows(&T, WINDOW), []);
    assert_eq!(sidebar.panes(WINDOW), WINDOW);
    sidebar.set_open(true);
    assert_eq!(shown(&sidebar.rows(&T, WINDOW)).len(), 4);
}

#[test]
fn before_the_first_list_it_is_empty_and_with_none_it_says_so() {
    assert_eq!(Sidebar::new(true).rows(&T, WINDOW), []);
    assert_eq!(
        shown(&heard(vec![]).rows(&T, WINDOW)),
        [("WORKSPACES", T.muted), ("no open workspaces", T.muted)]
    );
}

#[test]
fn each_canvas_has_its_workspaces_agent_for_its_button_and_footer() {
    assert_eq!(Sidebar::new(false).agents(), None);
    let none = Workspace { name: "plain".into(), canvas: "plain".into(), ..Workspace::default() };
    let mut sidebar = heard(vec![ws("ada", Some(Status::Blocked)), ws("bo", None), none]);
    let agents = sidebar.agents().unwrap();
    let canvases: Vec<&str> = agents.iter().map(|(c, _)| c.as_str()).collect();
    assert_eq!(canvases, ["ada-canvas", "bo-canvas", "plain"]);
    let named = |i: usize| agents[i].1.as_ref().map(|a| (a.name.as_str(), a.status));
    assert_eq!(named(0), Some(("ada", Some(Status::Blocked))));
    assert_eq!((named(1), named(2)), (Some(("bo", None)), None));
    sidebar.update(Err(crate::backend::Problem { message: "gone".into(), hint: None }));
    assert_eq!(sidebar.agents(), None, "a lost daemon: nothing known");
}

#[test]
fn the_daemon_s_count_of_pending_messages_wins_over_what_the_agent_said() {
    use crate::scene::sidebar::Process::{Absent, Exited, Running};
    use Status::{Blocked, Done, Stopped, Working};
    assert_eq!(Status::of(Some(Done), Exited, 0, true), Some(Stopped), "it ended");
    assert_eq!(Status::of(None, Exited, 1, false), Some(Stopped));
    assert_eq!(Status::of(None, Absent, 0, false), Some(Stopped), "none, and no hooks");
    assert_eq!(Status::of(Some(Working), Absent, 0, true), Some(Working), "run outside slingerd");
    assert_eq!(Status::of(Some(Blocked), Absent, 0, false), Some(Blocked));
    assert_eq!(Status::of(None, Running, 0, false), None, "started, nothing asked");
    assert_eq!(Status::of(Some(Done), Running, 1, true), Some(Working), "a hook is late");
    assert_eq!(Status::of(None, Running, 2, false), Some(Working), "no hooks at all");
    assert_eq!(Status::of(Some(Blocked), Running, 1, false), Some(Blocked));
    assert_eq!(Status::of(Some(Working), Running, 0, true), Some(Done), "it answered");
    assert_eq!(Status::of(None, Running, 0, true), Some(Done), "answered with no hooks");
    assert_eq!(Status::of(Some(Blocked), Running, 0, false), Some(Done));
    let named = [(Some("running"), true), (Some("exited"), true), (None, true), (None, false)];
    assert_eq!(named.map(|(n, s)| Process::named(n, s)), [Running, Exited, Exited, Absent]);
    let stopped = heard(vec![ws("ada", Some(Stopped))]);
    let rows = stopped.rows(&T, WINDOW);
    assert!(
        shown(&rows).iter().any(|(text, _)| *text == "ada · not running"),
        "{:?}",
        shown(&rows)
    );
}

#[test]
fn a_done_agent_leaves_the_notice_on_its_own_and_a_blocked_one_stays() {
    let start = std::time::Instant::now();
    let mut sidebar = heard(vec![ws("ada", None), ws("bo", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done)), ws("bo", Some(Status::Blocked))]));
    assert!(!sidebar.fade(start), "stamped when it first shows");
    assert_eq!(notice(&sidebar).as_deref(), Some("ada is done · bo is blocked"));
    assert!(!sidebar.fade(start + notice::FADE / 2));
    assert!(sidebar.fade(start + notice::FADE));
    assert_eq!(notice(&sidebar).as_deref(), Some("bo is blocked"));
    assert!(!sidebar.fade(start + 100 * notice::FADE), "blocked waits for the user");
    assert!(!sidebar.pressed());
    assert_eq!(notice(&sidebar).as_deref(), Some("bo is blocked"));
}

#[test]
fn the_pen_s_next_press_takes_a_done_agent_out_of_the_notice() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert!(sidebar.pressed());
    assert_eq!(notice(&sidebar), None);
    assert!(!sidebar.pressed(), "nothing more to take");
}

#[test]
fn the_notice_s_x_dismisses_it_and_the_rest_shows_the_agent_s_workspace() {
    let mut sidebar = heard(vec![ws("ada", None), ws("bo", None)]);
    assert_eq!(sidebar.notice_targets(WINDOW), [], "no notice, no targets");
    sidebar.update(Ok(vec![ws("ada", None), noted("bo", Status::Blocked, "which key?")]));
    let labels = sidebar.notice(&T, WINDOW);
    let targets = sidebar.notice_targets(WINDOW);
    let close = (labels[1].frame, Tap::Run(crate::commands::Command::DismissNotice));
    assert_eq!(targets, [close, (labels[0].frame, Tap::Workspace(1))]);
}

#[test]
fn a_canvas_names_its_workspaces_agent_for_the_sling() {
    assert_eq!(Sidebar::new(false).agent("ada-canvas"), None, "no list yet");
    let none = Workspace { name: "plain".into(), canvas: "plain".into(), ..Workspace::default() };
    let mut sidebar = heard(vec![ws("ada", None), none]);
    assert_eq!(sidebar.agent("ada-canvas"), Some("ada"));
    assert_eq!(sidebar.agent("plain"), None, "a workspace with no agent");
    assert_eq!(sidebar.agent("elsewhere"), None, "a canvas no open workspace has");
    sidebar.update(Err(crate::backend::Problem { message: "gone".into(), hint: None }));
    assert_eq!(sidebar.agent("ada-canvas"), None, "a lost daemon: no agent");
}

#[test]
fn lines_that_do_not_fit_are_left_out() {
    let many: Vec<_> = (0..40).map(|i| ws(&format!("a{i}"), None)).collect();
    let rows = heard(many).rows(&T, WINDOW);
    assert!(rows.len() < 120);
    let foot = WINDOW.height - crate::scene::status::HEIGHT;
    assert!(rows.iter().all(|r| r.label.frame.y + r.label.frame.height <= foot), "above its foot");
}

#[test]
fn each_workspace_s_lines_are_a_target_a_tap_shows_its_panes() {
    let mut blocked = noted("bo", Status::Blocked, "which key?");
    blocked.name = "bo-ws".into();
    let mut sidebar = heard(vec![ws("ada", None), blocked]);
    let targets = sidebar.targets(&T, WINDOW);
    assert_eq!(
        targets.iter().map(|t| t.1).collect::<Vec<_>>(),
        [Tap::Workspace(0), Tap::Workspace(1)]
    );
    let rows = sidebar.rows(&T, WINDOW);
    let at = |text: &str| rows.iter().find(|r| r.label.text == text).unwrap().label.frame;
    let (ada, bo) = (targets[0].0, targets[1].0);
    assert_eq!((ada.y, ada.height), (at("ada-ws").y, 2.0 * ROW), "the name and the agent");
    assert_eq!(bo.y + bo.height, at("which key?").y + ROW, "a blocked agent's note too");
    assert!(ada.height >= style::TAP && ada.y + ada.height <= bo.y, "pen-sized, apart");
    assert_eq!((ada.x, ada.width), (0.0, WIDTH - style::HAIRLINE), "the sidebar's width");
    assert_eq!(sidebar.pick(1).as_deref(), Some("bo-ws"));
    let lit = sidebar.rows(&T, WINDOW)[0].label.frame;
    assert_eq!(lit.y, bo.y, "the picked workspace stands out");
    assert_eq!(sidebar.pick(2), None, "past the last");
    sidebar.set_open(false);
    assert!(sidebar.targets(&T, WINDOW).is_empty() && sidebar.foot(WINDOW).is_none());
    sidebar.set_open(true);
    let foot = sidebar.foot(WINDOW).unwrap();
    assert_eq!(foot, Rect { width: WIDTH - style::HAIRLINE, ..WINDOW }, "its edge stays");
    assert!(Sidebar::new(true).targets(&T, WINDOW).is_empty(), "nothing heard yet");
}

#[test]
fn a_change_to_blocked_gives_a_notice_with_the_note() {
    let mut sidebar = heard(vec![ws("ada", Some(Status::Working))]);
    assert!(sidebar.update(Ok(vec![noted("ada", Status::Blocked, "which key?")])));
    let labels = sidebar.notice(&T, WINDOW);
    let (label, close) = (&labels[0], &labels[1]);
    assert_eq!(label.text, "ada is blocked: which key?");
    assert_eq!((label.colour, label.fill), (T.background, Some(T.focus)));
    assert_eq!(close.text, notice::CLOSE, "its ✕ at its end");
    assert_eq!(close.frame.x, label.frame.x + label.frame.width);
    let bottom = close.frame.y + close.frame.height;
    assert!(bottom <= WINDOW.height - status::HEIGHT, "above the footers");
    assert!(close.frame.x + close.frame.width - label.frame.x <= notice::WIDTH);
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
    assert_eq!(
        shown(&sidebar.rows(&T, WINDOW)),
        [("WORKSPACES", T.muted), ("✗ no daemon", T.error)]
    );
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    assert_eq!(notice(&sidebar), None, "the first list after it");
    assert_eq!(shown(&sidebar.rows(&T, WINDOW)).len(), 4);
}

#[test]
fn the_notice_sits_in_the_panes_area() {
    let mut sidebar = heard(vec![ws("ada", None)]);
    sidebar.update(Ok(vec![ws("ada", Some(Status::Done))]));
    let panes = sidebar.panes(WINDOW);
    let labels = sidebar.notice(&T, panes);
    let (text, close) = (labels[0].frame, labels[1].frame);
    assert!(text.x >= panes.x && close.x + close.width <= panes.x + panes.width);
    let narrow = Rect { width: 10.0, ..WINDOW };
    assert!(sidebar.notice(&T, narrow).iter().all(|l| l.frame.width.abs() < f64::EPSILON));
}

#[test]
fn an_agent_under_a_profile_says_which() {
    let mut dev = ws("ada", Some(Status::Working));
    dev.agent.as_mut().unwrap().profile = Some("dev".into());
    let rows = heard(vec![dev, ws("bo", None)]).rows(&T, WINDOW);
    let texts: Vec<&str> = shown(&rows).into_iter().map(|(text, _)| text).collect();
    assert!(texts.contains(&"ada (dev) · working"), "{texts:?}");
    assert!(texts.contains(&"bo"), "no profile, no mention: {texts:?}");
}

/// A workspace whose file is broken, named `name`, in `/w/<dir>`.
fn broken(name: &str, dir: &str) -> Workspace {
    let broken = Broken {
        message: "agent.tools: \"Shell\" is not a tool".into(),
        hint: Some("fix that file, or close it: 'slinger workspace close /w/bad'".into()),
        field: Some("agent.tools".into()),
    };
    let dir = PathBuf::from("/w").join(dir);
    Workspace { name: name.into(), dir, broken: Some(broken), ..Workspace::default() }
}

#[test]
fn a_broken_workspace_shows_why_the_field_and_the_hint_wrapped_to_the_sidebar() {
    let sidebar = heard(vec![ws("ada", None), broken("bad", "bad")]);
    let rows = sidebar.rows(&T, WINDOW);
    let texts = shown(&rows);
    let at = texts.iter().position(|(t, _)| *t == "bad").unwrap();
    let under: Vec<_> = texts[at + 1..].to_vec();
    assert_eq!(under[0], ("✗", T.error), "a mark, in the error colour");
    let said: Vec<_> = under[1..].iter().map(|(t, _)| *t).collect();
    let joined = said.join(" ");
    assert!(joined.starts_with("agent.tools: \"Shell\" is not a tool field: agent.tools fix"));
    assert!(joined.ends_with("'slinger workspace close /w/bad'"), "{joined}");
    assert!(said.len() > 4, "the hint wraps: {said:?}");
    assert!(!said.contains(&"no agent") && !said.contains(&broken::CLOSE), "not shown: no close");
    assert!(under[1..].iter().all(|&(_, c)| c == T.error || c == T.muted));
    let room = WIDTH - PAD;
    let row = |text: &str| rows.iter().find(|r| r.label.text == text).unwrap().label.frame;
    for text in &said {
        let fits = glyphs::frame(Point { x: 0.0, y: 0.0 }, text, 13.0);
        assert!(row(text).x + fits.width <= room, "{text:?} fits the sidebar");
    }
}

#[test]
fn a_shown_broken_workspace_has_a_close_line_whose_tap_closes_it() {
    let mut sidebar = heard(vec![broken("", "nameless"), ws("ada", None)]);
    sidebar.pick(0);
    let rows = sidebar.rows(&T, WINDOW);
    assert!(rows.iter().any(|r| r.label.text == "nameless"), "a file with no name: its folder's");
    let close = rows.iter().find(|r| r.label.text == broken::CLOSE).unwrap().label.frame;
    let targets = sidebar.targets(&T, WINDOW);
    let taps: Vec<_> = targets.iter().map(|t| t.1).collect();
    let run = Tap::Run(Command::CloseWorkspace);
    assert_eq!(taps, [Tap::Workspace(0), run, Tap::Workspace(0), Tap::Workspace(1)]);
    assert_eq!((targets[1].0.y, targets[1].0.height), (close.y, ROW), "the close line alone");
    assert_eq!(targets[0].0.y + targets[0].0.height, close.y, "above it shows it");
    assert_eq!(targets[2].0.y, close.y + ROW, "and the hint under it");
    assert_eq!(sidebar.close(), Some(PathBuf::from("/w/nameless")), "the shown one, by its dir");
    let names: Vec<_> = sidebar.agents().unwrap().into_iter().map(|(c, _)| c).collect();
    assert_eq!(names, ["ada-canvas"], "out of the list");
    assert!(shown(&sidebar.rows(&T, WINDOW)).contains(&("ada-ws", T.text)));
}

#[test]
fn close_takes_the_workspace_the_window_shows_and_the_first_shows_next() {
    let mut sidebar = heard(vec![ws("ada", None), ws("bo", None), ws("cy", None)]);
    sidebar.pick(1);
    sidebar.close().unwrap();
    let left: Vec<_> = sidebar.agents().unwrap().into_iter().map(|(c, _)| c).collect();
    assert_eq!(left, ["ada-canvas", "cy-canvas"]);
    let bold: Vec<_> = sidebar.rows(&T, WINDOW).into_iter().filter(|r| r.font.bold).collect();
    assert_eq!(bold[0].label.text, "ada-ws", "the first shows");
    sidebar.show(Some("gone".into()));
    assert_eq!(sidebar.close(), None, "the shown one is not in the list");
    assert_eq!(Sidebar::new(true).close(), None, "nothing heard yet");
}

#[test]
fn with_none_named_the_first_usable_workspace_stands_out_as_the_panes_show_it() {
    let mut sidebar = heard(vec![broken("", "nameless"), ws("ada", None), ws("bo", None)]);
    let bold = |s: &Sidebar| -> Vec<String> {
        let rows = s.rows(&T, WINDOW).into_iter().filter(|r| r.font.bold);
        rows.map(|r| r.label.text).collect()
    };
    assert_eq!(bold(&sidebar), ["ada-ws"], "not the broken first");
    assert!(!sidebar.rows(&T, WINDOW).iter().any(|r| r.label.text == broken::CLOSE));
    assert_eq!(sidebar.close(), Some(PathBuf::from("")), "closes the one shown: ada's");
    assert_eq!(bold(&sidebar), ["bo-ws"]);
}
