use super::*;

fn a(items: Vec<Value>) -> Value {
    Value::Array(items)
}

fn cell(text: &str, more: &[i64]) -> Value {
    let mut items = vec![Value::str(text)];
    items.extend(more.iter().map(|n| Value::Int(*n)));
    a(items)
}

#[test]
fn a_batch_resizes_writes_lines_scrolls_and_flushes() {
    let mut grid = Grid::new(1);
    let n = Value::Int;
    let line = |row, col, cells| a(vec![n(1), n(row), n(col), a(cells), Value::Bool(false)]);
    let batch = [
        a(vec![Value::str("grid_resize"), a(vec![n(1), n(4), n(3)])]),
        a(vec![
            Value::str("grid_line"),
            line(0, 0, vec![cell("a", &[7]), cell("b", &[]), cell(" ", &[0, 2])]),
            line(1, 1, vec![cell("漢", &[0]), cell("", &[])]),
        ]),
        a(vec![Value::str("grid_cursor_goto"), a(vec![n(1), n(1), n(1)])]),
        a(vec![Value::str("flush"), a(vec![])]),
    ];
    assert!(grid.redraw(&batch));
    assert_eq!((grid.row_text(0), grid.row_text(1)), ("ab".to_string(), " 漢".to_string()));
    assert_eq!((grid.row(0)[1].hl, grid.cursor, grid.serial), (7, (1, 1), 1));
    let up = a(vec![n(1), n(0), n(3), n(0), n(4), n(1), n(0)]);
    assert!(!grid.redraw(&[a(vec![Value::str("grid_scroll"), up])]));
    assert_eq!(grid.row_text(0), " 漢");
    let wider = a(vec![Value::str("grid_resize"), a(vec![n(1), n(6), n(2)])]);
    grid.redraw(&[wider]);
    assert_eq!((grid.cols, grid.rows, grid.row_text(0)), (6, 2, " 漢".to_string()), "kept");
}

#[test]
fn a_highlight_takes_the_default_colours_it_does_not_name() {
    let mut grid = Grid::new(1);
    let attrs = Value::Map(vec![
        (Value::str("foreground"), Value::Int(0x00ff_0000)),
        (Value::str("reverse"), Value::Bool(true)),
        (Value::str("undercurl"), Value::Bool(true)),
    ]);
    let define = a(vec![Value::Int(3), attrs, Value::Map(vec![]), a(vec![])]);
    grid.redraw(&[a(vec![Value::str("hl_attr_define"), define])]);
    let style = grid.style(3);
    assert_eq!(style.bg, Rgb { r: 255, g: 0, b: 0 });
    assert_eq!((style.fg, style.under), (grid.background(), Under::Curl));
}

#[test]
fn the_cursor_takes_its_modes_shape_and_hides_while_busy() {
    let mut grid = Grid::new(1);
    let info = |shape: &str, part: i64| {
        Value::Map(vec![
            (Value::str("cursor_shape"), Value::str(shape)),
            (Value::str("cell_percentage"), Value::Int(part)),
        ])
    };
    let modes = a(vec![Value::Bool(true), a(vec![info("block", 0), info("vertical", 25)])]);
    let change = a(vec![Value::str("insert"), Value::Int(1)]);
    grid.redraw(&[
        a(vec![Value::str("mode_info_set"), modes]),
        a(vec![Value::str("mode_change"), change]),
    ]);
    assert_eq!(
        (grid.mode_name.as_str(), grid.cursor_shape()),
        ("insert", Some(CursorShape::Vertical(0.25)))
    );
    grid.redraw(&[a(vec![Value::str("busy_start")])]);
    assert_eq!(grid.cursor_shape(), None);
}

#[test]
fn two_grids_are_the_same_picture_by_their_session_and_flush() {
    let (mut one, two) = (Grid::new(1), Grid::new(2));
    assert_ne!(one, two);
    let same = one.clone();
    assert_eq!(one, same);
    one.redraw(&[a(vec![Value::str("flush"), a(vec![])])]);
    assert_ne!(one, same, "a flush is a new picture");
}
