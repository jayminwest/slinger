use super::*;

const EXAMPLE: &str = include_str!("../../../../config.example.toml");

fn kind(text: &str) -> (usize, ErrorKind) {
    let e = parse(text).unwrap_err();
    (e.line, e.kind)
}

#[test]
fn an_empty_file_is_every_default() {
    assert_eq!(parse("").unwrap().resolve(), Config::default());
    assert_eq!(parse("# only a note\n\n[theme]\n").unwrap(), Layer::default());
}

#[test]
fn parses_every_key() {
    let text = "[theme]\nbackground = \"#000000\"\ntext = \"#FFFFFF\"\n\
                muted=\"#010203\"   # a note\nerror = \"#ff0000\"\n\
                [daemon]  # the daemon\nsocket = \"~/s.sock\"\n";
    let config = parse(text).unwrap().resolve();
    let theme = Theme {
        background: rgb(0, 0, 0),
        text: rgb(255, 255, 255),
        muted: rgb(1, 2, 3),
        error: rgb(255, 0, 0),
    };
    assert_eq!(config, Config { theme, socket: Some("~/s.sock".into()), keys: Keymap::default() });
}

#[test]
fn a_string_keeps_its_escapes_and_a_hash_inside_quotes() {
    let layer = parse("[daemon]\nsocket = \"/a \\\"b\\\" \\\\ #c\" # note\n").unwrap();
    assert_eq!(layer.resolve().socket.as_deref(), Some("/a \"b\" \\ #c"));
}

#[test]
fn the_overlay_wins_key_by_key() {
    let base = parse("[theme]\ntext = \"#111111\"\nmuted = \"#222222\"\n").unwrap();
    let overlay = parse("[theme]\nmuted = \"#333333\"\n[daemon]\nsocket = \"/x\"\n").unwrap();
    let config = base.over(overlay).resolve();
    assert_eq!(config.theme.text, rgb(0x11, 0x11, 0x11));
    assert_eq!(config.theme.muted, rgb(0x33, 0x33, 0x33));
    assert_eq!(config.theme.error, Theme::default().error);
    assert_eq!(config.socket.as_deref(), Some("/x"));
}

#[test]
fn an_empty_overlay_changes_nothing() {
    let base = parse("[theme]\ntext = \"#111111\"\n").unwrap();
    assert_eq!(base.clone().over(Layer::default()), base);
}

#[test]
fn a_line_that_is_nothing_is_a_syntax_error_at_its_line() {
    assert_eq!(kind("[theme]\n\nbackground\n"), (3, ErrorKind::Syntax));
    assert_eq!(kind("[theme\n"), (1, ErrorKind::Syntax));
    assert_eq!(kind("[theme] x\n"), (1, ErrorKind::Syntax));
    assert_eq!(kind("[theme]\n = \"#000000\"\n"), (2, ErrorKind::Syntax));
    assert_eq!(kind("[theme]\nthe text = \"#000000\"\n"), (2, ErrorKind::Syntax));
}

#[test]
fn an_unknown_table_is_an_error() {
    assert_eq!(kind("[themes]\n"), (1, ErrorKind::UnknownTable));
}

#[test]
fn an_unknown_key_is_an_error_with_its_table() {
    assert_eq!(kind("[theme]\ncolour = \"#000000\""), (2, ErrorKind::UnknownKey));
    assert_eq!(kind("socket = \"/x\""), (1, ErrorKind::UnknownKey));
    assert_eq!(kind("[daemon]\ntext = \"#000000\""), (2, ErrorKind::UnknownKey));
}

#[test]
fn a_key_set_twice_is_an_error_at_the_second() {
    let text = "[theme]\ntext = \"#000000\"\n[daemon]\n[theme]\ntext = \"#000000\"\n";
    assert_eq!(kind(text), (5, ErrorKind::DuplicateKey));
}

#[test]
fn a_value_must_be_one_quoted_string() {
    for value in ["#000000", "\"#000000", "\"#000000\" x", "\"a\\nb\"", "", "\"a\\"] {
        let text = format!("[daemon]\nsocket = {value}\n");
        assert_eq!(kind(&text), (2, ErrorKind::NotAString), "{value}");
    }
}

#[test]
fn a_colour_must_be_six_hex_digits() {
    for value in ["000000", "#00000", "#0000000", "#00000g", "#00000é", "#+12345", "red"] {
        let text = format!("[theme]\ntext = \"{value}\"\n");
        assert_eq!(kind(&text), (2, ErrorKind::BadColour), "{value}");
    }
}

