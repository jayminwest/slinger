use super::*;
use crate::commands::{CMD, Chord};
use crate::layout::Point;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 255, g: 255, b: 255 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 1000.0, height: 400.0 };

fn pen(tool: Tool, state: State) -> Pen {
    Pen { tool, colour: RED, width: 16.0, state }
}

/// A canvas pane's footer: the pen, page "ideas · 2/3", 100%, and another pane.
fn canvas(frame: Rect, keys: &Keymap, pen: Pen, sidebar: bool) -> Footer {
    let shows = Shows {
        pen: Some(pen),
        centre: "ideas · 2/3",
        pages: true,
        zoom: Some(100),
        sidebar,
        close: true,
        sling: false,
        agent: None,
    };
    footer(frame, &T, keys, &shows)
}

/// Each part's text, its runs joined, left to right.
fn said(footer: &Footer) -> Vec<String> {
    footer.parts.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect()).collect()
}

/// What each button does, left to right.
fn taps(footer: &Footer) -> Vec<Tap> {
    footer.targets.iter().map(|t| t.1).collect()
}

fn run(command: Command) -> Tap {
    Tap::Run(command)
}

#[test]
fn a_wide_canvas_footer_has_the_pen_the_page_the_zoom_the_panes_and_the_keys() {
    let keys = Keymap::default();
    let footer = canvas(PANE, &keys, pen(Tool::Pen, State::Selected(2)), true);
    let pen_key = keys.chord(Command::ToolPen).map(Chord::glyphs).unwrap_or_default();
    let (overlay, palette) = (Chord::new(CMD, '/').glyphs(), Chord::new(CMD, 'k').glyphs());
    assert_eq!(
        said(&footer),
        [
            "◧".to_string(),
            format!("Pen {pen_key}"),
            "· 2 selected".into(),
            "↶".into(),
            "↷".into(),
            "‹".into(),
            "ideas · 2/3".into(),
            "›".into(),
            "−".into(),
            "100%".into(),
            "+".into(),
            "◫".into(),
            "⊟".into(),
            "✕".into(),
            format!("{overlay} keys"),
            format!("{palette} commands"),
        ]
    );
    assert_eq!(
        taps(&footer),
        [
            run(Command::ToggleSidebar),
            run(Command::PickColour),
            run(Command::PickWidth),
            run(Command::PickTool),
            run(Command::Undo),
            run(Command::Redo),
            run(Command::PreviousPage),
            Tap::Pages,
            run(Command::NextPage),
            run(Command::ZoomOut),
            run(Command::ZoomReset),
            run(Command::ZoomIn),
            run(Command::SplitRight),
            run(Command::SplitDown),
            run(Command::ClosePane),
            run(Command::ShowKeys),
            run(Command::ShowCommands),
        ]
    );
    let name = &footer.parts[1].runs[0];
    assert_eq!((name.colour, name.bold), (T.text, true), "the tool is bold in the text colour");
    let rest = footer.parts.iter().enumerate().filter(|(i, _)| *i != 1).flat_map(|(_, p)| &p.runs);
    assert!(rest.filter(|r| !r.bold).all(|r| r.colour == T.muted), "every button is quiet");
    assert_eq!(footer.frame, Rect { y: 400.0 - HEIGHT, height: HEIGHT, ..PANE });
    assert_eq!(footer.fill, style::footer(&T));
    assert_eq!(footer.rule.frame, Rect { height: 1.0, ..footer.frame }, "a hairline on top");
    assert_eq!(footer.lit, None);
}

#[test]
fn every_button_is_a_pen_sized_target_in_the_footer_and_none_overlap() {
    let footer = canvas(PANE, &Keymap::default(), pen(Tool::Pen, State::Idle), true);
    let bar = footer.frame;
    for (frame, tap) in &footer.targets {
        assert!(frame.width >= style::TAP && frame.height >= style::TAP, "{tap:?} {frame:?}");
        assert!(frame.x >= bar.x && frame.x + frame.width <= bar.x + bar.width, "{tap:?}");
        assert_eq!((frame.y, frame.height), (bar.y, bar.height), "{tap:?}");
    }
    let mut frames: Vec<Rect> = footer.targets.iter().map(|t| t.0).collect();
    frames.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert!(frames.windows(2).all(|w| w[0].x + w[0].width <= w[1].x + 1e-9), "no overlap");
    let name = footer.parts.iter().find(|p| p.runs[0].text == "ideas · 2/3").unwrap().frame;
    let page = footer.targets.iter().find(|t| t.1 == Tap::Pages).unwrap().0;
    assert!(page.contains(Point { x: name.x + 1.0, y: bar.y + 2.0 }), "the name is its button");
    let middle = page.x + page.width / 2.0;
    assert!((middle - (PANE.x + PANE.width / 2.0)).abs() < 1e-9, "a wide pane centres the page");
}

