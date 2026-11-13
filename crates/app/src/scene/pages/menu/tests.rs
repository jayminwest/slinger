use super::*;
use crate::config::Rgb;
use crate::layout::Tree;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 255, g: 255, b: 255 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};
const SIZE: Size = Size { width: 800.0, height: 600.0 };
/// A footer's page name, at the window's bottom.
const ANCHOR: Rect = Rect { x: 350.0, y: 572.0, width: 100.0, height: 28.0 };

fn pane() -> PaneId {
    Tree::default().panes()[0]
}

fn open(names: &[&str], shown: usize) -> PageMenu {
    let mut menu = PageMenu::default();
    menu.open(pane(), ANCHOR, (names.iter().map(|n| (*n).to_string()).collect(), shown));
    menu
}

/// Each row's text (the ✓ on its own), top to bottom.
fn rows(menu: &PageMenu) -> Vec<String> {
    let overlay = menu.shown(&T, SIZE).unwrap();
    overlay
        .lines
        .iter()
        .filter(|l| !l.label.text.is_empty())
        .map(|l| l.label.text.clone())
        .collect()
}

/// The middle of the row that says `text`.
fn row(menu: &PageMenu, text: &str) -> Point {
    let overlay = menu.shown(&T, SIZE).unwrap();
    let line = overlay.lines.iter().find(|l| l.label.text == text).unwrap();
    let frame = line.label.frame;
    Point { x: frame.x + frame.width / 2.0, y: frame.y + frame.height / 2.0 }
}

#[test]
fn it_lists_the_pages_with_the_shown_one_marked_then_the_page_commands() {
    let menu = open(&["ideas", "spec", "plan"], 1);
    assert_eq!(
        rows(&menu),
        ["ideas", "✓", "spec", "plan", "New page", "Rename page…", "Remove page"]
    );
    let overlay = menu.shown(&T, SIZE).unwrap();
    assert_eq!(overlay.opacity, 0.0, "the window is not dimmed");
    let panel = overlay.lines[0].label.frame;
    assert!(panel.y + panel.height <= ANCHOR.y, "above the page name");
    assert!((panel.x + panel.width / 2.0 - (ANCHOR.x + ANCHOR.width / 2.0)).abs() < 1e-9);
    let spec = overlay.lines.iter().find(|l| l.label.text == "spec").unwrap();
    assert!(spec.bold, "the shown page stands out");
    let rows: Vec<Rect> = overlay
        .lines
        .iter()
        .filter(|l| !l.label.text.is_empty() && l.label.text != "✓")
        .map(|l| l.label.frame)
        .collect();
    assert!(rows.iter().all(|r| r.height >= style::TAP), "pen-sized rows");
    assert!(rows.windows(2).all(|w| w[0].y + w[0].height <= w[1].y + 1e-9), "none overlap");
}

#[test]
fn a_tap_on_a_row_runs_it_on_the_lift_and_closes_the_menu() {
    let mut menu = open(&["ideas", "spec", "plan"], 0);
    let plan = row(&menu, "plan");
    assert_eq!(menu.press(plan, SIZE), Took::Yes);
    assert!(menu.pointer(Point { x: plan.x + 3.0, ..plan }, SIZE, true), "the drag is the menu's");
    let Took::Tap((_, tap)) = menu.lift(plan, SIZE) else { panic!() };
    assert_eq!(tap, Tap::Page(2));
    assert!(!menu.is_open());
    let mut menu = open(&["ideas"], 0);
    let new = row(&menu, "New page");
    menu.press(new, SIZE);
    let Took::Tap((_, tap)) = menu.lift(new, SIZE) else { panic!() };
    assert_eq!(tap, Tap::Run(Command::NewPage));
}

#[test]
fn a_drag_off_the_row_runs_nothing_and_a_press_outside_or_escape_closes() {
    let mut menu = open(&["ideas", "spec"], 0);
    let (ideas, spec) = (row(&menu, "ideas"), row(&menu, "spec"));
    menu.press(ideas, SIZE);
    assert_eq!(menu.lift(spec, SIZE), Took::Yes, "another row: nothing runs");
    assert!(menu.is_open());
    assert_eq!(menu.press(Point { x: 10.0, y: 10.0 }, SIZE), Took::Yes, "used up");
    assert!(!menu.is_open());
    assert_eq!(menu.press(ideas, SIZE), Took::No, "closed, the press goes on");
    assert_eq!(menu.lift(ideas, SIZE), Took::No);
    let mut menu = open(&["ideas"], 0);
    assert!(!menu.key(Chord::new(0, 'x')), "another key goes on");
    assert!(menu.key(ESCAPE) && !menu.is_open());
    assert!(!menu.key(ESCAPE), "closed: not its key");
}

