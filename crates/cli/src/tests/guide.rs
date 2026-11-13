//! `slinger guide` and `slinger skill install`, and the tests that hold the
//! guide, the skill, the plugin and the sling note to the CLI: a command or
//! flag they name that the CLI does not have fails here, so they change with
//! the code.
use std::fs;
use std::path::Path;

use clap::Command;
use serde_json::Value;
use tempfile::TempDir;

use super::{Run, json};
use crate::cli::{command_tree, run};
use crate::guide::{GUIDE, LISTEN, SKILL, SKILLS, entries};
use crate::output::exit;

const PLUGIN: &str = include_str!("../../../../plugin/.claude-plugin/plugin.json");
const MARKET: &str = include_str!("../../../../.claude-plugin/marketplace.json");
/// The note a sling sends an agent, as the core's snapshot tests store it: the
/// CLI does not link the core, so it reads the note the core is held to.
const NOTES: [(&str, &str, usize); 3] = [
    ("first", include_str!("../../../core/tests/fixtures/sling/first.txt"), 3),
    ("later", include_str!("../../../core/tests/fixtures/sling/later.txt"), 2),
    ("listener", include_str!("../../../core/tests/fixtures/sling/listener.txt"), 5),
];

/// Runs the CLI with no daemon: its socket is not there.
fn alone(args: &[&str]) -> Run {
    let mut line = vec!["slinger", "--socket", "/nonexistent/slinger.sock"];
    line.extend(args);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(line, &mut std::io::empty(), &mut out, &mut err, false);
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
    Run { code, out: text(out), err: text(err) }
}

#[test]
fn guide_prints_the_written_part_and_every_command_with_no_daemon() {
    let run = alone(&["guide"]);
    assert_eq!((run.code, run.err.as_str()), (exit::OK, ""));
    assert!(run.out.starts_with(GUIDE.lines().next().unwrap()), "{}", run.out);
    let all = entries(&command_tree());
    assert!(all.len() > 40, "{} commands", all.len());
    for entry in &all {
        let line = format!("\n{}\n    {}", entry.usage, entry.about);
        assert!(run.out.contains(&line), "the guide lacks {}", entry.name);
        assert!(!entry.about.is_empty(), "{} has no help", entry.name);
    }
}

#[test]
fn guide_shows_a_command_as_the_user_types_it() {
    let all = entries(&command_tree());
    let usage = |name: &str| all.iter().find(|e| e.name == name).unwrap().usage.clone();
    assert_eq!(usage("slinger canvas create"), "slinger canvas create <CANVAS>");
    assert_eq!(
        usage("slinger session start"),
        "slinger session start <WORKSPACE> [--fresh] [--profile <NAME>]"
    );
    assert_eq!(usage("slinger canvas strokes"), "slinger canvas strokes <CANVAS> --item <ID>");
    assert_eq!(usage("slinger workspace open"), "slinger workspace open [<DIR>] [--name <NAME>]");
}

#[test]
fn guide_as_json_holds_the_text_and_the_commands() {
    let doc = json(&alone(&["guide", "--json"]));
    assert_eq!((&doc["success"], &doc["command"]), (&true.into(), &"guide".into()));
    assert!(doc["guide"].as_str().unwrap().contains("## Commands"));
    let commands = doc["commands"].as_array().unwrap();
    let put = commands.iter().find(|c| c["name"] == "slinger canvas put").unwrap();
    assert!(put["usage"].as_str().unwrap().contains("--beside <ID>"));
}

#[test]
fn skill_install_writes_the_plugins_skill_once() {
    let dir = TempDir::new().unwrap();
    let skills = dir.path().join("made").join("skills");
    let path = skills.join("slinger").join("SKILL.md");
    let listen = skills.join("slinger-listen").join("SKILL.md");
    let (skills, shown) = (skills.to_str().unwrap(), path.to_str().unwrap());
    let first = alone(&["skill", "install", "--dir", skills]);
    let both = format!("{shown}\n{}", listen.display());
    assert_eq!((first.code, first.out.trim_end()), (exit::OK, both.as_str()));
    assert_eq!(fs::read_to_string(&path).unwrap(), SKILL);
    assert_eq!(fs::read_to_string(&listen).unwrap(), LISTEN);

    let again = json(&alone(&["skill", "install", "--json", "--dir", skills]));
    let again = again["skills"].as_array().unwrap();
    assert_eq!((&again[0]["new"], &again[0]["path"]), (&false.into(), &shown.into()));
    assert_eq!((&again[1]["new"], &again[1]["name"]), (&false.into(), &"slinger-listen".into()));

    // An older skill is replaced by this binary's; the other is left.
    fs::write(&path, "old").unwrap();
    let newer = json(&alone(&["skill", "install", "--json", "--dir", skills]));
    assert_eq!(
        (&newer["skills"][0]["new"], &newer["skills"][1]["new"]),
        (&true.into(), &false.into())
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), SKILL);
}

#[test]
fn skill_install_fails_loudly_where_it_cannot_write() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file");
    fs::write(&file, "").unwrap();
    let run = alone(&["skill", "install", "--dir", file.to_str().unwrap()]);
    assert_eq!(run.code, exit::ERROR);
    assert!(run.err.contains("cannot make"), "{}", run.err);
}