#[test]
fn the_swatch_picks_a_colour_and_is_a_line_as_thick_as_the_pen_up_to_a_square() {
    let keys = Keymap::default();
    let swatch = |width: f64| {
        let footer = canvas(PANE, &keys, Pen { width, ..pen(Tool::Pen, State::Idle) }, false);
        let (mark, colour) = footer.marks[0];
        let target = footer.targets[0];
        assert_eq!((target.1, colour), (run(Command::PickColour), RED));
        let bar = footer.frame;
        assert!((mark.y + mark.height / 2.0 - (bar.y + bar.height / 2.0)).abs() < 1e-9);
        assert!((mark.x + mark.width / 2.0 - (target.0.x + target.0.width / 2.0)).abs() < 1e-9);
        (mark.width, mark.height)
    };
    assert_eq!(swatch(2.0), (SWATCH, 2.0));
    assert_eq!(swatch(0.5), (SWATCH, 1.0), "at least a point");
    assert_eq!(swatch(16.0), (SWATCH, SWATCH), "at most a square");
    let footer = canvas(PANE, &keys, pen(Tool::Pen, State::Idle), false);
    let bars: Vec<f64> = footer.marks[1..].iter().map(|(m, _)| m.height).collect();
    assert_eq!(bars, [1.0, 2.0, 3.0], "the width button's bars thicken");
    assert!(
        footer.marks[1..]
            .iter()
            .all(|(m, c)| *c == T.muted && footer.targets[1].0.contains(Point { x: m.x, y: m.y }))
    );
}

#[test]
fn the_state_follows_the_tool_and_an_unbound_key_says_nothing() {
    let mut keys = Keymap::default();
    let typing = canvas(PANE, &keys, pen(Tool::Text, State::Typing), false);
    let text_key = keys.chord(Command::ToolText).map(Chord::glyphs).unwrap_or_default();
    assert_eq!(said(&typing)[..2], [format!("Text {text_key}"), "· typing".into()]);
    keys.bind(Command::ToolLasso, None);
    keys.bind(Command::ShowKeys, None);
    let selected = canvas(PANE, &keys, pen(Tool::Lasso, State::Idle), false);
    assert_eq!(said(&selected)[0], "Lasso");
    assert!(!taps(&selected).contains(&run(Command::ShowKeys)), "an unbound key has no button");
    assert_eq!(taps(&selected).last(), Some(&run(Command::ShowCommands)));
}

#[test]
fn a_narrow_pane_drops_whole_groups_the_least_needed_first() {
    let keys = Keymap::default();
    let mut gone = Vec::new();
    let mut before = taps(&canvas(PANE, &keys, pen(Tool::Arrow, State::Idle), true));
    for width in (0..=100).rev().map(|n| f64::from(n) * 10.0) {
        let footer = canvas(Rect { width, ..PANE }, &keys, pen(Tool::Arrow, State::Idle), true);
        let now = taps(&footer);
        assert!(now.iter().all(|t| before.contains(t)), "nothing comes back as it narrows");
        gone.extend(before.iter().filter(|t| !now.contains(t)).copied());
        before = now;
    }
    assert_eq!(
        gone,
        [
            run(Command::ZoomOut),
            run(Command::ZoomReset),
            run(Command::ZoomIn),
            run(Command::SplitRight),
            run(Command::SplitDown),
            run(Command::ClosePane),
            run(Command::PickWidth),
            run(Command::ShowKeys),
            run(Command::Undo),
            run(Command::Redo),
            run(Command::PreviousPage),
            Tap::Pages,
            run(Command::NextPage),
            run(Command::ShowCommands),
        ]
    );
    assert_eq!(
        before,
        [run(Command::ToggleSidebar), run(Command::PickColour), run(Command::PickTool)],
        "the sidebar's button and the pen stay, cut off at the pane"
    );
    let half = canvas(Rect { width: 380.0, ..PANE }, &keys, pen(Tool::Pen, State::Idle), false);
    let tapped = taps(&half);
    assert!(
        tapped.contains(&Tap::Pages) && tapped.contains(&run(Command::ShowCommands)),
        "{tapped:?}"
    );
}

#[test]
fn an_agent_ui_pane_s_footer_has_its_name_the_pane_buttons_and_the_keys() {
    let keys = Keymap::default();
    let shows = Shows {
        pen: None,
        centre: "board",
        pages: false,
        zoom: None,
        sidebar: false,
        close: false,
        sling: false,
        agent: None,
    };
    let footer = footer(PANE, &T, &keys, &shows);
    assert_eq!(footer.marks, [], "no swatch, no width bars");
    assert_eq!(said(&footer)[0], "board");
    assert_eq!(
        taps(&footer),
        [
            run(Command::SplitRight),
            run(Command::SplitDown),
            run(Command::ShowKeys),
            run(Command::ShowCommands)
        ],
        "no close with one pane, and the name is no button"
    );
}

#[test]
fn the_open_sidebar_s_foot_hides_it_where_the_collapsed_one_s_button_shows() {
    let sidebar = Rect { x: 0.0, y: 0.0, width: 199.0, height: 400.0 };
    let strip = strip(sidebar, &T);
    assert_eq!(strip.frame, Rect { y: 400.0 - HEIGHT, height: HEIGHT, ..sidebar });
    assert_eq!(said(&strip), ["◧  Hide sidebar"]);
    let [(hide, tap)] = strip.targets[..] else { panic!() };
    assert_eq!(tap, run(Command::ToggleSidebar));
    let pane = Rect { x: 0.0, ..PANE };
    let footer = canvas(pane, &Keymap::default(), pen(Tool::Pen, State::Idle), true);
    let show = footer.targets[0].0;
    assert_eq!((show.x, show.y), (hide.x, hide.y), "one place for the button, open or not");
}

