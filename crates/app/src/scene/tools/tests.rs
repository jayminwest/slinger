use super::*;
use crate::config::Widths;

const WITHIN: Rect = Rect { x: 0.0, y: 0.0, width: 440.0, height: 160.0 };
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

fn rgb_palette() -> Palette {
    crate::config::parse("[pen]\ncolours = \"#ff0000 #00ff00 #0000ff\"\n")
        .unwrap()
        .resolve()
        .palette
}

fn tools() -> Tools {
    Tools::new(rgb_palette(), Point { x: 220.0, y: 80.0 })
}

fn key(text: &str) -> Chord {
    Chord::parse(text).unwrap()
}

/// The centre of the open pop-up's cell `i`.
fn cell(tools: &Tools, i: usize) -> Point {
    let c = tools.popup.as_ref().unwrap().place.cells[i];
    Point { x: c.x + c.width / 2.0, y: c.y + c.height / 2.0 }
}

fn lit(tools: &Tools) -> usize {
    tools.popup.as_ref().unwrap().lit
}

#[test]
fn starts_with_the_pen_in_the_first_colour_and_no_popup() {
    let tools = tools();
    assert_eq!((tools.tool(), tools.colour(), tools.open_kind()), (Tool::Pen, RED, None));
    assert_eq!(tools.labels(&T), []);
}

#[test]
fn opens_where_the_pen_last_was() {
    let mut tools = tools();
    tools.moved(Point { x: 200.0, y: 70.0 });
    tools.open(Kind::Tool, WITHIN);
    let panel = tools.popup.as_ref().unwrap().place.panel;
    assert_eq!(panel.x + panel.width / 2.0, 200.0);
    assert_eq!(panel.y + panel.height / 2.0, 70.0);
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
}

#[test]
fn opens_inside_the_window_near_its_edge() {
    let mut tools = tools();
    tools.moved(Point { x: 439.0, y: 159.0 });
    tools.open(Kind::Tool, WITHIN);
    let panel = tools.popup.as_ref().unwrap().place.panel;
    assert!(panel.x + panel.width <= WITHIN.width && panel.y + panel.height <= WITHIN.height);
}

#[test]
fn the_pen_picks_a_cell_and_the_popup_closes() {
    let mut tools = tools();
    tools.open(Kind::Colour, WITHIN);
    assert!(tools.press(cell(&tools, 2)));
    assert_eq!((tools.colour(), tools.open_kind()), (BLUE, None));
    tools.open(Kind::Shape, WITHIN);
    assert!(tools.press(cell(&tools, 1)));
    assert_eq!((tools.tool(), tools.open_kind()), (Tool::Arrow, None));
    tools.open(Kind::Tool, WITHIN);
    assert!(tools.press(cell(&tools, 2)));
    assert_eq!((tools.tool(), tools.open_kind()), (Tool::Lasso, None));
    assert_eq!(tools.colour(), BLUE, "a tool keeps the colour");
}

#[test]
fn a_press_between_cells_does_nothing_and_one_outside_closes() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    let panel = tools.popup.as_ref().unwrap().place.panel;
    assert!(tools.press(Point { x: panel.x + 1.0, y: panel.y + 1.0 }));
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
    assert!(tools.press(Point { x: 1.0, y: 1.0 }));
    assert_eq!((tools.tool(), tools.open_kind()), (Tool::Pen, None));
    assert!(!tools.press(Point { x: 1.0, y: 1.0 }), "with none open, a press is not the pop-up's");
}

#[test]
fn a_digit_picks_its_cell() {
    let mut tools = tools();
    tools.open(Kind::Colour, WITHIN);
    assert!(tools.key(key("2")));
    assert_eq!((tools.colour(), tools.open_kind()), (GREEN, None));
    tools.open(Kind::Tool, WITHIN);
    assert!(tools.key(key("9")), "a digit past the cells is used up");
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
    assert!(tools.key(key("4")));
    assert_eq!(tools.tool(), Tool::Text);
}

#[test]
fn the_arrows_move_the_light_around_and_return_picks_it() {
    let mut tools = tools();
    tools.open(Kind::Colour, WITHIN);
    assert_eq!(lit(&tools), 0, "the current colour is lit");
    assert!(tools.key(key("left")));
    assert_eq!(lit(&tools), 2);
    assert!(tools.key(key("right")) && tools.key(key("down")));
    assert_eq!(lit(&tools), 1);
    assert!(tools.key(key("up")));
    assert_eq!(lit(&tools), 0);
    assert!(tools.key(key("right")) && tools.key(key("return")));
    assert_eq!((tools.colour(), tools.open_kind()), (GREEN, None));
    tools.open(Kind::Shape, WITHIN);
    assert!(tools.key(key("space")));
    assert_eq!(tools.tool(), Tool::Box);
}

