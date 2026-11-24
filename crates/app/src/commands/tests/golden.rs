//! Every command's facts as the surfaces show them, against a checked-in
//! fixture (`golden.txt`): its name, title, group and default chord, the
//! `[keys]` lines of `config.example.toml`, the menu bar and each context
//! menu. Recorded before the command table (slinger-3bf67b) and kept so a
//! change to any command's facts is a change to this file, made on purpose.
//! `SLINGER_BLESS=1 cargo test -p slinger-app golden` writes it again.

use std::fmt::Write as _;
use std::path::PathBuf;

use super::super::able::Facts;
use super::super::context::{self, Kind, Subject};
use super::super::menu::{self, Action, Item};
use super::*;
use crate::layout::PaneId;

const GOLDEN: &str = include_str!("golden.txt");
const EXAMPLE: &str = include_str!("../../../../../config.example.toml");

/// A window where every command can act.
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
    text: true,
    workspace: true,
    agent: Some(false),
    notice: true,
    managed: true,
    old: true,
};

fn glyphs(chord: Option<Chord>) -> String {
    chord.map_or_else(|| "-".into(), Chord::glyphs)
}

fn items(out: &mut String, items: &[Item]) {
    for item in items {
        let what = match item.action {
            Action::Run(command) => format!("run {command:?}"),
            Action::Native(native) => format!("native {native:?}"),
            Action::Separator => "---".into(),
        };
        let check = if item.checked { " checked" } else { "" };
        let grey = if item.enabled { "" } else { " grey" };
        let _ = writeln!(out, "  {what} | {} | {}{check}{grey}", item.title, glyphs(item.key));
    }
}

/// The facts the fixture holds, as text.
fn recorded() -> String {
    let keys = Keymap::default();
    let mut out = String::from("# commands: index name | title | group | default chord\n");
    for (i, &(command, name)) in Command::ALL.iter().enumerate() {
        let (title, group) = (command.title(), command.group().title());
        let _ = writeln!(out, "{i} {name} | {title} | {group} | {}", glyphs(keys.chord(command)));
    }
    out.push_str("# config.example.toml [keys]\n");
    let at = EXAMPLE.find("\n[keys]\n").expect("a [keys] table");
    for line in EXAMPLE[at..].lines().filter(|l| l.starts_with('#') && l.contains(" = \"")) {
        let _ = writeln!(out, "{line}");
    }
    out.push_str("# menu bar (checked: tool-pen; every command can act)\n");
    for bar in menu::bar(&keys, Command::ToolPen, &ABLE) {
        let _ = writeln!(out, "{} {:?}", bar.title, bar.role);
        items(&mut out, &bar.items);
    }
    out.push_str("# context menus (checked: tool-pen; nothing can act)\n");
    let kinds =
        [Kind::Stroke, Kind::Text, Kind::LongText, Kind::Box, Kind::Arrow, Kind::Pdf, Kind::Live];
    let subjects = [Subject::Canvas, Subject::Pane { ui: false }, Subject::Pane { ui: true }]
        .into_iter()
        .chain(kinds.map(|kind| Subject::Item { kind }))
        .chain([
            Subject::Selection { boxed: false },
            Subject::Selection { boxed: true },
            Subject::Typing,
            Subject::Workspace { dir: PathBuf::from("/w") },
            Subject::Divider { before: PaneId::numbered(1), ratio: 0.5 },
            Subject::Sidebar,
        ]);
    for subject in subjects {
        let _ = writeln!(out, "{subject:?}");
        items(&mut out, &context::items(&subject, &keys, Command::ToolPen, &Facts::default()));
    }
    out
}

#[test]
fn every_command_s_facts_are_as_recorded() {
    let now = recorded();
    if std::env::var_os("SLINGER_BLESS").is_some() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/commands/tests/golden.txt");
        std::fs::write(path, &now).unwrap();
        return;
    }
    for (line, (want, got)) in GOLDEN.lines().zip(now.lines()).enumerate() {
        assert_eq!(got, want, "golden.txt line {}", line + 1);
    }
    assert_eq!(now.lines().count(), GOLDEN.lines().count(), "golden.txt's length");
}
