//! Which workspace the window shows: the one named or tapped, else the one
//! it showed when the app last quit, else the first usable; and what closes.

use super::*;

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
fn a_shown_workspace_s_stopped_agent_has_a_start_line_whose_tap_starts_it() {
    let stopped = Some(Status::Stopped);
    let mut sidebar = heard(vec![ws("ada", stopped), ws("bo", stopped), ws("cy", None)]);
    let start = Tap::Run(Command::StartAgent);
    let taps = |s: &Sidebar| s.targets(&T, WINDOW).into_iter().map(|t| t.1).collect::<Vec<_>>();
    let three = [Tap::Workspace(0), start, Tap::Workspace(1), Tap::Workspace(2)];
    assert_eq!(taps(&sidebar), three, "the shown one's only");
    assert!(shown(&sidebar.rows(&T, WINDOW)).contains(&(START, T.focus)));
    assert_eq!(sidebar.shown_dir(), Some(PathBuf::from("/w/ada")), "what it starts");
    sidebar.pick(2);
    assert!(!taps(&sidebar).contains(&start), "an agent that runs: no line");
    assert_eq!(Sidebar::new(true).shown_dir(), None, "nothing heard yet");
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
    assert_eq!(sidebar.close(), Some(PathBuf::from("/w/ada")), "closes the one shown: ada's");
    assert_eq!(bold(&sidebar), ["bo-ws"]);
    sidebar.update(Ok(vec![broken("", "one"), broken("", "two")]));
    // Of two that name none: by its folder, and no name for the panes to ask for.
    let two = (sidebar.pick(1), bold(&sidebar), sidebar.shown_dir());
    assert_eq!(two, (None, vec!["two".into()], Some("/w/two".into())));
}

#[test]
fn with_none_named_the_last_shown_workspace_shows_while_it_is_open_and_usable() {
    let list = vec![ws("ada", None), ws("bo", None), broken("cy-ws", "cy")];
    let mut sidebar = Sidebar::new(true).preferring(Some("bo-ws".into()));
    assert_eq!(sidebar.shown_name(), None, "nothing heard yet");
    sidebar.update(Ok(list.clone()));
    assert_eq!(sidebar.shown_name(), Some("bo-ws"), "the last shown, not the first");
    assert_eq!(sidebar.shown_dir(), Some(PathBuf::from("/w/bo")));
    sidebar.pick(0);
    assert_eq!(sidebar.shown_name(), Some("ada-ws"), "a tap wins");
    sidebar.show(None);
    sidebar.update(Ok(vec![ws("ada", None), broken("cy-ws", "cy")]));
    assert_eq!(sidebar.shown_name(), Some("ada-ws"), "the last shown was closed");
    let mut broken_last = Sidebar::new(true).preferring(Some("cy-ws".into()));
    broken_last.update(Ok(list));
    assert_eq!(broken_last.shown_name(), Some("ada-ws"), "the last shown is broken");
}

#[test]
fn a_broken_file_that_names_none_has_no_name_to_show_again() {
    let mut sidebar = heard(vec![broken("", "one")]);
    sidebar.pick(0);
    assert_eq!((sidebar.shown_name(), sidebar.shown_dir()), (None, Some("/w/one".into())));
}
