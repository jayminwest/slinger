use super::*;
use crate::commands::{CMD, Chord};

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 255, g: 255, b: 255 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 600.0, height: 400.0 };

fn pen(tool: Tool, state: State) -> Pen {
    Pen { tool, colour: RED, width: 16.0, state }
}

/// Each part's text, its runs joined.
fn said(footer: &Footer) -> Vec<(Align, String)> {
    let text = |p: &Part| p.runs.iter().map(|r| r.text.as_str()).collect();
    footer.parts.iter().map(|p| (p.align, text(p))).collect()
}

#[test]
fn a_canvas_pane_s_footer_says_the_pen_its_page_and_the_two_keys() {
    let keys = Keymap::default();
    let footer = footer(PANE, &T, &keys, Some(pen(Tool::Pen, State::Idle)), "ideas · 2/3");
    let pen_key = keys.chord(Command::ToolPen).map(Chord::glyphs).unwrap_or_default();
    let (overlay, palette) = (Chord::new(CMD, '/').glyphs(), Chord::new(CMD, 'k').glyphs());
    assert_eq!(
        said(&footer),
        [
            (Align::Left, format!("Pen {pen_key}")),
            (Align::Centre, "ideas · 2/3".to_string()),
            (Align::Right, format!("{overlay} keys · {palette} commands")),
        ]
    );
    let name = &footer.parts[0].runs[0];
    assert_eq!((name.colour, name.bold), (T.text, true), "the tool is bold in the text colour");
    assert!(footer.parts[1..].iter().flat_map(|p| &p.runs).all(|r| r.colour == T.muted));
    assert_eq!(footer.frame, Rect { y: 400.0 - HEIGHT, height: HEIGHT, ..PANE });
    assert_eq!(footer.fill, sidebar::lighter(&T));
    assert_ne!(footer.fill, T.background, "the footer is lighter than the pane");
    let (swatch, colour) = footer.swatch.unwrap();
    assert_eq!((swatch.width, swatch.height, colour), (SWATCH, SWATCH, RED));
    assert!(swatch.x > PANE.x && swatch.x + swatch.width < footer.parts[0].frame.x);
    assert!(footer.frame.contains(crate::layout::Point { x: swatch.x, y: swatch.y }));
}

#[test]
fn the_swatch_is_a_line_as_thick_as_the_pen_up_to_a_square_and_centred() {
    let keys = Keymap::default();
    let high = |width: f64| {
        let pen = Pen { width, ..pen(Tool::Pen, State::Idle) };
        let (swatch, _) = footer(PANE, &T, &keys, Some(pen), "").swatch.unwrap();
        let bar = footer(PANE, &T, &keys, Some(pen), "").frame;
        assert!((swatch.y + swatch.height / 2.0 - (bar.y + bar.height / 2.0)).abs() < 1e-9);
        (swatch.width, swatch.height)
    };
    assert_eq!(high(2.0), (SWATCH, 2.0));
    assert_eq!(high(4.0), (SWATCH, 4.0));
    assert_eq!(high(0.5), (SWATCH, 1.0), "at least a point");
    assert_eq!(high(16.0), (SWATCH, SWATCH), "at most a square");
}

#[test]
fn the_state_follows_the_tool_and_an_unbound_key_says_nothing() {
    let mut keys = Keymap::default();
    let typing = footer(PANE, &T, &keys, Some(pen(Tool::Text, State::Typing)), "");
    let text_key = keys.chord(Command::ToolText).map(Chord::glyphs).unwrap_or_default();
    assert_eq!(said(&typing)[0], (Align::Left, format!("Text {text_key} · typing")));

    keys.bind(Command::ToolLasso, None);
    keys.bind(Command::ShowKeys, None);
    let selected = footer(PANE, &T, &keys, Some(pen(Tool::Lasso, State::Selected(3))), "p");
    let palette = Chord::new(CMD, 'k').glyphs();
    assert_eq!(
        said(&selected),
        [
            (Align::Left, "Lasso · 3 selected".to_string()),
            (Align::Centre, "p".to_string()),
            (Align::Right, format!("{palette} commands")),
        ]
    );
    keys.bind(Command::ShowCommands, None);
    let bare = footer(PANE, &T, &keys, Some(pen(Tool::Box, State::Idle)), "");
    assert_eq!(said(&bare).len(), 1, "no page yet and no hints: the tool alone");
}

#[test]
fn an_agent_ui_pane_s_footer_has_no_tool_part() {
    let footer = footer(PANE, &T, &Keymap::default(), None, "board");
    assert_eq!(footer.swatch, None);
    let aligns: Vec<Align> = footer.parts.iter().map(|p| p.align).collect();
    assert_eq!(aligns, [Align::Centre, Align::Right]);
    assert_eq!(said(&footer)[0].1, "board");
}

#[test]
fn a_narrow_pane_drops_the_right_part_then_the_middle() {
    let keys = Keymap::default();
    let shown = |width| {
        let frame = Rect { width, ..PANE };
        let footer = footer(frame, &T, &keys, Some(pen(Tool::Arrow, State::Idle)), "Page 1 · 1/1");
        footer.parts.iter().map(|p| p.align).collect::<Vec<_>>()
    };
    assert_eq!(shown(600.0), [Align::Left, Align::Centre, Align::Right]);
    assert_eq!(shown(400.0), [Align::Left, Align::Centre]);
    assert_eq!(shown(200.0), [Align::Left]);
    assert_eq!(shown(0.0), [Align::Left], "the tool part stays, cut off at the pane");
}

#[test]
fn the_pane_shows_its_content_above_the_footer() {
    assert_eq!(content(PANE), Rect { height: 400.0 - HEIGHT, ..PANE });
    let short = Rect { height: 10.0, ..PANE };
    assert_eq!(content(short).height, 0.0);
    let footer = footer(short, &T, &Keymap::default(), None, "");
    assert_eq!(footer.frame, short, "a pane shorter than a footer is all footer");
}

#[test]
fn each_tool_is_named_by_its_command() {
    let tools = [Tool::Pen, Tool::Eraser, Tool::Lasso, Tool::Text, Tool::Box, Tool::Arrow];
    let names: Vec<&str> = tools.iter().map(|&t| command(t).title()).collect();
    assert_eq!(names, ["Pen", "Eraser", "Lasso", "Text", "Box", "Arrow"]);
}
