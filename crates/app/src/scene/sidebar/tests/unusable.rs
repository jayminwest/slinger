//! A workspace whose file the daemon cannot use: why, its field, its close
//! line and its hint, each wrapped to the sidebar and cut short past a few
//! lines.

use super::*;
use crate::layout::Point;
use crate::scene::glyphs;
use crate::scene::sidebar::broken::{self, CUT};

/// A broken workspace as the daemon says it when it cannot parse its file:
/// a long message, and a hint that names its long path.
fn long(dir: &str) -> Workspace {
    let path = format!("/private/tmp/some-long-scratch-directory/{dir}/and/deeper/still/{dir}");
    let broken = Broken {
        message: "line 1: extra `=`, expected nothing; and then a good deal more about it".into(),
        hint: Some(format!(
            "see docs/workspace.md for the workspace file format; fix the file, or close it: \
             'slinger workspace close {path}'"
        )),
        field: None,
    };
    Workspace { dir: PathBuf::from(path), broken: Some(broken), ..Workspace::default() }
}

/// The texts under the workspace named `name`, up to the next one's name.
fn under<'a>(rows: &'a [Row], name: &str, next: &str) -> Vec<&'a str> {
    let texts: Vec<_> = shown(rows).into_iter().map(|(t, _)| t).collect();
    let at = texts.iter().position(|t| *t == name).unwrap();
    let end = texts.iter().position(|t| *t == next).unwrap_or(texts.len());
    texts[at + 1..end].to_vec()
}

#[test]
fn a_broken_workspace_shows_why_and_the_field_wrapped_to_the_sidebar() {
    let sidebar = heard(vec![ws("ada", None), broken("bad", "bad")]);
    let rows = sidebar.rows(&T, WINDOW);
    let texts = shown(&rows);
    let at = texts.iter().position(|(t, _)| *t == "bad").unwrap();
    let under: Vec<_> = texts[at + 1..].to_vec();
    assert_eq!(under[0], ("✗", T.error), "a mark, in the error colour");
    let said: Vec<_> = under[1..].iter().map(|(t, _)| *t).collect();
    assert_eq!(said.join(" "), "agent.tools: \"Shell\" is not a tool field: agent.tools");
    assert!(!said.contains(&broken::CLOSE), "not shown: no close line, and no hint");
    assert!(under[1..].iter().all(|&(_, c)| c == T.error || c == T.muted));
    let room = WIDTH - PAD;
    let row = |text: &str| rows.iter().find(|r| r.label.text == text).unwrap().label.frame;
    for text in &said {
        let fits = glyphs::frame(Point { x: 0.0, y: 0.0 }, text, 13.0);
        assert!(row(text).x + fits.width <= room, "{text:?} fits the sidebar");
    }
}

#[test]
fn a_shown_broken_workspace_has_a_close_line_whose_tap_closes_it_then_its_hint() {
    let mut sidebar = heard(vec![broken("", "nameless"), ws("ada", None)]);
    sidebar.pick(0);
    let rows = sidebar.rows(&T, WINDOW);
    assert!(rows.iter().any(|r| r.label.text == "nameless"), "a file with no name: its folder's");
    let close = rows.iter().find(|r| r.label.text == broken::CLOSE).unwrap().label.frame;
    let hint = under(&rows, broken::CLOSE, "ada-ws").join(" ");
    let whole = "fix that file, or close it: 'slinger workspace close /w/bad'";
    assert!(hint.ends_with(CUT), "three lines at most: {hint}");
    assert!(whole.starts_with(hint.trim_end_matches(CUT)), "{hint}");
    let targets = rows_only(&sidebar);
    let taps: Vec<_> = targets.iter().map(|t| t.1).collect();
    let run = Tap::Run(Command::CloseWorkspace);
    assert_eq!(taps, [Tap::Workspace(0), run, Tap::Workspace(0), Tap::Workspace(1)]);
    assert_eq!((targets[1].0.y, targets[1].0.height), (close.y, ROW), "the close line alone");
    assert_eq!(targets[0].0.y + targets[0].0.height, close.y, "above it shows it");
    assert_eq!(targets[2].0.y, close.y + ROW, "and the hint under it");
    assert_eq!(
        close_shown(&mut sidebar),
        Some(PathBuf::from("/w/nameless")),
        "the shown one, by its dir"
    );
    let names: Vec<_> = sidebar.agents().unwrap().into_iter().map(|(c, _)| c).collect();
    assert_eq!(names, ["ada-canvas"], "out of the list");
    assert!(shown(&sidebar.rows(&T, WINDOW)).contains(&("ada-ws", T.text)));
}

