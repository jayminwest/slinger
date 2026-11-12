use super::*;
use crate::config::Rgb;
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

fn open() -> Palette {
    let mut palette = Palette::default();
    palette.toggle();
    palette
}

/// Types each character of `text` into `palette`.
fn type_in(palette: &mut Palette, text: &str) {
    for c in text.chars() {
        let chord = if c == ' ' { key("space") } else { Chord::new(0, c) };
        assert_eq!(palette.key(chord, &Keymap::default()), Taken::Yes);
    }
}

/// The text of each line of the open `palette`.
fn texts(palette: &Palette, size: Size) -> Vec<String> {
    let shown = palette.shown(&Keymap::default(), &T, size).unwrap();
    shown.lines.into_iter().map(|l| l.label.text).collect()
}

#[test]
fn opened_empty_it_lists_every_command_by_group_tools_first_and_quit_last() {
    let matches = open().matches();
    assert_eq!(matches.len(), Command::ALL.len());
    assert_eq!(matches[..2], [Command::ToolPen, Command::ToolEraser]);
    assert_eq!(matches.last(), Some(&Command::Quit));
    let groups: Vec<usize> =
        matches.iter().map(|c| Group::ALL.iter().position(|&g| g == c.group()).unwrap()).collect();
    assert!(groups.is_sorted(), "by group, in the surfaces' order");
}

#[test]
fn return_on_a_just_opened_palette_never_quits() {
    let keys = Keymap::default();
    let taken = open().key(key("return"), &keys);
    assert_eq!(taken, Taken::Run(Command::ToolPen));
}

#[test]
fn opened_again_it_lists_the_command_it_last_ran_first_but_never_quit() {
    let keys = Keymap::default();
    let mut palette = open();
    type_in(&mut palette, "zoom out");
    assert_eq!(palette.key(key("return"), &keys), Taken::Run(Command::ZoomOut));
    palette.toggle();
    let matches = palette.matches();
    assert_eq!(matches[..2], [Command::ZoomOut, Command::ToolPen]);
    assert_eq!(matches.len(), Command::ALL.len(), "listed once");
    type_in(&mut palette, "split");
    assert_eq!(palette.matches()[0], Command::SplitRight, "typed, the ranking leads");

    let mut palette = open();
    palette.last = Some(Command::ZoomOut);
    type_in(&mut palette, "quit");
    assert_eq!(palette.key(key("return"), &keys), Taken::Run(Command::Quit));
    palette.toggle();
    assert_eq!(palette.matches()[0], Command::ZoomOut, "quit is not remembered");
}

#[test]
fn a_title_prefix_ranks_over_a_word_start_over_a_subsequence_over_the_group() {
    let mut palette = open();
    type_in(&mut palette, "sp");
    let matches = palette.matches();
    let at = |c| matches.iter().position(|&m| m == c).unwrap();
    assert_eq!(matches[..2], [Command::SplitRight, Command::SplitDown]);
    assert!(at(Command::SplitDown) < at(Command::PickShape), "prefix before subsequence");

    let mut palette = open();
    type_in(&mut palette, "pane");
    let matches = palette.matches();
    let at = |c| matches.iter().position(|&m| m == c).unwrap();
    assert!(at(Command::ShrinkPane) < at(Command::PanLeft), "word start before subsequence");
    assert!(at(Command::PanLeft) < at(Command::SplitRight), "the title before the group");
}

#[test]
fn the_search_is_a_subsequence_of_title_and_group_in_any_case() {
    let mut palette = open();
    type_in(&mut palette, "split");
    assert_eq!(palette.matches(), [Command::SplitRight, Command::SplitDown]);
    let mut palette = open();
    type_in(&mut palette, "ERASER TOOLS");
    assert_eq!(palette.matches(), [Command::ToolEraser]);
    let mut palette = open();
    type_in(&mut palette, "zz");
    assert_eq!(palette.matches(), []);
    assert!(texts(&palette, layout::WINDOW).contains(&NONE.to_string()));
}