#[test]
fn the_row_under_the_pen_lights_and_more_while_pressed() {
    let mut menu = open(&["ideas", "spec"], 0);
    let spec = row(&menu, "spec");
    let fills = |menu: &PageMenu| {
        let overlay = menu.shown(&T, SIZE).unwrap();
        overlay.lines.iter().skip(1).filter_map(|l| l.label.fill).collect::<Vec<_>>()
    };
    let rule = vec![style::edge(&T)];
    assert_eq!(fills(&menu), rule, "only the hairline under the pages");
    assert!(menu.pointer(spec, SIZE, false));
    assert!(!menu.pointer(Point { x: spec.x + 1.0, ..spec }, SIZE, false), "same row");
    assert_eq!(fills(&menu)[0], style::hover(&T));
    menu.press(spec, SIZE);
    assert_eq!(fills(&menu)[0], style::selected(&T));
    assert!(menu.pointer(Point { x: 5.0, y: 5.0 }, SIZE, true));
    assert_eq!(fills(&menu), rule, "dragged off: nothing lit");
}

#[test]
fn many_pages_keep_the_shown_one_in_view_and_the_menu_follows_the_pages() {
    let names: Vec<String> = (1..=40).map(|n| format!("p{n}")).collect();
    let mut menu = PageMenu::default();
    menu.open(pane(), ANCHOR, (names.clone(), 30));
    let shown = rows(&menu);
    assert!(shown.contains(&"p31".to_string()) && !shown.contains(&"p1".to_string()), "{shown:?}");
    let panel = menu.shown(&T, SIZE).unwrap().lines[0].label.frame;
    assert!(panel.y >= EDGE, "inside the window");
    menu.refresh(Some((vec!["only".into()], Some(0))));
    assert_eq!(rows(&menu)[..2], ["✓", "only"]);
    assert_eq!(menu.pane(), Some((pane(), List::Pages)));
    menu.refresh(None);
    assert!(!menu.is_open() && menu.shown(&T, SIZE).is_none(), "its pane shows no pages: closed");
    assert!(!menu.pointer(Point { x: 400.0, y: 580.0 }, SIZE, false));
}

/// The pages the open menu shows, top to bottom.
fn pages(menu: &PageMenu) -> Vec<String> {
    rows(menu).into_iter().filter(|r| r.starts_with('p') && r != "✓").collect()
}

#[test]
fn a_scroll_turns_the_pages_a_row_at_a_time_and_stops_at_either_end() {
    let names: Vec<String> = (1..=40).map(|n| format!("p{n}")).collect();
    let mut menu = PageMenu::default();
    menu.open(pane(), ANCHOR, (names, 0));
    let first = pages(&menu);
    assert_eq!(first[0], "p1");
    assert!(first.len() < 40, "not all fit: {first:?}");
    assert!(menu.scroll(-ROW / 2.0, SIZE), "taken");
    assert_eq!(pages(&menu), first, "half a row waits for the rest");
    menu.scroll(-ROW / 2.0, SIZE);
    assert_eq!(pages(&menu)[0], "p2", "down the page shows later pages");
    menu.scroll(-ROW * 100.0, SIZE);
    assert_eq!(pages(&menu).last().map(String::as_str), Some("p40"), "the end");
    let end = pages(&menu);
    menu.scroll(-ROW, SIZE);
    assert_eq!(pages(&menu), end, "stays at the end");
    menu.scroll(ROW, SIZE);
    assert_eq!(pages(&menu)[0], format!("p{}", 40 - end.len()), "one back at once");
    menu.scroll(ROW * 100.0, SIZE);
    assert_eq!(pages(&menu), first, "back at the start");
    assert!(rows(&menu).ends_with(&[
        "New page".into(),
        "Rename page…".into(),
        "Remove page".into()
    ]));
    let mut closed = PageMenu::default();
    assert!(!closed.scroll(-ROW, SIZE), "closed: not its scroll");
}

