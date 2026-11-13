use super::*;

fn chord(text: &str) -> Chord {
    Chord::parse(text).unwrap_or_else(|| panic!("{text} is a chord"))
}

#[test]
fn every_command_is_at_its_index_and_found_by_its_name() {
    for (i, &(command, name)) in Command::ALL.iter().enumerate() {
        assert_eq!(command as usize, i);
        assert_eq!(Command::named(name), Some(command));
    }
    assert_eq!(Command::named("Quit"), None);
    assert_eq!(Command::named(""), None);
}

#[test]
fn parses_modifiers_in_any_order_and_a_key() {
    assert_eq!(chord("q"), Chord::new(0, 'q'));
    assert_eq!(chord("cmd+q"), Chord::new(CMD, 'q'));
    assert_eq!(chord("cmd+shift+k"), Chord::new(CMD | SHIFT, 'k'));
    assert_eq!(chord("shift+cmd+k"), chord("cmd+shift+k"));
    assert_eq!(chord("ctrl+alt+shift+cmd+1"), Chord::new(CTRL | ALT | SHIFT | CMD, '1'));
    assert_eq!(chord("alt+/"), Chord::new(ALT, '/'));
}

#[test]
fn the_plus_key_is_written_as_itself() {
    assert_eq!(chord("+"), Chord::new(0, '+'));
    assert_eq!(chord("cmd++"), Chord::new(CMD, '+'));
    assert_eq!(chord("ctrl+cmd++"), Chord::new(CTRL | CMD, '+'));
}

#[test]
fn parses_named_keys_and_function_keys() {
    assert_eq!(chord("space"), Chord::new(0, ' '));
    assert_eq!(chord("cmd+escape"), Chord::new(CMD, '\u{1b}'));
    assert_eq!(chord("left"), Chord::new(0, '\u{f702}'));
    assert_eq!(chord("forward-delete"), Chord::new(0, '\u{f728}'));
    assert_eq!(chord("f1"), Chord::new(0, '\u{f704}'));
    assert_eq!(chord("f9"), Chord::new(0, '\u{f70c}'));
    assert_eq!(chord("ctrl+f10"), Chord::new(CTRL, '\u{f70d}'));
    assert_eq!(chord("f20"), Chord::new(0, '\u{f717}'));
}

#[test]
fn a_letter_matches_in_either_case() {
    assert_eq!(chord("cmd+Q"), chord("cmd+q"));
    // A key press with shift held reports the capital letter.
    assert_eq!(Chord::new(SHIFT | CMD, 'K'), chord("cmd+shift+k"));
}

#[test]
fn a_key_event_keeps_only_the_four_modifiers() {
    assert_eq!(Chord::new(0xf0 | CMD, 'q'), chord("cmd+q"));
}

