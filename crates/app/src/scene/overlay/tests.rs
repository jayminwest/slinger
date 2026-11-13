use super::*;
use crate::layout;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 250, g: 250, b: 250 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 250 },
    done: Rgb { r: 5, g: 5, b: 5 },
};
/// A window tall enough for every row.
const TALL: Size = Size { width: 1600.0, height: 1000.0 };

fn key(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

fn open() -> KeyOverlay {
    let mut overlay = KeyOverlay::default();
    overlay.toggle();
    overlay
}

/// The text of each line of `overlay` with `keys`.
fn texts_of(overlay: &KeyOverlay, keys: &Keymap, size: Size) -> Vec<String> {
    let shown = overlay.shown(keys, &T, size).unwrap();
    shown.lines.into_iter().map(|l| l.label.text).collect()
}

fn texts(keys: &Keymap, size: Size) -> Vec<String> {
    texts_of(&open(), keys, size)
}

/// The chip beside `command`'s row: the line after its title.
fn chip(keys: &Keymap, command: Command) -> String {
    let texts = texts(keys, TALL);
    let at = texts.iter().position(|t| t == command.title()).unwrap();
    texts[at + 1].clone()
}

/// The middle of `command`'s row, as laid in a window of `size`, scrolled
/// by `scroll`.
fn on(command: Command, size: Size, scroll: f64) -> Point {
    let place = place(size, scroll);
    let (_, row) = place.rows.iter().find(|(c, _)| *c == command).unwrap();
    Point { x: row.x + row.width / 2.0, y: row.y + row.height / 2.0 }
}

#[test]
fn every_command_shows_exactly_once_with_its_key_under_its_group() {
    let keys = Keymap::default();
    let texts = texts(&keys, TALL);
    let heading = |g: Group| g.title().to_uppercase();
    for (command, _) in Command::ALL {
        let count = texts.iter().filter(|t| *t == command.title()).count();
        assert_eq!(count, 1, "{command:?}");
        let key = keys.chord(command).map_or(NO_KEY.into(), Chord::glyphs);
        assert_eq!(chip(&keys, command), key, "{command:?}");
        let at = texts.iter().position(|t| *t == heading(command.group())).unwrap();
        let row = texts.iter().position(|t| t == command.title()).unwrap();
        let next = Group::ALL.iter().filter_map(|&g| texts.iter().position(|t| *t == heading(g)));
        assert!(at < row && next.filter(|&n| n > at).all(|n| n > row), "{command:?}");
    }
    for group in Group::ALL {
        assert_eq!(texts.iter().filter(|t| **t == heading(group)).count(), 1, "{group:?}");
    }
    assert_eq!(texts[1], "Keys");
    assert_eq!(texts[2], "from config.toml · esc or ⌘/ closes");
    assert_eq!(texts[3], CLOSE);
}

#[test]
fn a_rebind_changes_the_row_and_an_unbound_command_says_no_key() {
    let mut keys = Keymap::default();
    assert_eq!(chip(&keys, Command::ShowWorkspaceFolder), NO_KEY, "no key by default");
    keys.bind(Command::ToolPen, Some(key("cmd+shift+p")));
    assert_eq!(chip(&keys, Command::ToolPen), "⇧⌘P");
    keys.bind(Command::ToolPen, None);
    assert_eq!(chip(&keys, Command::ToolPen), NO_KEY);
    keys.bind(Command::ShowKeys, None);
    assert_eq!(texts(&keys, TALL)[2], "from config.toml · esc closes");
}

/// The left edge of each group's heading in a window of `size`.
fn lefts(size: Size) -> Vec<f64> {
    let place = place(size, 0.0);
    Group::ALL.map(|g| place.headings.iter().find(|(h, _)| *h == g).unwrap().1.x).to_vec()
}

/// How many columns the groups fill in a window of `size`.
fn columns_at(size: Size) -> usize {
    let mut lefts = lefts(size);
    lefts.dedup();
    lefts.len()
}

#[test]
fn the_groups_fill_three_columns_left_to_right() {
    let (_, runs) = split(&Group::ALL, 3);
    let names = |run: &Vec<Group>| run.iter().map(|g| g.title()).collect::<Vec<_>>().join(" ");
    let runs: Vec<String> = runs.iter().map(names).collect();
    assert_eq!(runs, ["Tools Pop-ups Sling", "File Canvas", "Pages Panes Window"]);
    for size in [layout::WINDOW, TALL] {
        let x = lefts(size);
        assert!(x[0] < x[3] && x[3] < x[5], "{size:?}");
        assert_eq!((x[0], x[3]), (x[2], x[4]), "tools to sling share a column, file and canvas");
    }
}

#[test]
fn a_narrow_window_takes_fewer_columns_each_wide_enough_for_its_rows() {
    let medium = Size { width: 700.0, height: 600.0 };
    let sizes = [(layout::SMALLEST, 1), (medium, 2), (layout::WINDOW, 3), (TALL, 3)];
    for (size, n) in sizes {
        assert_eq!(columns_at(size), n, "{size:?}");
        let place = place(size, 0.0);
        assert!(place.rows.iter().all(|(_, f)| f.width >= MIN_COLUMN), "{size:?}");
        let (tallest, _) = split(&Group::ALL, u32::try_from(n).unwrap());
        assert!((place.most - (tallest - place.body.height).max(0.0)).abs() < 1e-9);
    }
    let place = place(layout::SMALLEST, 0.0);
    let tops: Vec<f64> = place.headings.iter().map(|(_, f)| f.y).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "one column, top to bottom");
    assert!(place.headings.iter().map(|(g, _)| *g).eq(Group::ALL), "in registry order");
    let (one, _) = split(&Group::ALL, 1);
    let (two, _) = split(&Group::ALL, 2);
    let (three, _) = split(&Group::ALL, 3);
    assert!(three < two && two < one, "each column more shortens the tallest");
}