#[test]
fn delete_takes_back_a_character_and_held_command_keys_type_nothing() {
    let keys = Keymap::default();
    let mut palette = open();
    type_in(&mut palette, "zx");
    assert_eq!(palette.key(key("delete"), &keys), Taken::Yes);
    assert_eq!(palette.query, "z");
    for other in ["cmd+d", "ctrl+a", "up", "f5", "tab"] {
        assert_eq!(palette.key(key(other), &keys), Taken::Yes, "{other}");
    }
    assert_eq!(palette.query, "z");
    assert!(texts(&palette, layout::WINDOW).contains(&"z▏".to_string()));
}

#[test]
fn a_paste_goes_into_the_search_lowercase_on_one_line_and_closed_it_does_nothing() {
    let mut closed = Palette::default();
    assert!(!closed.pasted("split"));
    assert_eq!(closed.query, "");
    let mut palette = open();
    type_in(&mut palette, "s");
    assert!(palette.pasted("PLIT\r\nright"));
    assert_eq!(palette.query, "split right");
    assert_eq!(palette.matches(), [Command::SplitRight]);
}

#[test]
fn up_and_down_move_the_selection_round_the_matches() {
    let keys = Keymap::default();
    let mut palette = open();
    type_in(&mut palette, "split");
    assert_eq!(palette.at, 0);
    palette.key(key("up"), &keys);
    assert_eq!(palette.at, 1, "up from the first goes to the last");
    palette.key(key("down"), &keys);
    assert_eq!(palette.at, 0, "down from the last goes to the first");
    palette.key(key("down"), &keys);
    type_in(&mut palette, " ");
    assert_eq!(palette.at, 0, "typing selects the first again");
}

#[test]
fn return_closes_it_and_runs_the_selected_command() {
    let keys = Keymap::default();
    let mut palette = open();
    type_in(&mut palette, "split");
    palette.key(key("down"), &keys);
    assert_eq!(palette.key(key("return"), &keys), Taken::Run(Command::SplitDown));
    assert!(!palette.is_open());
    palette.toggle();
    assert_eq!((palette.query.as_str(), palette.at), ("", 0), "it opens empty again");

    type_in(&mut palette, "zz");
    assert_eq!(palette.key(key("return"), &keys), Taken::Yes, "with no match nothing runs");
    assert!(palette.is_open());
}

#[test]
fn escape_or_its_own_key_closes_it_and_runs_nothing() {
    let mut keys = Keymap::default();
    let mut palette = Palette::default();
    assert_eq!(palette.key(key("cmd+k"), &keys), Taken::No, "closed, the keymap runs it");
    assert_eq!(palette.shown(&keys, &T, layout::WINDOW), None);
    palette.toggle();
    assert_eq!(palette.key(key("escape"), &keys), Taken::Yes);
    assert!(!palette.is_open());
    palette.toggle();
    assert_eq!(palette.key(key("cmd+k"), &keys), Taken::Yes);
    assert!(!palette.is_open());

    keys.bind(Command::ShowCommands, Some(key("f2")));
    palette.toggle();
    assert_eq!(palette.key(key("f2"), &keys), Taken::Yes);
    assert!(!palette.is_open());
}

#[test]
fn a_menu_choice_closes_it_and_runs_except_its_own_which_only_closes() {
    let mut palette = Palette::default();
    assert_eq!(palette.chosen(Command::Undo), Taken::No, "closed, the menu runs it");
    palette.toggle();
    type_in(&mut palette, "split");
    assert_eq!(palette.chosen(Command::Undo), Taken::Run(Command::Undo));
    assert!(!palette.is_open());
    palette.toggle();
    assert_eq!(
        palette.matches()[0],
        open().matches()[0],
        "not the palette's run: not listed first"
    );
    assert_eq!(palette.chosen(Command::ShowCommands), Taken::Yes);
    assert!(!palette.is_open(), "show-commands from the menu does not open it again");
}

#[test]
fn a_press_on_a_row_runs_it_and_one_outside_closes_it() {
    let size = layout::WINDOW;
    let mut palette = Palette::default();
    let row = place(size).row(1);
    let on = Point { x: row.x + 10.0, y: row.y + row.height / 2.0 };
    assert_eq!(palette.press(on, size), Taken::No);
    palette.toggle();
    type_in(&mut palette, "split");
    assert_eq!(palette.press(on, size), Taken::Run(Command::SplitDown));
    assert!(!palette.is_open());

    palette.toggle();
    let search = Point { x: on.x, y: place(size).panel.y + PAD + 4.0 };
    assert_eq!(palette.press(search, size), Taken::Yes);
    assert!(palette.is_open());
    assert_eq!(palette.press(Point { x: 1.0, y: 1.0 }, size), Taken::Yes);
    assert!(!palette.is_open());
}

