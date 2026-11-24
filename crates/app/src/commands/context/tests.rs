//! Which context menu a click pops: the empty canvas, a footer, an agent UI
//! pane's footer (never its page); an item's kind's or the selection's; a
//! text being typed offers cut, copy and paste. Items grey by `able::can`.

use super::*;
use crate::commands::Chord;

/// A window with two panes, a selection and a box it is stepped into.
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
    workspace: true,
    agent: Some(true),
    notice: false,
    managed: false,
    old: false,
};

const KINDS: [Kind; 6] = [Kind::Stroke, Kind::Text, Kind::Box, Kind::Arrow, Kind::Pdf, Kind::Live];

const SUBJECTS: [Subject; 12] = [
    Subject::Canvas,
    Subject::Pane { ui: true },
    Subject::Pane { ui: false },
    Subject::Item { kind: KINDS[0] },
    Subject::Item { kind: KINDS[1] },
    Subject::Item { kind: KINDS[2] },
    Subject::Item { kind: KINDS[3] },
    Subject::Item { kind: KINDS[4] },
    Subject::Item { kind: KINDS[5] },
    Subject::Selection { boxed: true },
    Subject::Selection { boxed: false },
    Subject::Typing,
];

fn titles(items: &[Item]) -> Vec<&'static str> {
    items.iter().map(|i| if i.action == Action::Separator { "-" } else { i.title }).collect()
}

fn of(subject: &Subject) -> Vec<Item> {
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
    assert_eq!(titles(&of(&Subject::Canvas)), canvas);
    let ui =
        ["Reload agent UI", "Export agent UI…", "-", "Split right", "Split down", "Close pane"];
    assert_eq!(titles(&of(&Subject::Pane { ui: true })), ui);
    assert_eq!(
        titles(&of(&Subject::Pane { ui: false })),
        ["Split right", "Split down", "Close pane"]
    );
}

#[test]
fn a_rebound_key_shows_on_its_item_and_a_key_that_types_does_not() {
    let mut keys = Keymap::default();
    let chord = |text| Chord::parse(text).unwrap();
    keys.bind(Command::SplitRight, Some(chord("cmd+shift+e")));
    keys.bind(Command::ClosePane, Some(chord("x")));
    let pane = items(&Subject::Pane { ui: false }, &keys, Command::ToolPen, &ABLE);
    assert_eq!(find(&pane, Command::SplitRight).key, Some(chord("cmd+shift+e")));
    assert_eq!(find(&pane, Command::ClosePane).key, None, "a key that types stays off");
    let canvas = of(&Subject::Canvas);
    assert_eq!(find(&canvas, Command::ZoomReset).key, Some(chord("cmd+0")));
    let paste = canvas.iter().find(|i| i.action == Action::Native(Native::Paste)).unwrap();
    assert_eq!((paste.key, paste.enabled), (Some(chord("cmd+v")), true), "native: always on");
}

#[test]
fn an_item_is_grey_where_its_command_cannot_act() {
    let alone = Facts { panes: 1, inside: false, ..ABLE };
    let pane = items(&Subject::Pane { ui: false }, &Keymap::default(), Command::ToolPen, &alone);
    assert!(!find(&pane, Command::ClosePane).enabled, "one pane: close is grey");
    assert!(find(&pane, Command::SplitRight).enabled);
    let canvas = items(&Subject::Canvas, &Keymap::default(), Command::ToolPen, &alone);
    assert!(!find(&canvas, Command::StepOut).enabled, "not stepped in");
    assert!(find(&of(&Subject::Canvas), Command::StepOut).enabled, "stepped in");
    for subject in SUBJECTS {
        for item in of(&subject) {
            if let Action::Run(command) = item.action {
                assert_eq!(item.enabled, crate::commands::able::can(command, &ABLE));
            }
        }
    }
}