#[test]
fn the_lit_button_gets_its_fill_and_its_words_and_marks_brighten() {
    let mut footers = vec![canvas(PANE, &Keymap::default(), pen(Tool::Pen, State::Idle), false)];
    let fill = style::hover(&T);
    let width = footers[0].targets[1].0;
    assert!(light(&mut footers, (width, fill), &T));
    assert_eq!(footers[0].lit, Some((width.outset(-LIT_INSET), fill)));
    assert!(footers[0].marks[1..].iter().all(|(_, c)| *c == T.text), "the width's bars brighten");
    let undo = footers[0].targets[3].0;
    let mut footers = vec![canvas(PANE, &Keymap::default(), pen(Tool::Pen, State::Idle), false)];
    assert!(light(&mut footers, (undo, fill), &T));
    let lit: Vec<&str> = footers[0]
        .parts
        .iter()
        .filter(|p| p.runs[0].colour == T.text)
        .map(|p| p.runs[0].text.as_str())
        .collect();
    assert_eq!(lit, ["Pen", "↶"], "only the lit button's glyph brightens");
    let elsewhere = Rect { x: 0.0, y: 0.0, width: 5.0, height: 5.0 };
    assert!(!light(&mut footers, (elsewhere, fill), &T), "a sidebar row is no footer's");
}

#[test]
fn the_pane_shows_its_content_above_the_footer() {
    assert_eq!(content(PANE), Rect { height: 400.0 - HEIGHT, ..PANE });
    let short = Rect { height: 10.0, ..PANE };
    assert_eq!(content(short).height, 0.0);
    let shows = Shows {
        pen: None,
        centre: "",
        pages: false,
        zoom: None,
        sidebar: false,
        close: false,
        sling: false,
        agent: None,
    };
    let footer = footer(short, &T, &Keymap::default(), &shows);
    assert_eq!(footer.frame, short, "a pane shorter than a footer is all footer");
}

#[test]
fn each_tool_is_named_by_its_command() {
    let tools = [Tool::Pen, Tool::Eraser, Tool::Lasso, Tool::Text, Tool::Box, Tool::Arrow];
    let names: Vec<&str> = tools.iter().map(|&t| command(t).title()).collect();
    assert_eq!(names, ["Pen", "Eraser", "Lasso", "Text", "Box", "Arrow"]);
}

/// A canvas pane's footer with the sling's button, to `agent`.
fn slinging(width: f64, agent: Option<(&str, Option<Status>)>) -> Footer {
    let shows = Shows {
        pen: Some(pen(Tool::Lasso, State::Selected(3))),
        centre: "ideas · 2/3",
        pages: true,
        zoom: Some(100),
        sidebar: false,
        close: true,
        sling: true,
        agent,
    };
    footer(Rect { width, ..PANE }, &T, &Keymap::default(), &shows)
}

/// The sling button's run.
fn sling_run(footer: &Footer) -> &Run {
    let parts = footer.parts.iter().flat_map(|p| &p.runs);
    parts.into_iter().find(|r| r.text == SLING).expect("a sling button")
}

#[test]
fn a_canvas_footer_names_its_agent_and_slings_in_one_tap() {
    let footer = slinging(1200.0, Some(("scout", Some(Status::Working))));
    let said = said(&footer);
    let agent = said.iter().position(|s| s == "● scout · working").expect("the agent");
    assert_eq!(said[agent + 1], SLING, "the button beside it");
    assert!(taps(&footer).contains(&run(Command::SlingNow)));
    let button = sling_run(&footer);
    assert_eq!((button.colour, button.bold), (T.focus, true), "it can sling");

    let quiet = slinging(1200.0, Some(("scout", None)));
    assert!(said_has(&quiet, "○ scout"), "{:?}", said_of(&quiet));
    let stopped = slinging(1200.0, Some(("scout", Some(Status::Stopped))));
    assert!(said_has(&stopped, "○ scout · not running"));
    assert_eq!(sling_run(&stopped).colour, T.muted, "nothing takes a sling");
    let alone = slinging(1200.0, None);
    assert_eq!(sling_run(&alone).colour, T.muted, "no agent");
    assert!(!said_of(&alone).iter().any(|s| s.contains("scout")));
}

#[test]
fn a_narrow_canvas_footer_keeps_the_sling_button() {
    let footer = slinging(330.0, Some(("scout", Some(Status::Done))));
    assert!(taps(&footer).contains(&run(Command::SlingNow)), "{:?}", said_of(&footer));
    assert!(!said_has(&footer, "● scout · done"), "the agent goes before it");
}

fn said_of(footer: &Footer) -> Vec<String> {
    said(footer)
}

fn said_has(footer: &Footer, text: &str) -> bool {
    said(footer).iter().any(|s| s == text)
}