#[test]
fn the_panel_is_rounded_with_a_hairline_and_its_chips_are_rounded_slabs() {
    let keys = Keymap::default();
    let shown = open().shown(&keys, &T, TALL).unwrap();
    let panel = &shown.lines[0];
    assert_eq!(panel.label.fill, Some(style::panel(&T)));
    assert_eq!((panel.edge, panel.radius), (Some(style::edge(&T)), style::PANEL_RADIUS));
    let glyphs = keys.chord(Command::Quit).unwrap().glyphs();
    let chip = shown.lines.iter().find(|l| l.label.text == glyphs).unwrap();
    assert_eq!(chip.label.fill, Some(style::chip(&T)));
    assert_eq!((chip.label.colour, chip.radius), (T.text, style::CHIP_RADIUS));
    let title = shown.lines.iter().find(|l| l.label.text == Command::Quit.title()).unwrap();
    assert!(title.label.frame.height >= style::TAP, "a row is a pen's target");
    let middle = |f: Rect| f.y + f.height / 2.0;
    assert!((middle(chip.label.frame) - middle(title.label.frame)).abs() < 1e-9);
}

#[test]
fn the_panel_fits_the_window_at_every_size_over_the_dim() {
    for size in [layout::SMALLEST, layout::WINDOW, TALL] {
        let shown = open().shown(&Keymap::default(), &T, size).unwrap();
        assert_eq!((shown.frame, shown.dim), (size.rect(), T.background));
        let panel = shown.lines[0].label.frame;
        assert!(panel.x >= 0.0 && panel.y >= 0.0, "{size:?}");
        assert!(panel.x + panel.width <= size.width && panel.y + panel.height <= size.height);
        for line in &shown.lines {
            let f = line.label.frame;
            assert!(panel.contains(Point { x: f.x, y: f.y }), "{:?} at {size:?}", line.label.text);
            let corner = Point { x: f.x + f.width, y: f.y + f.height };
            assert!(panel.contains(corner), "{:?} at {size:?}", line.label.text);
        }
    }
}

#[test]
fn while_open_it_takes_every_key_and_escape_or_its_own_key_closes_it() {
    let mut keys = Keymap::default();
    let mut overlay = KeyOverlay::default();
    assert!(!overlay.key(key("cmd+/"), &keys), "closed, the keymap runs show-keys");
    assert_eq!(overlay.shown(&keys, &T, layout::WINDOW), None);
    overlay.toggle();
    for other in ["cmd+q", "a", "delete", "cmd+d"] {
        assert!(overlay.key(key(other), &keys) && overlay.is_open(), "{other}");
    }
    assert!(overlay.key(key("escape"), &keys) && !overlay.is_open());
    overlay.toggle();
    assert!(overlay.key(key("cmd+/"), &keys) && !overlay.is_open());

    keys.bind(Command::ShowKeys, Some(key("f1")));
    overlay.toggle();
    assert!(overlay.key(key("cmd+/"), &keys) && overlay.is_open(), "the old key is just a key");
    assert!(overlay.key(key("f1"), &keys) && !overlay.is_open());
}

#[test]
fn a_press_outside_the_panel_or_on_its_close_target_closes_it() {
    let size = layout::WINDOW;
    let mut overlay = KeyOverlay::default();
    let centre = Point { x: size.width / 2.0, y: size.height / 2.0 };
    assert_eq!(overlay.press(centre, size), Taken::No);
    assert_eq!(overlay.lift(centre, size), Taken::No);
    overlay.toggle();
    assert_eq!(overlay.press(Point { x: 1.0, y: 1.0 }, size), Taken::Yes);
    assert!(!overlay.is_open());

    overlay.toggle();
    let close = place(size, 0.0).close;
    assert!(close.width >= style::TAP && close.height >= style::TAP, "big enough for the pen");
    let on = Point { x: close.x + close.width / 2.0, y: close.y + close.height / 2.0 };
    assert_eq!(overlay.press(on, size), Taken::Yes);
    assert!(!overlay.is_open());
}