#[test]
fn every_error_names_its_line_and_has_a_hint() {
    let kinds = [
        ErrorKind::Syntax,
        ErrorKind::UnknownTable,
        ErrorKind::UnknownKey,
        ErrorKind::DuplicateKey,
        ErrorKind::NotAString,
        ErrorKind::BadColour,
        ErrorKind::UnknownCommand,
        ErrorKind::BadChord,
        ErrorKind::ChordTaken,
    ];
    let shown: Vec<String> = kinds
        .into_iter()
        .map(|kind| Error { line: 7, kind })
        .map(|e| format!("{e} / {}", e.hint()))
        .collect();
    assert_eq!(
        shown,
        [
            "line 7: not a table, a setting or a comment / write a [table] or key = \"value\"; start a comment with #",
            "line 7: unknown table / config.example.toml lists every table and key",
            "line 7: unknown key / config.example.toml lists every table and key",
            "line 7: the key is set twice / set each key once; put machine values in the overlay",
            "line 7: the value is not a quoted string / quote the value: key = \"value\"",
            "line 7: the value is not a colour / write a colour as \"#rrggbb\"",
            "line 7: unknown command / config.example.toml lists every table and key",
            "line 7: the value is not a key chord / write a chord as \"cmd+shift+k\", or \"\" for none",
            "line 7: another command is on this chord / bind each chord to one command",
        ]
    );
}

#[test]
fn the_example_parses_to_the_defaults_and_lists_every_key() {
    assert_eq!(parse(EXAMPLE).unwrap().resolve(), Config::default());
    // Uncomment each setting: every key is known, and the defaults are the defaults.
    let uncommented: String = EXAMPLE
        .lines()
        .map(|l| l.strip_prefix('#').filter(|s| s.starts_with(char::is_alphabetic)).unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");
    let all = parse(&uncommented).unwrap();
    assert_eq!(all.clone().resolve().theme, Theme::default());
    let every_key = Layer {
        background: Some(Theme::default().background),
        text: Some(Theme::default().text),
        muted: Some(Theme::default().muted),
        error: Some(Theme::default().error),
        socket: all.socket.clone(),
        // The defaults, each written out.
        keys: [Some(Chord::parse("cmd+q")), Some(Chord::parse("cmd+r"))],
    };
    assert_eq!(all, every_key);
    assert!(all.socket.is_some());
}

#[test]
fn expands_a_leading_tilde_only() {
    let home = Path::new("/home/me");
    assert_eq!(expand_home("~/s.sock", Some(home)), PathBuf::from("/home/me/s.sock"));
    assert_eq!(expand_home("/a/~/b", Some(home)), PathBuf::from("/a/~/b"));
    assert_eq!(expand_home("~/s.sock", None), PathBuf::from("~/s.sock"));
}

fn command(config: &Config, chord: &str) -> Option<Command> {
    config.keys.command(Chord::parse(chord).unwrap())
}

#[test]
fn keys_binds_commands_to_chords() {
    let config =
        parse("[keys]\nquit = \"ctrl+alt+q\"\nreload-config = \"f5\"\n").unwrap().resolve();
    assert_eq!(command(&config, "ctrl+alt+q"), Some(Command::Quit));
    assert_eq!(command(&config, "f5"), Some(Command::ReloadConfig));
    assert_eq!(command(&config, "cmd+q"), None);
    assert_eq!(command(&config, "cmd+r"), None);
}

#[test]
fn an_unbound_command_is_allowed() {
    let config = parse("[keys]\nquit = \"\"\n").unwrap().resolve();
    assert_eq!(command(&config, "cmd+q"), None);
    assert_eq!(command(&config, "cmd+r"), Some(Command::ReloadConfig));
}

#[test]
fn an_unknown_command_is_an_error_at_its_line() {
    assert_eq!(kind("[keys]\n\nquit-now = \"cmd+q\"\n"), (3, ErrorKind::UnknownCommand));
}

#[test]
fn a_chord_that_does_not_parse_is_an_error_at_its_line() {
    assert_eq!(kind("[keys]\nquit = \"cmd+\"\n"), (2, ErrorKind::BadChord));
    assert_eq!(kind("[keys]\nquit = \"hyper+q\"\n"), (2, ErrorKind::BadChord));
}

#[test]
fn two_commands_on_one_chord_in_one_file_is_an_error_at_the_second() {
    let text = "[keys]\nquit = \"cmd+k\"\nreload-config = \"cmd+K\"\n";
    assert_eq!(kind(text), (3, ErrorKind::ChordTaken));
    let text = "[keys]\nquit = \"\"\nreload-config = \"\"\n";
    assert!(parse(text).is_ok(), "two unbound commands share no chord");
    assert_eq!(kind("[keys]\nquit = \"cmd+k\"\nquit = \"cmd+k\"\n"), (3, ErrorKind::DuplicateKey));
}

#[test]
fn a_chord_bound_in_a_file_is_taken_from_the_default_command_on_it() {
    let config = parse("[keys]\nreload-config = \"cmd+q\"\n").unwrap().resolve();
    assert_eq!(command(&config, "cmd+q"), Some(Command::ReloadConfig));
    assert_eq!(command(&config, "cmd+r"), None);
}

#[test]
fn the_overlay_takes_a_chord_from_the_shared_file() {
    let base = parse("[keys]\nquit = \"cmd+k\"\nreload-config = \"cmd+j\"\n").unwrap();
    let overlay = parse("[keys]\nreload-config = \"cmd+k\"\n").unwrap();
    let config = base.over(overlay).resolve();
    assert_eq!(command(&config, "cmd+k"), Some(Command::ReloadConfig));
    assert_eq!(command(&config, "cmd+j"), None);
    assert_eq!(command(&config, "cmd+q"), None, "the shared file moved quit off its default");
}
