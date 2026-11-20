//! A workspace row's ✕: one per workspace, at its name's end, and what a
//! close by folder leaves shown.

use super::*;
use crate::scene::taps::Taps;

#[test]
fn each_workspace_has_one_close_mark_at_its_name_s_end_before_its_row() {
    let sidebar = heard(vec![ws("ada", None), broken("", "nameless"), ws("bo", None)]);
    let targets = sidebar.targets(&T, WINDOW);
    let rows = sidebar.rows(&T, WINDOW);
    for (i, name) in ["ada-ws", "nameless", "bo-ws"].into_iter().enumerate() {
        let closes: Vec<_> = targets.iter().filter(|t| t.1 == Tap::Close(i)).collect();
        assert_eq!(closes.len(), 1, "one ✕ for {name}");
        let close = closes[0].0;
        let line = rows.iter().find(|r| r.label.text == name).unwrap().label.frame;
        assert_eq!((close.y, close.height), (line.y, ROW), "on {name}'s line");
        assert!(close.width >= style::TAP, "pen-sized");
        assert_eq!(close.x + close.width, WIDTH - style::HAIRLINE, "at the sidebar's edge");
        assert!(line.x + line.width <= close.x, "{name} ends before it");
        let at = |tap| targets.iter().position(|t| t.1 == tap).unwrap();
        assert!(at(Tap::Close(i)) < at(Tap::Workspace(i)), "found before its row");
        let mark = rows
            .iter()
            .find(|r| r.label.text == notice::CLOSE && (r.label.frame.y - line.y).abs() < 0.5);
        let mark = &mark.unwrap().label;
        assert_eq!(mark.colour, T.muted);
        assert!(close.contains(crate::layout::Point { x: mark.frame.x + 4.0, y: line.y + 1.0 }));
    }
    let mut taps = Taps::default();
    taps.set(targets.clone());
    let close = targets.iter().find(|t| t.1 == Tap::Close(0)).unwrap().0;
    let middle = crate::layout::Point { x: close.x + close.width / 2.0, y: close.y + 4.0 };
    taps.hover(middle);
    assert_eq!(taps.lit(&T).map(|l| l.0), Some(close), "the ✕ lights alone");
    let mut collapsed = sidebar.clone();
    collapsed.set_open(false);
    assert!(!collapsed.targets(&T, WINDOW).iter().any(|t| matches!(t.1, Tap::Close(_))));
    assert!(!Sidebar::new(true).targets(&T, WINDOW).iter().any(|t| matches!(t.1, Tap::Close(_))));
}

#[test]
fn a_close_names_its_workspace_by_folder() {
    let sidebar = heard(vec![ws("ada", None), broken("", "nameless")]);
    assert_eq!(sidebar.dir(0), Some(PathBuf::from("/w/ada")));
    assert_eq!(sidebar.dir(1), Some(PathBuf::from("/w/nameless")), "one that names none too");
    assert_eq!(sidebar.dir(2), None, "past the last");
    assert_eq!(Sidebar::new(true).dir(0), None, "nothing heard yet");
}

#[test]
fn closing_a_row_not_shown_keeps_the_shown_one() {
    let mut sidebar = heard(vec![ws("ada", None), ws("bo", None), ws("cy", None)]);
    sidebar.pick(2);
    let (gone, shown) = sidebar.closed(Path::new("/w/ada")).unwrap();
    assert_eq!((gone.name.as_str(), shown), ("ada-ws", false));
    assert_eq!(sidebar.shown_dir(), Some(PathBuf::from("/w/cy")), "cy still shows");
    assert_eq!(sidebar.closed(Path::new("/w/ada")), None, "not listed any more");
    let left: Vec<_> = sidebar.agents().unwrap().into_iter().map(|(c, _)| c).collect();
    assert_eq!(left, ["bo-canvas", "cy-canvas"]);
}

#[test]
fn closing_the_shown_one_shows_the_first_usable() {
    let mut sidebar = heard(vec![broken("", "nameless"), ws("ada", None), ws("bo", None)]);
    sidebar.pick(2);
    let (gone, shown) = sidebar.closed(Path::new("/w/bo")).unwrap();
    assert_eq!((gone.title(), shown), ("bo-ws".to_string(), true));
    assert_eq!(sidebar.shown_dir(), Some(PathBuf::from("/w/ada")), "not the broken first");
    let (nameless, _) = sidebar.closed(Path::new("/w/nameless")).unwrap();
    assert_eq!(nameless.title(), "nameless", "a file that names none: its folder's");
}