#[test]
fn the_current_choice_is_lit_when_a_popup_opens() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    tools.key(key("3"));
    tools.open(Kind::Tool, WITHIN);
    assert_eq!(lit(&tools), 2);
    tools.key(key("escape"));
    tools.open(Kind::Shape, WITHIN);
    assert_eq!(lit(&tools), 0, "a tool that is no shape lights the first");
}

#[test]
fn escape_closes_and_changes_nothing() {
    let mut tools = tools();
    tools.open(Kind::Shape, WITHIN);
    assert!(tools.key(key("escape")));
    assert_eq!((tools.tool(), tools.open_kind()), (Tool::Pen, None));
    assert!(!tools.key(key("escape")), "with none open, a key is not the pop-up's");
}

#[test]
fn other_keys_pass_and_leave_the_popup_open() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    assert!(!tools.key(key("cmd+q")));
    assert!(!tools.key(key("a")));
    assert!(!tools.key(key("shift+1")));
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
}

#[test]
fn the_same_command_again_closes_and_another_replaces() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    tools.open(Kind::Shape, WITHIN);
    assert_eq!(tools.open_kind(), Some(Kind::Shape));
    tools.open(Kind::Shape, WITHIN);
    assert_eq!(tools.open_kind(), None);
}

#[test]
fn a_new_palette_keeps_the_colour_and_closes_a_colour_popup() {
    let mut tools = tools();
    tools.open(Kind::Colour, WITHIN);
    tools.set_palette(rgb_palette());
    assert_eq!(tools.open_kind(), Some(Kind::Colour), "the same palette changes nothing");
    tools.set_palette(Palette::default());
    assert_eq!((tools.colour(), tools.open_kind()), (RED, None));
    tools.open(Kind::Tool, WITHIN);
    tools.set_palette(rgb_palette());
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
    tools.key(key("escape"));
    tools.open(Kind::Colour, WITHIN);
    assert_eq!(tools.popup.as_ref().unwrap().place.cells.len(), 3);
}

#[test]
fn a_colour_popup_draws_a_bordered_panel_and_a_ring_round_the_lit_swatch() {
    let mut tools = tools();
    tools.open(Kind::Colour, WITHIN);
    let labels = tools.labels(&T);
    let fills: Vec<Option<Rgb>> = labels.iter().map(|l| l.fill).collect();
    assert_eq!(
        fills,
        [Some(T.muted), Some(T.background), Some(T.text), Some(RED), Some(GREEN), Some(BLUE)]
    );
    assert!(labels.iter().all(|l| l.text.is_empty()));
    let place = &tools.popup.as_ref().unwrap().place;
    assert_eq!(labels[0].frame, place.panel);
    assert_eq!(labels[2].frame, place.cells[0].outset(2.0));
    assert_eq!(labels[3].frame, place.cells[0]);
}

#[test]
fn a_word_popup_names_each_tool_and_lights_the_current_one() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    let labels = tools.labels(&T);
    let shown: Vec<(&str, Rgb, Option<Rgb>)> =
        labels[2..].iter().map(|l| (l.text.as_str(), l.colour, l.fill)).collect();
    assert_eq!(
        shown,
        [
            ("pen", T.background, Some(T.text)),
            ("eraser", T.text, None),
            ("lasso", T.text, None),
            ("text", T.text, None),
        ]
    );
    tools.open(Kind::Shape, WITHIN);
    let words: Vec<String> = tools.labels(&T)[2..].iter().map(|l| l.text.clone()).collect();
    assert_eq!(words, ["box", "arrow"]);
}

#[test]
fn a_tool_command_picks_its_tool_and_closes_a_tool_pop_up() {
    let mut tools = tools();
    tools.open(Kind::Tool, WITHIN);
    for (command, tool) in [
        (Command::ToolText, Tool::Text),
        (Command::ToolBox, Tool::Box),
        (Command::ToolArrow, Tool::Arrow),
        (Command::ToolEraser, Tool::Eraser),
        (Command::ToolLasso, Tool::Lasso),
        (Command::ToolPen, Tool::Pen),
    ] {
        assert!(tools.command(command));
        assert_eq!(tools.tool(), tool);
    }
    assert_eq!(tools.open_kind(), None);
    tools.open(Kind::Colour, WITHIN);
    assert!(tools.command(Command::ToolBox));
    assert_eq!(tools.open_kind(), Some(Kind::Colour), "a colour pop-up stays");
    assert!(!tools.command(Command::Quit));
    assert_eq!(tools.tool(), Tool::Box);
}

fn widths(value: &str) -> Widths {
    crate::config::parse(&format!("[pen]\nwidths = \"{value}\"\n")).unwrap().resolve().widths
}

fn width_text(tools: &Tools) -> String {
    tools.width().text()
}

#[test]
fn the_pen_starts_with_the_first_width() {
    assert_eq!(width_text(&tools()), "2", "the default widths' first");
    assert_eq!(width_text(&tools().with_widths(widths("3 1"))), "3");
}