#[test]
fn rows_are_tall_enough_for_the_pen_and_the_panel_fits_every_window() {
    for size in [layout::SMALLEST, layout::WINDOW, Size { width: 1600.0, height: 1000.0 }] {
        let place = place(size);
        let panel = place.panel;
        assert!(panel.width <= WIDTH && place.rows >= 1, "{size:?}");
        assert!(panel.x >= 0.0 && panel.y >= 0.0, "{size:?}");
        assert!(panel.x + panel.width <= size.width && panel.y + panel.height <= size.height);
        assert!((place.row(0).height - ROW).abs() < f64::EPSILON);
        let shown = open().shown(&Keymap::default(), &T, size).unwrap();
        assert_eq!((shown.frame, shown.dim), (size.rect(), T.background));
        for line in &shown.lines {
            let f = line.label.frame;
            let corner = Point { x: f.x + f.width, y: f.y + f.height };
            assert!(panel.contains(Point { x: f.x, y: f.y }) && panel.contains(corner), "{size:?}");
        }
    }
}

#[test]
fn a_row_shows_title_group_and_key_and_the_footer_says_the_keys() {
    let texts = texts(&open(), layout::WINDOW);
    let at = texts.iter().position(|t| t == Command::ToolPen.title()).unwrap();
    assert_eq!(texts[at + 1], "Tools");
    assert_eq!(texts[at + 2], "⌘1");
    assert_eq!(texts[1], PLACEHOLDER);
    assert_eq!(texts[2], format!("{n} of {n}", n = Command::ALL.len()));
    assert_eq!(texts[texts.len() - 2..], [FOOTER, PEN]);
}

#[test]
fn the_selection_stays_in_view_and_is_filled() {
    let keys = Keymap::default();
    let mut palette = open();
    let rows = place(layout::WINDOW).rows as usize;
    for _ in 0..rows {
        palette.key(key("down"), &keys);
    }
    let texts = texts(&palette, layout::WINDOW);
    let matches = palette.matches();
    assert!(texts.contains(&matches[rows].title().to_string()));
    assert!(!texts.contains(&matches[0].title().to_string()), "the first scrolled off");
    let shown = palette.shown(&keys, &T, layout::WINDOW).unwrap();
    let filled = shown.lines.iter().filter(|l| l.label.fill == Some(style::selected(&T)));
    assert_eq!(filled.map(|l| l.radius).collect::<Vec<_>>(), [style::ROW_RADIUS]);
}

#[test]
fn the_selected_row_reads_on_its_fill_and_the_panel_is_rounded_with_a_hairline() {
    let keys = Keymap::default();
    let shown = open().shown(&keys, &T, layout::WINDOW).unwrap();
    let panel = &shown.lines[0];
    assert_eq!((panel.label.fill, panel.edge), (Some(style::panel(&T)), Some(style::edge(&T))));
    assert_eq!(panel.radius, style::PANEL_RADIUS);
    let matches = open().matches();
    let group = matches[0].group().title();
    let label = |n: usize| {
        let row = place(layout::WINDOW).row(u32::try_from(n).unwrap());
        let lines = shown.lines.iter().filter(|l| l.label.frame.y >= row.y);
        lines.filter(|l| l.label.frame.y < row.y + row.height).cloned().collect::<Vec<_>>()
    };
    let selected = label(0);
    let named = selected.iter().find(|l| l.label.text == group).unwrap();
    assert_eq!(named.label.colour, style::selected_label(&T), "the group reads on the blue");
    let chip = selected.iter().find(|l| l.radius == style::CHIP_RADIUS).unwrap();
    assert_eq!(chip.label.fill, Some(T.background));
    let other = label(1);
    let chip = other.iter().find(|l| l.radius == style::CHIP_RADIUS).unwrap();
    assert_eq!(chip.label.fill, Some(style::chip(&T)));
    let title = other.iter().find(|l| l.label.text == matches[1].title()).unwrap();
    let middle = |f: Rect| f.y + f.height / 2.0;
    assert!((middle(chip.label.frame) - middle(title.label.frame)).abs() < 1e-9, "centred");
}
