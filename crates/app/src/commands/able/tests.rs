use super::*;

/// The commands that can always act: on an empty window too.
const ALWAYS: [Command; 33] = [
    Command::Quit,
    Command::ReloadConfig,
    Command::PickColour,
    Command::PickShape,
    Command::PickTool,
    Command::PickWidth,
    Command::PickSize,
    Command::SplitRight,
    Command::SplitDown,
    Command::ToggleSidebar,
    Command::ToolPen,
    Command::ToolEraser,
    Command::ToolLasso,
    Command::ToolText,
    Command::ToolBox,
    Command::ToolArrow,
    Command::ToolSelect,
    Command::CropPdf,
    Command::WidthUp,
    Command::WidthDown,
    Command::SizeUp,
    Command::SizeDown,
    Command::Undo,
    Command::Redo,
    Command::ShowKeys,
    Command::ShowCommands,
    Command::LeaveFeedback,
    Command::ShowAllFeedback,
    Command::ShowFeedbackFile,
    Command::ToggleGrid,
    Command::OpenPdf,
    Command::OpenWorkspace,
    Command::SelectAll,
];

/// A canvas pane with its page and nothing selected, the items holding the keys.
const PAGE: Facts = Facts { panes: 1, canvas: true, pages: 1, items: true, ..NONE };
const NONE: Facts = Facts {
    panes: 1,
    canvas: false,
    pages: 0,
    ui: false,
    ui_pane: false,
    items: false,
    selected: false,
    boxed: false,
    inside: false,
    sling: false,
    entry: false,
    sling_now: false,
    new_text: false,
    workspace: false,
    agent: None,
    notice: false,
    managed: false,
    old: false,
};

/// Each rule: its commands, a window where they act and one where they do not.
fn rules() -> Vec<(&'static [Command], Facts, Facts)> {
    use Command as C;
    let selected = Facts { selected: true, ..PAGE };
    let boxed = Facts { boxed: true, ..selected };
    let shown = Facts { workspace: true, ..PAGE };
    vec![
        (
            &[
                C::ClosePane,
                C::FocusNext,
                C::FocusLeft,
                C::FocusRight,
                C::FocusUp,
                C::FocusDown,
                C::GrowPane,
                C::ShrinkPane,
            ],
            Facts { panes: 2, ..PAGE },
            PAGE,
        ),
        (&[C::DismissNotice], Facts { notice: true, ..NONE }, NONE),
        (
            &[
                C::PanLeft,
                C::PanRight,
                C::PanUp,
                C::PanDown,
                C::ZoomIn,
                C::ZoomOut,
                C::ZoomReset,
                C::ZoomToFit,
                C::Save,
                C::ShowCanvasFile,
                C::Find,
                C::FindNext,
            ],
            Facts { canvas: true, ..NONE },
            Facts { ui: true, ..NONE },
        ),
        (&[C::NewPage, C::RenamePage], PAGE, Facts { pages: 0, ..PAGE }),
        (&[C::NextPage, C::PreviousPage, C::RemovePage], Facts { pages: 2, ..PAGE }, PAGE),
        (&[C::NextItem, C::PreviousItem], PAGE, Facts { items: false, ..PAGE }),
        (&[C::NextItem, C::PreviousItem], PAGE, Facts { pages: 0, ..PAGE }),
        (
            &[
                C::DeleteSelection,
                C::Duplicate,
                C::NudgeLeft,
                C::NudgeRight,
                C::NudgeUp,
                C::NudgeDown,
                C::NudgeLeftFine,
                C::NudgeRightFine,
                C::NudgeUpFine,
                C::NudgeDownFine,
                C::ClearSelection,
            ],
            selected,
            PAGE,
        ),
        (&[C::DeleteSelection, C::ClearSelection], selected, Facts { items: false, ..selected }),
        (&[C::ClearSelection, C::StepOut], Facts { inside: true, ..PAGE }, PAGE),
        (&[C::ClearSelection], Facts { inside: true, ..PAGE }, Facts { items: false, ..PAGE }),
        (&[C::Fold, C::StepIn, C::StepInBeside], boxed, selected),
        (&[C::Fold], boxed, Facts { items: false, ..boxed }),
        (&[C::StepIn, C::StepInBeside], Facts { items: false, ..boxed }, PAGE),
        (&[C::Sling, C::Dictate], Facts { sling: true, ..PAGE }, PAGE),
        (&[C::Dictate], Facts { entry: true, ..NONE }, NONE),
        (&[C::SlingNow], Facts { sling_now: true, ..PAGE }, PAGE),
        (&[C::NewText], Facts { new_text: true, ..NONE }, PAGE),
        (&[C::CloseWorkspace, C::ShowWorkspaceFolder], shown, PAGE),
        (&[C::StartAgent, C::RestartAgent], Facts { agent: Some(false), ..shown }, shown),
        (&[C::StopAgent, C::RestartAgent], Facts { agent: Some(true), ..shown }, shown),
        (
            &[C::StartAgent],
            Facts { agent: Some(false), ..shown },
            Facts { agent: Some(true), ..shown },
        ),
        (
            &[C::StopAgent],
            Facts { agent: Some(true), ..shown },
            Facts { agent: Some(false), ..shown },
        ),
        (
            &[C::RestartAgent],
            Facts { agent: Some(true), ..shown },
            Facts { agent: Some(true), ..PAGE },
        ),
        (&[C::ExportUi], Facts { ui: true, ..NONE }, Facts { canvas: true, ..NONE }),
        (&[C::ExportUi], Facts { ui: true, ..NONE }, Facts { ui_pane: true, ..NONE }),
        (&[C::ReloadUi], Facts { ui_pane: true, ..NONE }, Facts { canvas: true, ..NONE }),
        (&[C::StartSlingerd, C::RestartDaemon], Facts { managed: true, ..NONE }, NONE),
        (&[C::StartSlingerd], Facts { managed: true, old: true, ..NONE }, NONE),
        (
            &[C::RestartDaemon],
            Facts { managed: true, ..NONE },
            Facts { managed: true, old: true, ..NONE },
        ),
    ]
}

#[test]
fn each_rule_lets_its_commands_act_in_one_window_and_not_in_the_other() {
    for (commands, on, off) in rules() {
        for &command in commands {
            assert!(can(command, &on), "{command:?} acts in {on:?}");
            assert!(!can(command, &off), "{command:?} cannot act in {off:?}");
        }
    }
}

#[test]
fn every_command_has_a_rule_or_can_always_act() {
    let all = rules();
    for &(command, name) in &Command::ALL {
        let ruled = all.iter().any(|(commands, ..)| commands.contains(&command));
        assert!(ruled != ALWAYS.contains(&command), "{name}: one rule, or always");
    }
}

#[test]
fn what_can_always_act_can_act_on_an_empty_window() {
    let empty = Facts::default();
    for command in ALWAYS {
        assert!(can(command, &empty), "{command:?}");
    }
    let refused = Command::ALL.iter().filter(|&&(c, _)| !can(c, &empty)).count();
    assert_eq!(refused, Command::ALL.len() - ALWAYS.len(), "the rest wait for something");
}
