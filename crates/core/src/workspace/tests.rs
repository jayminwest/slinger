use super::*;

const FIXTURE: &str = include_str!("../../fixtures/workspace/slinger-workspace.toml");

fn with_primary(name: &str, machine: &str, path: &str) -> Workspace {
    let mut ws = Workspace::named(name);
    ws.roots.push(Root::primary(machine, path));
    ws
}

#[test]
fn reads_the_stored_fixture() {
    let ws = Workspace::parse(FIXTURE).unwrap();
    assert_eq!(ws.version(), 1);
    assert_eq!(ws.name, "slinger");
    assert_eq!(ws.tag, "slinger");
    assert_eq!(ws.canvas, "main");
    assert_eq!(
        ws.agent.as_ref().map(|a| (a.name.as_str(), a.durable)),
        Some(("slinger-dev", true))
    );
    assert_eq!(ws.roots.len(), 2);
    let primary = ws.primary_root().unwrap();
    assert_eq!(
        (primary.machine.as_str(), primary.path.as_str()),
        ("my-laptop", "/Users/me/Projects/slinger")
    );
    assert!(!ws.roots[1].primary);
    assert_eq!(ws.layout.version(), 1);
}

#[test]
fn the_fixture_round_trips_with_its_unknown_fields() {
    let ws = Workspace::parse(FIXTURE).unwrap();
    let text = ws.to_toml();
    assert_eq!(Workspace::parse(&text).unwrap(), ws);
    for kept in ["colour = \"teal\"", "mount = \"ro\"", "split = \"columns\"", "panes = ["] {
        assert!(text.contains(kept), "{kept} missing from:\n{text}");
    }
}

#[test]
fn a_new_workspace_round_trips() {
    let mut ws = Workspace::new("notes");
    ws.agent = Some(Agent::new("scribe", false));
    ws.roots.push(Root::new("my-laptop", "/Users/me/notes"));
    let text = ws.to_toml();
    assert_eq!(Workspace::parse(&text).unwrap(), ws);
    assert_eq!(ws.version(), VERSION);
}

#[test]
fn an_agent_reads_its_limits_and_writes_back_only_the_ones_set() {
    let text = "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\nuser_config = false\n\
                dirs = [\"/srv/docs\"]\ntools = [\"Edit\", \"Bash(git *)\"]\n";
    let ws = Workspace::parse(text).unwrap();
    assert!(ws.to_toml().contains("user_config = false"), "an opt-out is kept");
    let agent = ws.agent.unwrap();
    assert!(!agent.user_config);
    let plain = Workspace::parse("version = 1\nname = \"x\"\n[agent]\nname = \"a\"\n").unwrap();
    assert!(plain.agent.unwrap().user_config, "the owner's own config, by default");
    assert_eq!(
        (agent.dirs, agent.tools),
        (vec!["/srv/docs".into()], vec!["Edit".into(), "Bash(git *)".into()])
    );
    let text = "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\n\
                tools = [\"Bash(git:*)\", \"mcp__gh__get_issue\", \"Sudo\"]\n";
    let err = Workspace::parse(text).unwrap_err();
    assert!(err.to_string().contains("agent.tools: \"Sudo\""), "{err}");
    assert!(err.hint().unwrap().contains("mcp__<server>__<tool>"), "the way out is named");
    let mut ws = Workspace::new("y");
    ws.agent = Some(Agent::new("b", false));
    assert!(ws.to_toml().ends_with("[agent]\nname = \"b\"\n"), "{}", ws.to_toml());
}

#[test]
fn a_minimal_file_gets_the_defaults() {
    let ws = Workspace::parse("version = 1\nname = \"scratch\"\n").unwrap();
    assert_eq!(ws, Workspace::new("scratch"));
    assert_eq!(ws.tag, "scratch");
    assert_eq!(ws.canvas, DEFAULT_CANVAS);
    assert!(ws.roots.is_empty() && ws.agent.is_none());
    assert_eq!(ws.layout.version(), 0);
    assert_eq!(
        ws.to_toml(),
        "version = 1\nname = \"scratch\"\ntag = \"scratch\"\ncanvas = \"main\"\n"
    );
}

#[test]
fn a_newer_version_is_still_read_and_kept() {
    let ws = Workspace::parse("version = 7\nname = \"later\"\nnew_thing = 3\n").unwrap();
    assert_eq!(ws.version(), 7);
    assert!(ws.to_toml().starts_with("version = 7\n"));
    assert!(ws.to_toml().contains("new_thing = 3"));
}

#[test]
fn a_refusal_names_the_field_that_broke_a_rule() {
    for (text, field) in [
        ("version = 0\nname = \"z\"", Some("version")),
        ("version = 1\nname = \" \"", Some("name")),
        ("version = 1\nname = \"x\"\n[[root]]\nmachine = \"m\"\npath = \"rel\"", Some("root.path")),
        (
            "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ntools = [\"Shell\"]",
            Some("agent.tools"),
        ),
        ("version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ndurable = true", Some("agent.durable")),
        ("not toml at all", None),
    ] {
        let rejected = Workspace::read(text).unwrap_err();
        assert_eq!(rejected.field, field, "{text}");
        assert_eq!(rejected.error.code(), "invalid", "{text}");
    }
    let err = Workspace::parse("version = 1\nname = \"x\"\ncanvas = 3\n").unwrap_err();
    assert!(err.to_string().starts_with("line 3: "), "{err}");
}

