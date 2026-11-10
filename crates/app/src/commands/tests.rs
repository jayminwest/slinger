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
fn the_defaults_bind_quit_and_reload_config() {
    let keys = Keymap::default();
    assert_eq!(keys.command(chord("cmd+q")), Some(Command::Quit));
    assert_eq!(keys.command(chord("cmd+r")), Some(Command::ReloadConfig));
}

#[test]
fn a_chord_with_no_command_runs_nothing() {
    let keys = Keymap::default();
    assert_eq!(keys.command(chord("q")), None);
    assert_eq!(keys.command(chord("cmd+shift+q")), None);
    assert_eq!(keys.command(Chord::new(CMD, 'w')), None);
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
