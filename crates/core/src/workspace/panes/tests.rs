use super::*;

fn canvas(id: &str) -> Pane {
    Pane::new(Kind::Canvas, id).unwrap()
}

fn words(layout: &Layout) -> Vec<String> {
    layout.panes().iter().map(|p| p.word()).collect()
}

#[test]
fn opens_beside_a_pane_or_beside_the_whole_window() {
    let mut layout = Layout::new(canvas("main"));
    let (n, moved) = layout.open(canvas("notes"), Some(1), Axis::Row).unwrap();
    assert_eq!(n, 2);
    assert_eq!(layout.encode(), "row 0.5 canvas:main canvas:notes");
    assert_eq!(
        (moved.pane.as_str(), moved.x, moved.y, moved.width, moved.height),
        ("canvas:notes", 0.5, 0.0, 0.5, 1.0)
    );

    let board = Pane::new(Kind::Ui, "board").unwrap();
    let (n, moved) = layout.open(board, Some(1), Axis::Column).unwrap();
    assert_eq!(n, 2);
    assert_eq!(layout.encode(), "row 0.5 column 0.5 canvas:main ui:board canvas:notes");
    assert_eq!((moved.x, moved.y, moved.width, moved.height), (0.0, 0.5, 0.5, 0.5));

    let (n, moved) = layout.open(canvas("log"), None, Axis::Column).unwrap();
    assert_eq!(n, 4);
    assert_eq!(words(&layout), ["canvas:main", "ui:board", "canvas:notes", "canvas:log"]);
    assert_eq!((moved.x, moved.y, moved.width, moved.height), (0.0, 0.5, 1.0, 0.5));
}

#[test]
fn opening_beside_a_pane_that_is_not_there_is_not_found() {
    let mut layout = Layout::new(canvas("main"));
    for beside in [0, 2] {
        let err = layout.open(canvas("x"), Some(beside), Axis::Row).unwrap_err();
        assert_eq!(err.code(), "not_found", "{beside}");
    }
    assert_eq!(layout, Layout::new(canvas("main")));
}

#[test]
fn a_full_layout_opens_no_more() {
    let mut layout = Layout::new(canvas("main"));
    for _ in 1..MAX_PANES {
        layout.open(canvas("main"), Some(1), Axis::Row).unwrap();
    }
    assert_eq!(layout.open(canvas("x"), None, Axis::Row).unwrap_err().code(), "invalid");
    assert_eq!(Layout::decode(&layout.encode()), Some(layout));
}

#[test]
fn close_gives_the_space_to_the_sibling_and_keeps_the_last() {
    let mut layout = Layout::decode("row 0.6 canvas:a column 0.3 ui:b canvas:c").unwrap();
    let moved = layout.close(2).unwrap();
    assert_eq!((moved.pane.as_str(), moved.width, moved.height), ("ui:b", 0.0, 0.0));
    assert_eq!(layout.encode(), "row 0.6 canvas:a canvas:c");
    assert_eq!(layout.close(3), None, "already gone");
    assert_eq!(layout.close(0), None);
    assert!(layout.close(1).is_some());
    assert_eq!(layout.encode(), "canvas:c");
    assert_eq!(layout.close(1), None, "the last pane stays");
    assert_eq!(layout.encode(), "canvas:c");
}

#[test]
fn closing_inside_a_nested_split_finds_the_pane_by_number() {
    let text = "column 0.5 row 0.5 canvas:a canvas:b row 0.5 canvas:c canvas:d";
    let mut layout = Layout::decode(text).unwrap();
    layout.close(3).unwrap();
    assert_eq!(layout.encode(), "column 0.5 row 0.5 canvas:a canvas:b canvas:d");
    layout.close(1).unwrap();
    assert_eq!(layout.encode(), "column 0.5 canvas:b canvas:d");
}

#[test]
fn frames_tile_the_window() {
    let layout = Layout::decode("row 0.25 canvas:a column 0.5 canvas:b canvas:c").unwrap();
    let frames: Vec<_> = layout.frames().iter().map(|f| (f.x, f.y, f.width, f.height)).collect();
    assert_eq!(frames, [(0.0, 0.0, 0.25, 1.0), (0.25, 0.0, 0.75, 0.5), (0.25, 0.5, 0.75, 0.5)]);
}

#[test]
fn the_text_reads_back_as_written_and_holds_shares_in_their_limits() {
    let text = "row 0.6 canvas:main column 0.5 ui:board canvas:notes";
    assert_eq!(Layout::decode(text).unwrap().encode(), text);
    assert_eq!(Layout::decode("  canvas:a \n").unwrap().encode(), "canvas:a");
    let held = Layout::decode("column 2 canvas:a row -1 canvas:b canvas:c").unwrap();
    assert_eq!(held.encode(), "column 0.9 canvas:a row 0.1 canvas:b canvas:c");
}

#[test]
fn text_that_is_not_a_layout_is_none() {
    for text in [
        "",
        "main",
        "canvas:",
        "browser:x",
        "row 0.5 canvas:a",
        "canvas:a canvas:b",
        "row x canvas:a canvas:b",
        "row NaN canvas:a canvas:b",
        "row inf canvas:a canvas:b",
    ] {
        assert_eq!(Layout::decode(text), None, "{text:?}");
    }
    let too_many = format!("{}canvas:a", "row 0.5 canvas:a ".repeat(MAX_PANES));
    assert_eq!(Layout::decode(&too_many), None);
    let deep = format!("{}canvas:a", "row 0.5 ".repeat(MAX_PANES));
    assert_eq!(Layout::decode(&deep), None);
}

#[test]
fn a_target_has_no_space_and_is_not_empty() {
    for bad in ["", "two words", "tab\there"] {
        assert_eq!(Pane::new(Kind::Ui, bad).unwrap_err().code(), "invalid", "{bad:?}");
    }
    assert_eq!(Pane::new(Kind::Ui, "board").unwrap().word(), "ui:board");
}
