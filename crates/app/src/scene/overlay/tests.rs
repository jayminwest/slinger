use super::*;
use crate::layout;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

fn key(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

fn open() -> KeyOverlay {
    let mut overlay = KeyOverlay::default();
    overlay.toggle();
    overlay
}

/// The text of each line of the open overlay with `keys`.
fn texts(keys: &Keymap, size: Size) -> Vec<String> {
    let shown = open().shown(keys, &T, size).unwrap();
    shown.lines.into_iter().map(|l| l.label.text).collect()
}

/// The chip beside `command`'s row: the line after its title.
fn chip(keys: &Keymap, command: Command) -> String {
    let texts = texts(keys, layout::WINDOW);
    let at = texts.iter().position(|t| t == command.title()).unwrap();
    texts[at + 1].clone()
}

#[test]
fn every_command_shows_exactly_once_with_its_key_under_its_group() {
    let keys = Keymap::default();
    let texts = texts(&keys, layout::WINDOW);
    for (command, _) in Command::ALL {
        let count = texts.iter().filter(|t| *t == command.title()).count();
        assert_eq!(count, 1, "{command:?}");
        assert_eq!(chip(&keys, command), keys.chord(command).unwrap().glyphs());
        let heading = texts.iter().position(|t| t == command.group().title()).unwrap();
        let row = texts.iter().position(|t| t == command.title()).unwrap();
        let next = Group::ALL.iter().filter_map(|g| texts.iter().position(|t| t == g.title()));
        assert!(heading < row && next.filter(|&n| n > heading).all(|n| n > row), "{command:?}");
    }
    for group in Group::ALL {
        assert_eq!(texts.iter().filter(|t| *t == group.title()).count(), 1, "{group:?}");
    }
    assert_eq!(texts[2], "Keys");
    assert_eq!(texts[3], "from config.toml · esc or ⌘/ closes");
}

#[test]
fn a_rebind_changes_the_row_and_an_unbound_command_says_no_key() {
    let mut keys = Keymap::default();
    keys.bind(Command::ToolPen, Some(key("cmd+shift+p")));
    assert_eq!(chip(&keys, Command::ToolPen), "⇧⌘P");
    keys.bind(Command::ToolPen, None);
    assert_eq!(chip(&keys, Command::ToolPen), NO_KEY);
    keys.bind(Command::ShowKeys, None);
    assert_eq!(texts(&keys, layout::WINDOW)[3], "from config.toml · esc closes");
}

#[test]
fn the_groups_fill_three_columns_left_to_right() {
    assert_eq!(Group::ALL.map(column), [0, 0, 0, 1, 1, 2, 2]);
    let shown = open().shown(&Keymap::default(), &T, layout::WINDOW).unwrap();
    let x = |title: &str| shown.lines.iter().find(|l| l.label.text == title).unwrap().label.frame.x;
    assert!(x("Tools") < x("Canvas") && x("Canvas") < x("Panes"));
    assert_eq!(x("Tools"), x("Sling"));
}

#[test]
fn the_panel_fits_the_window_at_every_size_over_the_dim() {
    for size in [layout::SMALLEST, layout::WINDOW, Size { width: 1600.0, height: 1000.0 }] {
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
fn while_open_it_takes_every_press_and_one_outside_the_panel_closes_it() {
    let size = layout::WINDOW;
    let mut overlay = KeyOverlay::default();
    let centre = Point { x: size.width / 2.0, y: size.height / 2.0 };
    assert!(!overlay.press(centre, size));
    overlay.toggle();
    assert!(overlay.press(centre, size) && overlay.is_open());
    assert!(overlay.press(Point { x: 1.0, y: 1.0 }, size) && !overlay.is_open());
}