#[test]
fn a_drag_on_the_panel_scrolls_the_pages_and_taps_nothing() {
    let names: Vec<String> = (1..=40).map(|n| format!("p{n}")).collect();
    let mut menu = PageMenu::default();
    menu.open(pane(), ANCHOR, (names, 0));
    let p3 = row(&menu, "p3");
    menu.press(p3, SIZE);
    let up = Point { y: p3.y - 2.0 * ROW, ..p3 };
    assert!(menu.pointer(up, SIZE, true));
    assert_eq!(pages(&menu)[0], "p3", "the pen up two rows: two later pages");
    assert_eq!(menu.lift(up, SIZE), Took::Yes, "a scroll is not a tap");
    assert!(menu.is_open());
    let p5 = row(&menu, "p5");
    menu.press(p5, SIZE);
    let Took::Tap((_, tap)) = menu.lift(p5, SIZE) else { panic!() };
    assert_eq!(tap, Tap::Page(4), "a tap after the scroll picks the page under it");
}

#[test]
fn a_menu_choice_closes_it_and_runs() {
    let mut menu = PageMenu::default();
    assert_eq!(menu.chosen(Command::NextPage), Took::No, "closed, the menu runs it");
    let mut menu = open(&["a", "b"], 0);
    let Took::Tap((_, tap)) = menu.chosen(Command::NextPage) else { panic!("a run") };
    assert_eq!(tap, Tap::Run(Command::NextPage));
    assert!(!menu.is_open());
}

#[test]
fn a_sign_says_when_more_pages_are_above_or_below() {
    let signs = |menu: &PageMenu| {
        let rows = rows(menu);
        let pages = pages(menu);
        let at = |sign: &str| rows.iter().position(|r| r == sign);
        let page = |n: usize| rows.iter().position(|r| *r == pages[n]);
        (at("▲").map(|_| page(0)), at("▼").map(|_| page(pages.len() - 1)))
    };
    assert_eq!(signs(&open(&["a", "b", "c"], 0)), (None, None), "all fit: no sign");
    let names: Vec<String> = (1..=40).map(|n| format!("p{n}")).collect();
    let mut menu = PageMenu::default();
    menu.open(pane(), ANCHOR, (names, 0));
    let (up, down) = signs(&menu);
    assert!(up.is_none() && down.is_some(), "at the top: only more below");
    let overlay = menu.shown(&T, SIZE).unwrap();
    let sign = overlay.lines.iter().find(|l| l.label.text == "▼").unwrap();
    let last = pages(&menu).pop().unwrap();
    let name = overlay.lines.iter().find(|l| l.label.text == last).unwrap();
    assert_eq!(sign.label.colour, T.muted);
    assert!(name.label.frame.x + name.label.frame.width <= sign.label.frame.x, "beside the name");
    menu.scroll(-ROW * 3.0, SIZE);
    let (up, down) = signs(&menu);
    assert!(up.is_some() && down.is_some(), "in the middle: both");
    menu.scroll(-ROW * 100.0, SIZE);
    let (up, down) = signs(&menu);
    assert!(up.is_some() && down.is_none(), "at the end: only more above");
}

#[test]
fn the_profile_menu_lists_the_profiles_with_the_running_one_marked_and_no_commands() {
    let names = || ["no profile", "dev", "look"].map(String::from).to_vec();
    let mut menu = PageMenu::default();
    menu.open_profiles(pane(), ANCHOR, (names(), Some(1)));
    assert_eq!(rows(&menu), ["no profile", "✓", "dev", "look"]);
    assert_eq!(menu.pane(), Some((pane(), List::Profiles)));
    let lines = menu.shown(&T, SIZE).unwrap().lines;
    let fills = lines.iter().skip(1).filter(|l| l.label.fill.is_some()).count();
    assert_eq!(fills, 0, "no hairline: no commands under them");
    let look = row(&menu, "look");
    menu.press(look, SIZE);
    let Took::Tap((_, tap)) = menu.lift(look, SIZE) else { panic!() };
    assert_eq!(tap, Tap::Profile(2));
    assert!(!menu.is_open());

    menu.open_profiles(pane(), ANCHOR, (names(), None));
    assert_eq!(rows(&menu), ["no profile", "dev", "look"], "not running: none marked");
    menu.refresh(None);
    assert!(!menu.is_open(), "no profiles left");
}