#[test]
fn long_broken_files_never_push_the_rows_under_them_out_of_sight() {
    // As the hand check saw (slinger-a57147): two files that do not parse,
    // whose hints name long paths, above the workspace the window shows.
    let mut sidebar = heard(vec![long("one"), long("two"), ws("ada", None), ws("bo", None)]);
    let foot = WINDOW.height - crate::scene::status::HEIGHT;
    let names = |s: &Sidebar| {
        let rows = s.rows(&T, WINDOW);
        assert!(rows.iter().all(|r| r.label.frame.y + r.label.frame.height <= foot));
        shown(&rows).into_iter().map(|(t, _)| t.to_string()).collect::<Vec<_>>()
    };
    let all = names(&sidebar);
    assert!(all.contains(&"ada-ws".into()) && all.contains(&"bo-ws".into()), "{all:?}");
    let one = under(&sidebar.rows(&T, WINDOW), "one", "two").join(" ");
    assert!(one.ends_with(CUT) && one.contains("line 1: extra `=`"), "the message, cut: {one}");
    assert!(!one.contains("see docs"), "no hint while not shown: {one}");
    // Shown, the first gives its close line and its hint, cut short too.
    sidebar.pick(0);
    let rows = sidebar.rows(&T, WINDOW);
    let one = under(&rows, "one", "two");
    assert!(one.contains(&broken::CLOSE), "{one:?}");
    assert!(one.len() <= 1 + 2 + 1 + 3, "mark, message, close, hint: {one:?}");
    assert!(one.last().unwrap().ends_with(CUT), "{one:?}");
    let all = names(&sidebar);
    assert!(all.contains(&"bo-ws".into()), "the last row still shows: {all:?}");
    let room = WIDTH - PAD;
    for row in rows.iter().filter(|r| r.label.text.ends_with(CUT)) {
        let fits = glyphs::frame(Point { x: 0.0, y: 0.0 }, &row.label.text, 13.0);
        assert!(row.label.frame.x + fits.width <= room, "{:?} fits", row.label.text);
    }
}

/// A measure of 6 points a character, as the platform hands one in.
fn six(text: &str, size: f64) -> crate::layout::Size {
    let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    crate::layout::Size { width: 6.0 * f64::from(u32::try_from(widest).unwrap()), height: size }
}

#[test]
fn each_line_fits_its_row_less_the_field_s_inset() {
    // 26 characters measure 157 points: inside the 160-point row, but not
    // inside the text field that draws it (slinger-c19909).
    glyphs::set_measure(six);
    let mut bad = broken("bad", "bad");
    bad.broken.as_mut().unwrap().message = "abcdefghijklmnopqrstuvwxyz expected".into();
    let mut sidebar = heard(vec![bad]);
    sidebar.pick(0);
    let rows = sidebar.rows(&T, WINDOW);
    let texts = shown(&rows);
    assert!(texts.iter().any(|(t, _)| t.ends_with("expected")), "{texts:?}");
    let under = rows.iter().filter(|r| r.label.frame.x > PAD && r.label.text.chars().count() > 1);
    for row in under {
        let glyphs = glyphs::frame(Point { x: 0.0, y: 0.0 }, &row.label.text, 13.0).width;
        let room = row.label.frame.width - 2.0 * broken::INSET;
        assert!(glyphs <= room, "{:?}: {glyphs} in {room}", row.label.text);
    }
}