#[test]
fn a_file_that_does_not_read_still_says_its_name_and_canvas() {
    let text =
        "version = 1\nname = \"x\"\ncanvas = \"c\"\n[agent]\nname = \"a\"\ntools = [\"Shell\"]";
    assert!(Workspace::parse(text).is_err());
    assert_eq!(peek(text), (Some("x".into()), Some("c".into())));
    assert_eq!(peek("version = 1\nname = \"x\""), (Some("x".into()), None), "no default canvas");
    assert_eq!(peek("not toml"), (None, None));
    assert_eq!(peek("name = 3"), (None, None));
}

#[test]
fn rejects_files_that_are_not_workspaces() {
    for text in [
        "name = \"no version\"",
        "version = 0\nname = \"zero\"",
        "version = 1",
        "version = 1\nname = \"  \"",
        "version = 1\nname = \"x\"\ncanvas = \"\"",
        "not toml at all",
        "version = 1\nname = \"x\"\n[[root]]\nmachine = \"m\"\npath = \"relative/dir\"",
        "version = 1\nname = \"x\"\n[[root]]\nmachine = \"\"\npath = \"/a\"",
        "version = 1\nname = \"x\"\n[[root]]\npath = \"/a\"",
        "version = 1\nname = \"x\"\n[agent]\nname = \"\"",
        "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ndirs = [\"notes\"]",
        "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ntools = [\" \"]",
        "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ntools = [\"Shell\"]",
        "version = 1\nname = \"x\"\n[agent]\nname = \"a\"\ntools = [\"Bash(git\"]",
        "version = 1\nname = \"x\"\n[agent]\nname = \"a#dev\"",
        "version = 1\nname = \"x\"\n[layout]\nsplit = \"no version\"",
    ] {
        let err = Workspace::parse(text).unwrap_err();
        assert_eq!(err.code(), "invalid", "{text}");
        assert!(err.hint().is_some(), "{text}");
    }
}

#[test]
fn rule_a_durable_agent_needs_a_primary_root() {
    let mut ws = Workspace::new("home");
    ws.agent = Some(Agent::new("keeper", true));
    ws.roots.push(Root::new("my-laptop", "/Users/me/home"));
    let err = check(std::slice::from_ref(&ws)).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.to_string().contains("durable agent"), "{err}");
    assert!(Workspace::parse(&ws.to_toml()).is_err());

    ws.roots[0].primary = true;
    check(&[ws]).unwrap();
}

#[test]
fn rule_a_workspace_has_at_most_one_primary_root() {
    let mut ws = with_primary("two", "my-laptop", "/a");
    ws.roots.push(Root::primary("my-laptop", "/b"));
    let err = check(&[ws]).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.to_string().contains("2 primary roots"), "{err}");
}

#[test]
fn a_workspace_without_an_agent_or_roots_keeps_the_rules() {
    check(&[Workspace::named("loose"), Workspace::named("also loose")]).unwrap();
    let mut ws = Workspace::new("plain");
    ws.agent = Some(Agent::new("visitor", false));
    check(&[ws]).unwrap();
}

#[test]
fn rule_a_second_workspace_on_one_primary_root_is_a_conflict() {
    let first = with_primary("first", "my-laptop", "/Users/me/project");
    let second = with_primary("second", "my-laptop", "/Users/me/project/");
    let err = check(&[first, second]).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("\"first\""), "{err}");
    assert!(err.hint().unwrap().contains("\"first\""));
}

#[test]
fn one_directory_may_be_a_plain_root_of_many_workspaces() {
    let first = with_primary("first", "my-laptop", "/shared");
    let mut second = with_primary("second", "my-laptop", "/other");
    second.roots.push(Root::new("my-laptop", "/shared"));
    let mut third = Workspace::named("third");
    third.roots.push(Root::new("my-laptop", "/shared"));
    check(&[first, second, third]).unwrap();
}

#[test]
fn the_same_path_on_another_machine_is_another_directory() {
    let a = with_primary("a", "my-laptop", "/srv/x");
    let b = with_primary("b", "server", "/srv/x");
    check(&[a, b]).unwrap();
    assert!(Root::new("m", "/").same_dir(&Root::new("m", "//")));
    assert!(!Root::new("m", "/a").same_dir(&Root::new("n", "/a")));
}

#[test]
fn rule_a_name_and_a_default_canvas_belong_to_one_workspace() {
    let err = check(&[Workspace::named("notes"), Workspace::named("notes")]).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("\"notes\""), "{err}");
    let err = check(&[Workspace::new("a"), Workspace::new("b")]).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("canvas \"main\"") && err.to_string().contains("\"a\""));
    assert!(err.hint().unwrap().contains("\"b\""));
}

