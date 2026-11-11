use super::*;

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap_or_else(|| panic!("{text} is a chord"))
}

fn items(menus: &[Menu]) -> impl Iterator<Item = &Item> {
    menus.iter().flat_map(|m| &m.items)
}

/// The item that does `action` in the bar.
fn find(menus: &[Menu], action: Action) -> Item {
    *items(menus).find(|i| i.action == action).unwrap_or_else(|| panic!("{action:?} is in a menu"))
}

#[test]
fn every_command_is_in_exactly_one_menu() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    for &(command, name) in &Command::ALL {
        let count = items(&menus).filter(|i| i.action == Action::Run(command)).count();
        assert_eq!(count, 1, "{name}");
    }
}

#[test]
fn the_menus_come_in_the_macos_order_with_the_app_menu_first() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    let titles: Vec<&str> = menus.iter().map(|m| m.title).collect();
    let order = ["slinger", "File", "Edit", "Tools", "Canvas", "Pane", "Window", "Help"];
    assert_eq!(titles, order);
}

#[test]
fn only_the_pens_tool_has_a_check_mark() {
    let menus = bar(&Keymap::default(), Command::ToolLasso);
    let checked: Vec<Action> = items(&menus).filter(|i| i.checked).map(|i| i.action).collect();
    assert_eq!(checked, [Action::Run(Command::ToolLasso)]);
}

#[test]
fn an_item_shows_its_commands_live_key_and_title() {
    let mut keys = Keymap::default();
    let split = find(&bar(&keys, Command::ToolPen), Action::Run(Command::SplitRight));
    assert_eq!((split.title, split.key), ("Split right", Some(chord("cmd+d"))));
    keys.bind(Command::SplitRight, Some(chord("cmd+shift+e")));
    let split = find(&bar(&keys, Command::ToolPen), Action::Run(Command::SplitRight));
    assert_eq!(split.key, Some(chord("cmd+shift+e")));
    keys.bind(Command::SplitRight, None);
    assert_eq!(find(&bar(&keys, Command::ToolPen), Action::Run(Command::SplitRight)).key, None);
}

#[test]
fn a_key_that_types_stays_off_the_menu_for_the_text_field() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    for command in [Command::DeleteSelection, Command::ClearSelection, Command::PanLeft] {
        assert_eq!(find(&menus, Action::Run(command)).key, None, "{command:?}");
    }
    assert_eq!(find(&menus, Action::Run(Command::FocusNext)).key, Some(chord("ctrl+tab")));
}

#[test]
fn an_edit_item_keeps_its_usual_key_unless_the_keymap_binds_it() {
    let mut keys = Keymap::default();
    let copy = find(&bar(&keys, Command::ToolPen), Action::Native(Native::Copy));
    assert_eq!((copy.title, copy.key), ("Copy", Some(chord("cmd+c"))));
    keys.bind(Command::PickColour, Some(chord("cmd+c")));
    let menus = bar(&keys, Command::ToolPen);
    assert_eq!(find(&menus, Action::Native(Native::Copy)).key, None);
    assert_eq!(find(&menus, Action::Run(Command::PickColour)).key, Some(chord("cmd+c")));
}

#[test]
fn no_two_items_answer_to_one_key() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    let keys: Vec<Chord> = items(&menus).filter_map(|i| i.key).collect();
    for (n, key) in keys.iter().enumerate() {
        assert!(!keys[n + 1..].contains(key), "{key:?}");
    }
    assert_eq!(find(&menus, Action::Native(Native::About)).key, None);
}

#[test]
fn a_chord_gives_back_its_modifiers_and_key() {
    let c = chord("cmd+shift+k");
    assert_eq!((c.mods(), c.key()), (CMD | super::super::SHIFT, 'k'));
}