#[test]
fn the_width_popup_picks_by_pen_digit_and_arrows_and_keeps_the_tool() {
    let mut tools = tools().with_widths(widths("1 3 5"));
    tools.command(Command::ToolBox);
    tools.open(Kind::Width, WITHIN);
    assert_eq!((lit(&tools), tools.popup.as_ref().unwrap().place.cells.len()), (0, 3));
    assert!(tools.press(cell(&tools, 2)));
    assert_eq!((width_text(&tools), tools.open_kind()), ("5".to_string(), None));
    tools.open(Kind::Width, WITHIN);
    assert_eq!(lit(&tools), 2, "the current width is lit");
    assert!(tools.key(key("2")));
    assert_eq!(width_text(&tools), "3");
    tools.open(Kind::Width, WITHIN);
    assert!(tools.key(key("left")) && tools.key(key("return")));
    assert_eq!(width_text(&tools), "1");
    tools.open(Kind::Width, WITHIN);
    assert!(tools.key(key("right")) && tools.key(key("escape")));
    assert_eq!((width_text(&tools), tools.open_kind()), ("1".to_string(), None));
    assert_eq!((tools.tool(), tools.colour()), (Tool::Box, RED), "a width keeps tool and colour");
}

#[test]
fn width_up_and_down_step_along_the_list_and_stop_at_its_ends() {
    let mut tools = tools().with_widths(widths("1 3 5"));
    tools.open(Kind::Width, WITHIN);
    assert!(tools.command(Command::WidthDown));
    assert_eq!(width_text(&tools), "1", "the first stays");
    assert_eq!(tools.open_kind(), None, "a width pop-up closes");
    let mut seen = Vec::new();
    for _ in 0..3 {
        assert!(tools.command(Command::WidthUp));
        seen.push(width_text(&tools));
    }
    assert_eq!(seen, ["3", "5", "5"]);
    tools.open(Kind::Colour, WITHIN);
    assert!(tools.command(Command::WidthDown));
    assert_eq!((width_text(&tools), tools.open_kind()), ("3".to_string(), Some(Kind::Colour)));
    assert_eq!(tools.tool(), Tool::Pen, "a width keeps the tool");
}

#[test]
fn new_widths_keep_the_width_and_close_a_width_popup() {
    let mut tools = tools().with_widths(widths("1 3 5"));
    tools.command(Command::WidthUp);
    tools.open(Kind::Width, WITHIN);
    tools.set_widths(widths("1 3 5"));
    assert_eq!(tools.open_kind(), Some(Kind::Width), "the same widths change nothing");
    tools.set_widths(widths("2 4"));
    assert_eq!((width_text(&tools), tools.open_kind()), ("3".to_string(), None));
    tools.command(Command::WidthUp);
    assert_eq!(width_text(&tools), "4", "a width not in the list steps from the first");
    tools.open(Kind::Tool, WITHIN);
    tools.set_widths(widths("9"));
    assert_eq!(tools.open_kind(), Some(Kind::Tool));
}

#[test]
fn a_width_popup_draws_each_width_as_a_line_in_the_pen_colour_framed_when_lit() {
    let mut tools = tools().with_widths(widths("1 4 40"));
    tools.command(Command::WidthUp);
    tools.open(Kind::Colour, WITHIN);
    tools.key(key("3"));
    tools.open(Kind::Width, WITHIN);
    let labels = tools.labels(&T);
    let fills: Vec<Option<Rgb>> = labels.iter().map(|l| l.fill).collect();
    let (m, b, t) = (Some(T.muted), Some(T.background), Some(T.text));
    assert_eq!(fills, [m, b, Some(BLUE), t, b, Some(BLUE), Some(BLUE)]);
    let cells = tools.popup.as_ref().unwrap().place.cells.clone();
    let (thin, lit, thick) = (labels[2].frame, labels[5].frame, labels[6].frame);
    assert_eq!((labels[3].frame, labels[4].frame), (cells[1], cells[1].outset(-1.0)));
    assert_eq!((thin.height, lit.height), (1.0, 4.0));
    assert_eq!(thick.height, cells[2].height - 2.0 * LINE_INSET, "a wide line fits its cell");
    for (line, cell) in [(thin, cells[0]), (lit, cells[1]), (thick, cells[2])] {
        assert!(cell.contains(Point { x: line.x, y: line.y }), "{line:?} in {cell:?}");
        assert!((line.y + line.height / 2.0 - (cell.y + cell.height / 2.0)).abs() < 1e-9);
    }
    assert!(labels.iter().all(|l| l.text.is_empty()));
}

#[test]
fn each_pick_command_opens_its_popup() {
    let opened: Vec<Option<Kind>> = [
        Command::PickColour,
        Command::PickShape,
        Command::PickTool,
        Command::PickWidth,
        Command::Quit,
    ]
    .map(Kind::opened_by)
    .to_vec();
    let kinds = [Kind::Colour, Kind::Shape, Kind::Tool, Kind::Width].map(Some);
    assert_eq!(opened[..4], kinds);
    assert_eq!(opened[4], None);
}