#[test]
fn skill_has_the_front_matter_a_harness_needs_and_sends_the_agent_to_the_guide() {
    for (name, skill) in SKILLS {
        let mut parts = skill.splitn(3, "---\n");
        let (before, front, body) = (parts.next(), parts.next().unwrap(), parts.next().unwrap());
        assert_eq!(before, Some(""));
        assert!(front.lines().any(|l| l == format!("name: {name}")), "{front}");
        let description = front.lines().find_map(|l| l.strip_prefix("description: ")).unwrap();
        assert!((40..=1024).contains(&description.len()), "{}", description.len());
        assert!(body.contains("slinger guide"));
    }
    assert!(LISTEN.contains("slinger session listen <WORKSPACE>"));
}

#[test]
fn plugin_has_the_version_of_the_binary_and_the_marketplace_lists_it() {
    let plugin: Value = serde_json::from_str(PLUGIN).unwrap();
    assert_eq!(plugin["name"], "slinger");
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(plugin["version"], version, "bump plugin/.claude-plugin/plugin.json");
    let market: Value = serde_json::from_str(MARKET).unwrap();
    let listed = &market["plugins"][0];
    assert_eq!((&listed["name"], &listed["source"]), (&"slinger".into(), &"./plugin".into()));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (name, _) in SKILLS {
        assert!(root.join("plugin/skills").join(name).join("SKILL.md").is_file());
    }
}

/// Each `slinger ...` command a document shows: in a code block, a line that
/// starts with it; in the text, a backticked span that starts with it.
fn shown(doc: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (n, block) in doc.split("```").enumerate() {
        if n % 2 == 1 {
            found.extend(block.lines().map(str::trim).map(str::to_string));
        } else {
            let spans = block.split('`').skip(1).step_by(2);
            found.extend(spans.map(|s| s.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
    }
    found.retain(|line| line.starts_with("slinger "));
    found
}

/// What is wrong with one shown command, if anything: a word that is no
/// command, or a flag its command does not take.
fn fault(root: &Command, line: &str) -> Option<String> {
    let line = line.split(" #").next().unwrap_or(line).trim_end();
    let mut words = line.split_whitespace().skip(1).peekable();
    let mut command = root;
    while command.has_subcommands() {
        match words.peek() {
            // A whole group (`slinger canvas`), or a placeholder (`slinger <command> --help`).
            None => return None,
            Some(word) if word.starts_with(['<', '-']) => return None,
            Some(word) => match command.find_subcommand(word) {
                Some(next) => command = next,
                None => return Some(format!("`{line}`: no command `{word}`")),
            },
        }
        words.next();
    }
    let global = ["json", "quiet", "verbose", "socket", "help"];
    let takes = |flag: &str| {
        global.contains(&flag) || command.get_arguments().any(|a| a.get_long() == Some(flag))
    };
    let flags = words.filter_map(|w| w.strip_prefix("--"));
    let unknown = flags.map(|f| f.split('=').next().unwrap_or(f)).find(|f| !takes(f));
    unknown.map(|flag| format!("`{line}`: `{}` takes no --{flag}", command.get_name()))
}

#[test]
fn guide_and_skill_show_only_commands_and_flags_the_cli_has() {
    let root = command_tree();
    let docs = [("guide.md", GUIDE, 27), ("SKILL.md", SKILL, 4), ("slinger-listen", LISTEN, 6)];
    for (name, doc, least) in docs {
        let lines = shown(doc);
        assert!(lines.len() >= least, "{name} shows {} commands", lines.len());
        let faults: Vec<String> = lines.iter().filter_map(|line| fault(&root, line)).collect();
        assert_eq!(faults, [] as [String; 0], "{name} is behind the CLI");
    }
}

#[test]
fn the_sling_note_shows_only_commands_and_flags_the_cli_has_and_points_at_the_guide() {
    let root = command_tree();
    for (name, note, least) in NOTES {
        let lines = shown(note);
        assert!(lines.len() >= least, "the {name} note shows {} commands", lines.len());
        let faults: Vec<String> = lines.iter().filter_map(|line| fault(&root, line)).collect();
        assert_eq!(faults, [] as [String; 0], "the {name} note is behind the CLI");
    }
    assert!(shown(NOTES[0].1).contains(&"slinger guide".to_string()));
}

#[test]
fn a_stale_command_or_flag_is_a_fault() {
    let root = command_tree();
    let doc =
        "Run `slinger canvas\n  paint x`.\n```bash\nslinger canvas put c --near i1  # no\n```";
    let lines = shown(doc);
    assert_eq!(lines, ["slinger canvas paint x", "slinger canvas put c --near i1  # no"]);
    assert_eq!(fault(&root, &lines[0]).unwrap(), "`slinger canvas paint x`: no command `paint`");
    let near = "`slinger canvas put c --near i1`: `put` takes no --near";
    assert_eq!(fault(&root, &lines[1]).unwrap(), near);
    assert_eq!(fault(&root, "slinger canvas put c --beside i1 --text hi --json"), None);
    assert_eq!(fault(&root, "slinger <command> --help"), None);
    assert_eq!(fault(&root, "slinger canvas"), None);
}
