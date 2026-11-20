use super::*;
use crate::commands::Chord;

/// A window with two panes, a selection and a box it is stepped into.
const ABLE: Facts = Facts {
    panes: 2,
    canvas: true,
    pages: 2,
    ui: true,
    items: true,
    selected: true,
    boxed: true,
    inside: true,
    sling: true,
    entry: false,
    sling_now: true,
    new_text: true,
    workspace: true,
    agent: Some(true),
    notice: false,
    managed: false,
    old: false,
};

const SUBJECTS: [Subject; 3] =
    [Subject::Canvas, Subject::Pane { ui: true }, Subject::Pane { ui: false }];

fn titles(items: &[Item]) -> Vec<&'static str> {
    items.iter().map(|i| if i.action == Action::Separator { "-" } else { i.title }).collect()
}

fn of(subject: Subject) -> Vec<Item> {
    items(subject, &Keymap::default(), Command::ToolPen, &ABLE)
}

fn find(items: &[Item], command: Command) -> Item {
    *items.iter().find(|i| i.action == Action::Run(command)).expect("in the menu")
}

#[test]
fn each_subject_s_items_in_order() {
    let canvas = [
        "Paste",
        "New text",
        "Select all",
        "-",
        "Sling now",
        "-",
        "Zoom to 100%",
        "Dotted grid",
        "-",
        "New page",
        "Step out of box",
    ];
    assert_eq!(titles(&of(Subject::Canvas)), canvas);
    let ui = ["Export agent UI…", "-", "Split right", "Split down", "Close pane"];
    assert_eq!(titles(&of(Subject::Pane { ui: true })), ui);
    assert_eq!(
        titles(&of(Subject::Pane { ui: false })),
        ["Split right", "Split down", "Close pane"]
    );
}

#[test]
fn a_rebound_key_shows_on_its_item_and_a_key_that_types_does_not() {
    let mut keys = Keymap::default();
    let chord = |text| Chord::parse(text).unwrap();
    keys.bind(Command::SplitRight, Some(chord("cmd+shift+e")));
    keys.bind(Command::ClosePane, Some(chord("x")));
    let pane = items(Subject::Pane { ui: false }, &keys, Command::ToolPen, &ABLE);
    assert_eq!(find(&pane, Command::SplitRight).key, Some(chord("cmd+shift+e")));
    assert_eq!(find(&pane, Command::ClosePane).key, None, "a key that types stays off");
    let canvas = of(Subject::Canvas);
    assert_eq!(find(&canvas, Command::ZoomReset).key, Some(chord("cmd+0")));
    let paste = canvas.iter().find(|i| i.action == Action::Native(Native::Paste)).unwrap();
    assert_eq!((paste.key, paste.enabled), (Some(chord("cmd+v")), true), "native: always on");
}

#[test]
fn an_item_is_grey_where_its_command_cannot_act() {
    let alone = Facts { panes: 1, inside: false, ..ABLE };
    let pane = items(Subject::Pane { ui: false }, &Keymap::default(), Command::ToolPen, &alone);
    assert!(!find(&pane, Command::ClosePane).enabled, "one pane: close is grey");
    assert!(find(&pane, Command::SplitRight).enabled);
    let canvas = items(Subject::Canvas, &Keymap::default(), Command::ToolPen, &alone);
    assert!(!find(&canvas, Command::StepOut).enabled, "not stepped in");
    assert!(find(&of(Subject::Canvas), Command::StepOut).enabled, "stepped in");
    for subject in SUBJECTS {
        for item in of(subject) {
            if let Action::Run(command) = item.action {
                assert_eq!(item.enabled, crate::commands::able::can(command, &ABLE));
            }
        }
    }
}

#[test]
fn no_menu_has_a_stray_separator_or_a_command_twice() {
    for subject in SUBJECTS {
        let items = of(subject);
        let lines: Vec<bool> = items.iter().map(|i| i.action == Action::Separator).collect();
        assert!(!lines.first().unwrap() && !lines.last().unwrap(), "{subject:?}");
        assert!(!lines.windows(2).any(|w| w[0] && w[1]), "{subject:?}");
        let mut actions: Vec<String> = items
            .iter()
            .filter(|i| i.action != Action::Separator)
            .map(|i| format!("{:?}", i.action))
            .collect();
        let all = actions.len();
        actions.sort();
        actions.dedup();
        assert_eq!(actions.len(), all, "{subject:?}: no command twice");
    }
}

#[test]
fn export_agent_ui_only_for_an_agent_ui_pane() {
    let export =
        |items: Vec<Item>| items.iter().any(|i| i.action == Action::Run(Command::ExportUi));
    assert!(export(of(Subject::Pane { ui: true })));
    assert!(!export(of(Subject::Pane { ui: false })));
    assert!(!export(of(Subject::Canvas)));
}

#[test]
fn tidy_drops_separators_at_the_ends_and_in_a_row() {
    let sep = menu::item(S, &Keymap::default(), Command::ToolPen, &ABLE);
    let new = menu::item(R(Command::NewPage), &Keymap::default(), Command::ToolPen, &ABLE);
    assert_eq!(tidy(vec![sep, new, sep, sep, new, sep]), [new, sep, new]);
    assert_eq!(tidy(vec![sep, sep]), []);
}