#[test]
fn no_menu_has_a_stray_separator_or_a_command_twice() {
    for subject in SUBJECTS {
        let items = of(&subject);
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
    assert!(export(of(&Subject::Pane { ui: true })));
    assert!(!export(of(&Subject::Pane { ui: false })));
    assert!(!export(of(&Subject::Canvas)));
}

#[test]
fn tidy_drops_separators_at_the_ends_and_in_a_row() {
    let sep = menu::item(S, &Keymap::default(), Command::ToolPen, &ABLE);
    let new = menu::item(R(Command::NewPage), &Keymap::default(), Command::ToolPen, &ABLE);
    assert_eq!(tidy(vec![sep, new, sep, sep, new, sep]), [new, sep, new]);
    assert_eq!(tidy(vec![sep, sep]), []);
}

/// What any item offers before what its kind adds.
const ANY_ITEM: [&str; 8] =
    ["Sling selection", "-", "Cut", "Copy", "Duplicate", "Delete selection", "-", "Pick a colour"];

fn with_any(extra: &[&'static str]) -> Vec<&'static str> {
    ANY_ITEM.iter().chain(extra).copied().collect()
}

#[test]
fn each_kind_of_item_s_items_in_order() {
    let item = |kind| titles(&of(&Subject::Item { kind }));
    assert_eq!(item(Kind::Stroke), with_any(&[]));
    assert_eq!(item(Kind::Arrow), with_any(&[]));
    assert_eq!(item(Kind::Text), with_any(&["Pick a text size", "Bigger text", "Smaller text"]));
    let boxed = ["-", "Fold or unfold box", "Step into box", "Step into box beside"];
    assert_eq!(item(Kind::Box), with_any(&boxed));
    assert_eq!(item(Kind::Pdf), with_any(&["-", "Crop PDF", "Show canvas file in Finder"]));
    assert_eq!(item(Kind::Live), ["Sling selection", "Delete selection"]);
}

#[test]
fn a_selection_offers_fold_only_with_a_box_and_typing_only_the_clipboard() {
    let tail = ["Pick a colour", "Select nothing"];
    let plain = titles(&of(&Subject::Selection { boxed: false }));
    assert_eq!(plain, ANY_ITEM[..7].iter().chain(&tail).copied().collect::<Vec<_>>());
    let boxed = titles(&of(&Subject::Selection { boxed: true }));
    let fold = ANY_ITEM[..7].iter().chain(&["Fold or unfold box"]).chain(&tail);
    assert_eq!(boxed, fold.copied().collect::<Vec<_>>());
    assert_eq!(titles(&of(&Subject::Typing)), ["Cut", "Copy", "Paste"]);
    let fold = |s| of(&s).iter().any(|i| i.action == Action::Run(Command::Fold));
    for kind in KINDS {
        assert_eq!(fold(Subject::Item { kind }), kind == Kind::Box, "{kind:?}");
    }
}

#[test]
fn an_item_s_commands_grey_where_the_items_do_not_have_the_keys() {
    let typing = Facts { items: false, ..ABLE };
    let menu =
        items(&Subject::Item { kind: Kind::Box }, &Keymap::default(), Command::ToolPen, &typing);
    for command in [Command::DeleteSelection, Command::Duplicate, Command::Fold] {
        assert!(!find(&menu, command).enabled, "{command:?}");
    }
    assert!(find(&menu, Command::StepIn).enabled, "step-in reads the selection only");
}

fn workspace() -> Subject {
    Subject::Workspace { dir: "/work/notes".into() }
}

#[test]
fn a_row_a_divider_and_the_sidebar_offer_their_own_items() {
    let row = ["Close workspace", "Show workspace folder in Finder", "-", "Start agent"];
    let row = row.iter().chain(&["Stop agent", "Restart agent in a new conversation"]);
    assert_eq!(titles(&of(&workspace())), row.copied().collect::<Vec<_>>());
    let divider = Subject::Divider { before: crate::layout::PaneId::numbered(1), ratio: 0.5 };
    assert_eq!(titles(&of(&divider)), ["Grow pane", "Shrink pane"]);
    assert_eq!(titles(&of(&Subject::Sidebar)), ["Open workspace…", "Show or hide sidebar"]);
}

#[test]
fn a_row_s_agent_items_grey_by_that_row_s_agent() {
    let menu = |agent| {
        let facts = Facts { agent, ..ABLE };
        items(&workspace(), &Keymap::default(), Command::ToolPen, &facts)
    };
    let on = |items: &[Item], c| find(items, c).enabled;
    let runs = menu(Some(true));
    assert!(!on(&runs, Command::StartAgent) && on(&runs, Command::StopAgent));
    let stopped = menu(Some(false));
    assert!(on(&stopped, Command::StartAgent) && !on(&stopped, Command::StopAgent));
    let none = menu(None);
    assert!(!on(&none, Command::StartAgent) && !on(&none, Command::RestartAgent));
    assert!(on(&none, Command::CloseWorkspace), "a row with no agent still closes");
}

#[test]
fn only_a_row_s_and_a_divider_s_own_commands_carry_their_subject() {
    let divider = Subject::Divider { before: crate::layout::PaneId::numbered(1), ratio: 0.5 };
    assert!(carried(Command::CloseWorkspace, &workspace()));
    assert!(carried(Command::RestartAgent, &workspace()));
    assert!(!carried(Command::OpenWorkspace, &workspace()));
    assert!(carried(Command::GrowPane, &divider) && carried(Command::ShrinkPane, &divider));
    assert!(!carried(Command::ClosePane, &divider));
    for subject in SUBJECTS {
        assert!(!carried(Command::CloseWorkspace, &subject), "{subject:?}");
        assert!(!carried(Command::GrowPane, &subject), "{subject:?}");
    }
}
