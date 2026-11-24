use super::*;

/// A window where every command but `start-agent` (its agent runs) can act.
const ABLE: Facts = Facts {
    panes: 2,
    canvas: true,
    pages: 2,
    ui: true,
    ui_pane: true,
    items: true,
    selected: true,
    boxed: true,
    steps: true,
    inside: true,
    sling: true,
    entry: false,
    sling_now: true,
    new_text: true,
    figure: true,
    workspace: true,
    agent: Some(true),
    notice: true,
    managed: true,
    old: false,
};

/// The bar for a window where (nearly) everything can act.
fn bar(keys: &Keymap, checked: Command) -> Vec<Menu> {
    super::bar(keys, checked, &ABLE)
}

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

#[test]
fn an_item_is_grey_where_its_command_cannot_act_and_an_edit_item_never() {
    let none = Facts::default();
    let menus = super::bar(&Keymap::default(), Command::ToolPen, &none);
    for item in items(&menus) {
        let want = match item.action {
            Action::Run(command) => able::can(command, &none),
            Action::Native(_) | Action::Separator => true,
        };
        assert_eq!(item.enabled, want, "{:?}", item.action);
    }
    assert!(!find(&menus, Action::Run(Command::DeleteSelection)).enabled);
    assert!(!find(&menus, Action::Run(Command::ClosePane)).enabled);
    assert!(find(&menus, Action::Run(Command::SplitRight)).enabled);
    assert!(find(&menus, Action::Native(Native::Paste)).enabled);
    let able = bar(&Keymap::default(), Command::ToolPen);
    let grey: Vec<Action> = items(&able).filter(|i| !i.enabled).map(|i| i.action).collect();
    assert_eq!(grey, [Action::Run(Command::StartAgent)], "an agent that runs cannot start");
}

#[test]
fn the_same_layout_is_a_change_only_in_what_is_grey() {
    let keys = Keymap::default();
    let able = bar(&keys, Command::ToolPen);
    let none = super::bar(&keys, Command::ToolPen, &Facts::default());
    assert_ne!(able, none);
    assert!(same_layout(&able, &none));
    assert!(same_layout(&able, &able));
    let checked = bar(&keys, Command::ToolLasso);
    assert!(!same_layout(&able, &checked), "a check mark moved");
    let mut rekeyed = keys;
    rekeyed.bind(Command::SplitRight, Some(chord("cmd+shift+e")));
    assert!(!same_layout(&able, &bar(&rekeyed, Command::ToolPen)), "a key changed");
    let mut retitled = able.clone();
    retitled[1].items[0].title = "Open";
    assert!(!same_layout(&able, &retitled), "a title changed");
    let mut reordered = able.clone();
    reordered[1].items.swap(0, 1);
    assert!(!same_layout(&able, &reordered), "two items swapped");
    let mut shorter = able.clone();
    shorter[1].items.pop();
    assert!(!same_layout(&able, &shorter), "an item went");
    assert!(!same_layout(&able, &able[1..]), "a menu went");
}

/// The titles of `menu`'s items, a separator as "-".
fn titles(menu: &Menu) -> Vec<&str> {
    menu.items.iter().map(|i| if i.action == Action::Separator { "-" } else { i.title }).collect()
}

#[test]
fn the_app_menu_and_the_window_menu_hold_what_every_mac_app_has() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    let app = [
        "About slinger",
        "-",
        "Reload config",
        "-",
        "Services",
        "-",
        "Hide slinger",
        "Hide Others",
        "Show All",
        "-",
        "Quit slinger",
    ];
    assert_eq!(titles(&menus[0]), app);
    let window = menus.iter().find(|m| m.role == Role::Window).unwrap();
    let want = [
        "Minimize",
        "Zoom",
        "Enter Full Screen",
        "-",
        "Show or hide sidebar",
        "Dismiss notice",
        Command::StartSlingerd.title(),
        Command::RestartDaemon.title(),
        "-",
        "Bring All to Front",
    ];
    assert_eq!(titles(window), want);
    let keys = |native| find(&menus, Action::Native(native)).key;
    assert_eq!(keys(Native::Hide), Some(chord("cmd+h")));
    assert_eq!(keys(Native::HideOthers), Some(chord("alt+cmd+h")));
    assert_eq!(keys(Native::FullScreen), Some(chord("ctrl+cmd+f")));
    assert_eq!(keys(Native::ShowAll), None);
}

#[test]
fn no_menu_starts_or_ends_with_a_separator_or_has_two_in_a_row() {
    for menu in bar(&Keymap::default(), Command::ToolPen) {
        let lines: Vec<bool> = menu.items.iter().map(|i| i.action == Action::Separator).collect();
        assert!(
            !lines.first().unwrap_or(&true) && !lines.last().unwrap_or(&true),
            "{}",
            menu.title
        );
        assert!(!lines.windows(2).any(|w| w[0] && w[1]), "{}", menu.title);
    }
}

#[test]
fn hide_loses_its_key_while_the_keymap_binds_cmd_h() {
    let mut keys = Keymap::default();
    keys.bind(Command::ToggleGrid, Some(chord("cmd+h")));
    let menus = bar(&keys, Command::ToolPen);
    assert_eq!(find(&menus, Action::Native(Native::Hide)).key, None);
    assert_eq!(find(&menus, Action::Run(Command::ToggleGrid)).key, Some(chord("cmd+h")));
}

#[test]
fn exactly_one_menu_is_the_app_s_the_window_menu_and_the_help_menu() {
    let menus = bar(&Keymap::default(), Command::ToolPen);
    for role in [Role::App, Role::Window, Role::Help] {
        assert_eq!(menus.iter().filter(|m| m.role == role).count(), 1, "{role:?}");
    }
    assert_eq!(menus[0].role, Role::App);
    assert_eq!(menus.iter().find(|m| m.role == Role::Help).unwrap().title, "Help");
}

#[test]
fn a_moved_separator_is_another_layout() {
    let able = bar(&Keymap::default(), Command::ToolPen);
    let mut moved = able.clone();
    moved[0].items.swap(1, 2);
    assert!(!same_layout(&able, &moved));
}