#[test]
fn rule_a_broken_file_keeps_the_name_and_canvas_it_states() {
    let ws = Workspace::named("notes");
    let err = check_reserved(&ws, "/w/old", Some("notes"), None).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert!(err.to_string().contains("name \"notes\"") && err.to_string().contains("/w/old"));
    assert!(err.hint().unwrap().contains("workspace close /w/old"), "{:?}", err.hint());
    let err = check_reserved(&ws, "/w/old", Some("old"), Some("notes")).unwrap_err();
    assert!(err.to_string().contains("canvas \"notes\""), "{err}");
    check_reserved(&ws, "/w/old", Some("old"), Some("old")).unwrap();
    check_reserved(&ws, "/w/old", None, None).unwrap();
}

#[test]
fn a_named_workspace_opens_on_a_canvas_of_its_own() {
    let ws = Workspace::named("My Notes!");
    assert_eq!(
        (ws.name.as_str(), ws.tag.as_str(), ws.canvas.as_str()),
        ("My Notes!", "My Notes!", "my-notes")
    );
    assert_eq!(Workspace::parse(&ws.to_toml()).unwrap(), ws);
}

#[test]
fn a_workspace_for_a_directory_is_its_primary_root() {
    let ws = Workspace::for_dir("my-laptop", "/Users/me/Projects/slinger/", None);
    assert_eq!((ws.name.as_str(), ws.canvas.as_str()), ("slinger", "slinger"));
    let root = ws.primary_root().unwrap();
    assert_eq!(
        (root.machine.as_str(), root.path.as_str()),
        ("my-laptop", "/Users/me/Projects/slinger")
    );
    assert_eq!(ws.roots.len(), 1);
    let named = Workspace::for_dir("my-laptop", "/srv/x", Some("Work"));
    assert_eq!((named.name.as_str(), named.canvas.as_str()), ("Work", "work"));
    assert_eq!(Workspace::for_dir("m", "/", None).primary_root().unwrap().path, "/");
    check(&[ws, named]).unwrap();
}

#[test]
fn a_slug_is_a_short_safe_id() {
    for (name, slug_) in [
        ("slinger", "slinger"),
        ("Two  Words", "two-words"),
        ("--a.b--", "a-b"),
        ("snake_case-9", "snake_case-9"),
        ("日本", "workspace"),
        ("", "workspace"),
    ] {
        assert_eq!(slug(name), slug_, "{name:?}");
    }
    assert_eq!(slug(&"x".repeat(100)).len(), 64);
    assert!(!slug(&format!("{}!z", "x".repeat(63))).ends_with('-'));
}

#[test]
fn an_event_takes_the_tag_of_its_canvas_or_agent() {
    let mut a = Workspace::named("alpha");
    a.tag = "a-tag".into();
    a.agent = Some(Agent::new("scout", false));
    let mut b = Workspace::named("beta");
    b.agent = Some(Agent::new("shared", false));
    let mut c = Workspace::named("gamma");
    c.agent = Some(Agent::new("shared", false));
    let all = [a, b, c];
    assert_eq!(tag_for(&all, Some("alpha"), None), Some("a-tag"));
    assert_eq!(tag_for(&all, Some("beta"), Some("scout")), Some("beta"));
    assert_eq!(tag_for(&all, Some("nope"), Some("scout")), Some("a-tag"));
    assert_eq!(tag_for(&all, None, Some("shared")), None);
    assert_eq!(tag_for(&all, None, None), None);
    assert_eq!(tag_for(&[], Some("alpha"), Some("scout")), None);
}

#[test]
fn panes_default_to_one_on_the_canvas_and_are_stored_as_text() {
    let mut ws = Workspace::named("My Notes");
    assert_eq!(ws.panes().encode(), "canvas:my-notes");
    let mut layout = ws.panes();
    let board = panes::Pane::new(panes::Kind::Ui, "board").unwrap();
    layout.open(board, Some(1), panes::Axis::Row).unwrap();
    ws.set_panes(&layout);
    let text = ws.to_toml();
    assert!(text.contains("[layout]\nversion = 1\npanes = \"row 0.5 canvas:my-notes ui:board\""));
    assert_eq!(Workspace::parse(&text).unwrap().panes(), layout);
}

#[test]
fn a_layout_this_version_cannot_read_opens_one_pane_and_a_change_replaces_it() {
    // The fixture's layout is version 1, but its `panes` is not text.
    let fixture = Workspace::parse(FIXTURE).unwrap();
    assert_eq!(fixture.panes().encode(), "canvas:main");
    let newer =
        "version = 1\nname = \"x\"\n[layout]\nversion = 2\npanes = \"canvas:x\"\nzoom = 2\n";
    let mut ws = Workspace::parse(newer).unwrap();
    assert_eq!(ws.panes().encode(), "canvas:main");
    ws.set_panes(&ws.panes());
    let text = ws.to_toml();
    assert!(text.contains("[layout]\nversion = 1\npanes = \"canvas:main\"\n"), "{text}");
    assert!(!text.contains("zoom"));
    let mut same = Workspace::parse(FIXTURE).unwrap();
    same.set_panes(&same.panes());
    assert!(same.to_toml().contains("split = \"columns\""), "a version 1 layout keeps its fields");
}