#[test]
fn rejects_what_is_not_a_chord() {
    for text in [
        "",
        "cmd",
        "cmd+",
        "+cmd",
        "cmd+qq",
        "command+q",
        "Cmd+q",
        "cmd+cmd+q",
        "cmd++q",
        "q+cmd",
        " q",
        "cmd + q",
        "\t",
        "f0",
        "f21",
        "f01",
        "f1x",
        "fx",
        "space+q",
        "cmd+ q",
    ] {
        assert_eq!(Chord::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_defaults_bind_every_command() {
    let keys = Keymap::default();
    assert_eq!(keys.command(chord("cmd+q")), Some(Command::Quit));
    assert_eq!(keys.command(chord("cmd+r")), Some(Command::ReloadConfig));
    assert_eq!(keys.command(chord("cmd+shift+c")), Some(Command::PickColour));
    assert_eq!(keys.command(chord("cmd+shift+s")), Some(Command::PickShape));
    assert_eq!(keys.command(chord("cmd+shift+t")), Some(Command::PickTool));
}

#[test]
fn a_chord_with_no_command_runs_nothing() {
    let keys = Keymap::default();
    assert_eq!(keys.command(chord("q")), None);
    assert_eq!(keys.command(chord("cmd+shift+q")), None);
    assert_eq!(keys.command(Chord::new(CMD, 'j')), None);
}

#[test]
fn each_default_chord_runs_its_own_command_and_a_few_commands_have_none() {
    let keys = Keymap::default();
    for (command, _) in Command::ALL {
        match keys.0[command as usize] {
            Some(chord) => assert_eq!(keys.command(chord), Some(command)),
            None => assert_eq!(command, Command::ShowWorkspaceFolder, "{command:?} has no chord"),
        }
    }
    assert_eq!(keys.command(chord("cmd+alt+left")), Some(Command::FocusLeft));
    assert_eq!(keys.command(chord("ctrl+tab")), Some(Command::FocusNext));
    assert_eq!(keys.command(chord("cmd+b")), Some(Command::ToggleSidebar));
    assert_eq!(keys.command(chord("cmd+.")), Some(Command::DismissNotice));
}

#[test]
fn an_unbound_command_runs_from_no_chord() {
    let mut keys = Keymap::default();
    keys.bind(Command::Quit, None);
    assert_eq!(keys.command(chord("cmd+q")), None);
    assert_eq!(keys.command(chord("cmd+r")), Some(Command::ReloadConfig));
}

#[test]
fn binding_a_taken_chord_takes_it_from_the_other_command() {
    let mut keys = Keymap::default();
    keys.bind(Command::ReloadConfig, Some(chord("cmd+q")));
    assert_eq!(keys.command(chord("cmd+q")), Some(Command::ReloadConfig));
    assert_eq!(keys.command(chord("cmd+r")), None);
    keys.bind(Command::Quit, Some(chord("cmd+r")));
    assert_eq!(keys.command(chord("cmd+r")), Some(Command::Quit));
    assert_eq!(keys.command(chord("cmd+q")), Some(Command::ReloadConfig));
}

#[test]
fn every_command_has_a_title_and_a_group_in_the_list() {
    for (command, _) in Command::ALL {
        let title = command.title();
        assert_ne!(title, "", "{command:?}");
        assert!(title.starts_with(|c: char| c.is_ascii_uppercase()), "{title}");
        assert!(Group::ALL.contains(&command.group()), "{command:?}");
    }
    for group in Group::ALL {
        assert_ne!(group.title(), "");
        assert!(Command::ALL.iter().any(|(c, _)| c.group() == group), "{group:?} is empty");
    }
    assert_eq!(Command::SplitRight.title(), "Split right");
    assert_eq!(Group::PopUps.title(), "Pop-ups");
    let groups = [
        (Command::PickTool, Group::PopUps),
        (Command::OpenPdf, Group::File),
        (Command::OpenWorkspace, Group::File),
        (Command::CloseWorkspace, Group::File),
        (Command::ShowWorkspaceFolder, Group::File),
        (Command::StartAgent, Group::Sling),
        (Command::StopAgent, Group::Sling),
        (Command::RestartAgent, Group::Sling),
    ];
    for (command, group) in groups {
        assert_eq!(command.group(), group, "{command:?}");
    }
}

#[test]
fn no_two_commands_share_a_title() {
    for (i, (a, _)) in Command::ALL.iter().enumerate() {
        for (b, _) in &Command::ALL[i + 1..] {
            assert_ne!(a.title(), b.title(), "{a:?} and {b:?}");
        }
    }
}

#[test]
fn prints_a_chord_in_macos_glyphs() {
    for (text, glyphs) in [
        ("cmd+shift+d", "⇧⌘D"),
        ("shift+cmd+d", "⇧⌘D"),
        ("ctrl+alt+shift+cmd+1", "⌃⌥⇧⌘1"),
        ("cmd+return", "⌘↩"),
        ("cmd+shift+delete", "⇧⌘⌫"),
        ("forward-delete", "⌦"),
        ("ctrl+tab", "⌃⇥"),
        ("escape", "⎋"),
        ("alt+space", "⌥Space"),
        ("cmd+alt+left", "⌥⌘←"),
        ("alt+right", "⌥→"),
        ("alt+up", "⌥↑"),
        ("alt+down", "⌥↓"),
        ("cmd+/", "⌘/"),
        ("cmd++", "⌘+"),
        ("f1", "F1"),
        ("ctrl+f9", "⌃F9"),
        ("f10", "F10"),
        ("f20", "F20"),
    ] {
        assert_eq!(chord(text).glyphs(), glyphs, "{text}");
    }
}

/// Reads glyphs back into a chord: the inverse of `Chord::glyphs`.
fn from_glyphs(text: &str) -> Option<Chord> {
    let mut mods = 0;
    let mut rest = text;
    while let Some(&(bit, glyph)) = registry::MOD_GLYPHS.iter().find(|(_, g)| rest.starts_with(*g))
    {
        mods |= bit;
        rest = &rest[glyph.len_utf8()..];
    }
    if let Some(&(key, _)) = registry::KEY_GLYPHS.iter().find(|(_, g)| *g == rest) {
        return Some(Chord::new(mods, key));
    }
    let key = match rest.strip_prefix('F').filter(|n| !n.is_empty()) {
        Some(n) => key_named(&format!("f{n}"))?,
        None => key_named(rest)?,
    };
    Some(Chord::new(mods, key))
}

#[test]
fn the_glyphs_of_every_default_read_back_as_its_chord() {
    let keys = Keymap::default();
    for (command, chord) in Command::ALL.iter().filter_map(|&(c, _)| Some((c, keys.chord(c)?))) {
        assert_eq!(from_glyphs(&chord.glyphs()), Some(chord), "{command:?}: {}", chord.glyphs());
    }
    assert_eq!(keys.chord(Command::ShowKeys), Some(chord("cmd+/")));
    assert_eq!(keys.chord(Command::ShowCommands), Some(chord("cmd+k")));
}

#[test]
fn the_live_chord_follows_a_binding() {
    let mut keys = Keymap::default();
    keys.bind(Command::ShowCommands, Some(chord("cmd+p")));
    assert_eq!(keys.chord(Command::ShowCommands), Some(chord("cmd+p")));
    keys.bind(Command::Quit, None);
    assert_eq!(keys.chord(Command::Quit), None);
}

/// The zoom keys are the Mac ones, and what a tablet's zoom wheel sends:
/// cmd+=, cmd+- and cmd+0, with cmd++ (shift held) as cmd+=. The pane keys
/// are on option: option+= types "≠", but `AppKit` reports the key with no
/// modifier but shift ('=').
#[test]
fn the_zoom_keys_are_on_command_and_the_pane_keys_on_option() {
    let keys = Keymap::default();
    assert_eq!(keys.command(Chord::new(CMD, '=')), Some(Command::ZoomIn));
    assert_eq!(keys.command(Chord::new(CMD | SHIFT, '+')), Some(Command::ZoomIn));
    assert_eq!(keys.command(Chord::new(CMD, '-')), Some(Command::ZoomOut));
    assert_eq!(keys.command(Chord::new(CMD, '0')), Some(Command::ZoomReset));
    assert_eq!(keys.command(Chord::new(ALT, '=')), Some(Command::GrowPane));
    assert_eq!(keys.command(Chord::new(ALT, '-')), Some(Command::ShrinkPane));
    assert_eq!(keys.command(Chord::new(ALT, '≠')), None, "never the typed character");
}