#[test]
fn a_tap_on_a_row_closes_it_and_runs_the_command() {
    let size = TALL;
    let mut overlay = open();
    let at = on(Command::SplitRight, size, 0.0);
    assert_eq!(overlay.press(at, size), Taken::Yes);
    assert!(overlay.is_open(), "the press only holds the row");
    let held = overlay.shown(&Keymap::default(), &T, size).unwrap();
    assert!(held.lines.iter().any(|l| l.label.fill == Some(style::selected(&T))), "lit");
    let near = Point { x: at.x + 2.0, y: at.y + 1.0 };
    assert!(overlay.drag(near, size), "a small slip is still a tap");
    assert_eq!(overlay.lift(near, size), Taken::Run(Command::SplitRight));
    assert!(!overlay.is_open());

    let mut overlay = open();
    overlay.press(at, size);
    assert_eq!(overlay.lift(on(Command::SplitDown, size, 0.0), size), Taken::Yes);
    assert!(overlay.is_open(), "lifted on another row: nothing runs");

    overlay.press(on(Command::ShowKeys, size, 0.0), size);
    assert_eq!(overlay.lift(on(Command::ShowKeys, size, 0.0), size), Taken::Yes);
    assert!(!overlay.is_open(), "its own row just closes it");
}

#[test]
fn the_row_under_the_pen_lights_up() {
    let size = TALL;
    let mut overlay = open();
    let lit = |overlay: &KeyOverlay| {
        let shown = overlay.shown(&Keymap::default(), &T, size).unwrap();
        shown.lines.iter().filter(|l| l.label.fill == Some(style::hover(&T))).count()
    };
    assert_eq!(lit(&overlay), 0);
    assert!(overlay.hover(on(Command::Undo, size, 0.0), size));
    assert_eq!(lit(&overlay), 1);
    let near = on(Command::Undo, size, 0.0);
    assert!(!overlay.hover(Point { x: near.x + 3.0, ..near }, size), "the same row");
    assert!(overlay.hover(Point { x: 1.0, y: 1.0 }, size));
    assert_eq!(lit(&overlay), 0);
    assert!(!KeyOverlay::default().hover(near, size), "closed, nothing lights");
}

#[test]
fn rows_that_do_not_fit_scroll_by_drag_scroll_or_arrow_and_a_drag_runs_nothing() {
    let size = layout::SMALLEST;
    let keys = Keymap::default();
    let last = place(size, 0.0).rows.last().unwrap().0;
    let mut overlay = open();
    assert!(!texts_of(&overlay, &keys, size).contains(&last.title().to_string()), "below");
    let most = place(size, f64::MAX).scroll;
    assert!(most > 0.0);

    let from = on(Command::ToolPen, size, 0.0);
    overlay.press(from, size);
    let up = Point { y: from.y - 2000.0, ..from };
    assert!(overlay.drag(up, size));
    assert_eq!(overlay.lift(up, size), Taken::Yes, "a drag scrolls and runs nothing");
    assert!(overlay.is_open());
    assert!(texts_of(&overlay, &keys, size).contains(&last.title().to_string()), "scrolled in");
    assert!((overlay.scroll - most).abs() < 1e-9, "no further than the last row");

    assert!(overlay.scroll(most * 2.0, size));
    assert!(overlay.scroll.abs() < 1e-9, "a scroll up goes back to the top");
    overlay.key(key("down"), &keys);
    assert!((overlay.scroll - ROW).abs() < 1e-9);
    assert!(!KeyOverlay::default().scroll(10.0, size));

    let shown = overlay.shown(&keys, &T, size).unwrap();
    let body = place(size, overlay.scroll).body;
    for line in shown.lines.iter().filter(|l| l.label.text == Command::ToolPen.title()) {
        assert!(line.label.frame.y >= body.y, "a row scrolled off does not show over the title");
    }
    assert!(shown.lines.iter().any(|l| l.radius == BAR / 2.0), "a bar says where the rows are");
}

#[test]
fn a_menu_choice_closes_it_and_runs_except_its_own_which_only_closes() {
    let mut overlay = KeyOverlay::default();
    assert_eq!(overlay.chosen(Command::Undo), Taken::No, "closed, the menu runs it");
    let mut overlay = open();
    assert_eq!(overlay.chosen(Command::Undo), Taken::Run(Command::Undo));
    assert!(!overlay.is_open());
    let mut overlay = open();
    assert_eq!(overlay.chosen(Command::ShowKeys), Taken::Yes);
    assert!(!overlay.is_open(), "show-keys from the menu does not open it again");
}
