use super::*;

fn canvas(id: &str) -> Pane {
    Pane::new(Kind::Canvas, id).unwrap()
}

fn words(layout: &Layout) -> Vec<String> {
    layout.panes().iter().map(|p| p.word()).collect()
}

/// The layout text without the ids, for the tests of the tree's shape.
fn plain(layout: &Layout) -> String {
    let word = |w: &str| w.rsplit_once('@').map_or(w, |(pane, _)| pane).to_string();
    layout.encode().split(' ').map(word).collect::<Vec<_>>().join(" ")
}

fn ids(layout: &Layout) -> Vec<u32> {
    layout.panes().iter().map(|p| p.id).collect()
}

#[test]
fn opens_beside_a_pane_or_beside_the_whole_window() {
    let mut layout = Layout::new(canvas("main"));
    let (n, moved) = layout.open(canvas("notes"), Some(1), Axis::Row).unwrap();
    assert_eq!(n, 2);
    assert_eq!(plain(&layout), "row 0.5 canvas:main canvas:notes");
    assert_eq!(
        (moved.pane.as_str(), moved.x, moved.y, moved.width, moved.height),
        ("canvas:notes", 0.5, 0.0, 0.5, 1.0)
    );

    let board = Pane::new(Kind::Ui, "board").unwrap();
    let (n, moved) = layout.open(board, Some(1), Axis::Column).unwrap();
    assert_eq!(n, 2);
    assert_eq!(plain(&layout), "row 0.5 column 0.5 canvas:main ui:board canvas:notes");
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
    assert_eq!(plain(&layout), "row 0.6 canvas:a canvas:c");
    assert_eq!(layout.close(3), None, "already gone");
    assert_eq!(layout.close(0), None);
    assert!(layout.close(1).is_some());
    assert_eq!(plain(&layout), "canvas:c");
    assert_eq!(layout.close(1), None, "the last pane stays");
    assert_eq!(plain(&layout), "canvas:c");
}

#[test]
fn closing_inside_a_nested_split_finds_the_pane_by_number() {
    let text = "column 0.5 row 0.5 canvas:a canvas:b row 0.5 canvas:c canvas:d";
    let mut layout = Layout::decode(text).unwrap();
    layout.close(3).unwrap();
    assert_eq!(plain(&layout), "column 0.5 row 0.5 canvas:a canvas:b canvas:d");
    layout.close(1).unwrap();
    assert_eq!(plain(&layout), "column 0.5 canvas:b canvas:d");
}

#[test]
fn resize_moves_the_border_after_a_pane() {
    let text = "row 0.5 column 0.5 canvas:a canvas:b canvas:c";
    let mut layout = Layout::decode(text).unwrap();
    // After pane 1 comes the column's border; after pane 2, the row's.
    let moved = layout.resize(1, 0.25).unwrap();
    assert_eq!(plain(&layout), "row 0.5 column 0.25 canvas:a canvas:b canvas:c");
    assert_eq!(
        (moved.pane.as_str(), moved.x, moved.y, moved.width, moved.height),
        ("canvas:a", 0.0, 0.0, 0.5, 0.25)
    );
    let moved = layout.resize(2, 0.7).unwrap();
    assert_eq!(plain(&layout), "row 0.7 column 0.25 canvas:a canvas:b canvas:c");
    assert_eq!((moved.pane.as_str(), moved.width), ("canvas:b", 0.7));
    layout.resize(2, 5.0).unwrap();
    assert_eq!(plain(&layout), "row 0.9 column 0.25 canvas:a canvas:b canvas:c", "held");
}

#[test]
fn a_resize_that_changes_nothing_is_none() {
    let text = "row 0.5 canvas:a canvas:b";
    let mut layout = Layout::decode(text).unwrap();
    for (number, ratio) in [(0, 0.3), (2, 0.3), (3, 0.3), (1, 0.5), (1, f64::NAN)] {
        assert_eq!(layout.resize(number, ratio), None, "{number} {ratio}");
    }
    assert_eq!(plain(&layout), text);
    assert_eq!(Layout::new(canvas("a")).resize(1, 0.3), None, "a lone pane has no border");
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
    assert_eq!(plain(&Layout::decode(text).unwrap()), text);
    assert_eq!(Layout::decode("  canvas:a@1 \n").unwrap().encode(), "canvas:a@1");
    let held = Layout::decode("column 2 canvas:a row -1 canvas:b canvas:c").unwrap();
    assert_eq!(plain(&held), "column 0.9 canvas:a row 0.1 canvas:b canvas:c");
}

#[test]
fn text_that_is_not_a_layout_is_none() {
    for text in [
        "",
        "main",
        "canvas:",
        "canvas:@3",
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

#[test]
fn a_pane_keeps_its_id_when_others_open_close_and_resize() {
    let mut layout = Layout::new(canvas("a"));
    assert_eq!((ids(&layout), layout.next()), (vec![1], 2));
    let given = Pane { id: 9, ..canvas("b") };
    layout.open(given, Some(1), Axis::Row).unwrap();
    assert_eq!(ids(&layout), [1, 2], "the layout gives the id");
    layout.open(canvas("c"), Some(1), Axis::Column).unwrap();
    assert_eq!(ids(&layout), [1, 3, 2]);
    layout.resize(1, 0.3).unwrap();
    assert_eq!(ids(&layout), [1, 3, 2]);
    layout.close(1).unwrap();
    assert_eq!(ids(&layout), [3, 2], "closing the first leaves the others' ids");
    assert_eq!(layout.encode(), "row 0.5 canvas:c@3 canvas:b@2");
}

#[test]
fn an_id_is_not_given_again_after_a_close_nor_after_the_text_is_read() {
    let mut layout = Layout::new(canvas("a"));
    layout.open(canvas("b"), None, Axis::Row).unwrap();
    layout.open(canvas("c"), None, Axis::Row).unwrap();
    layout.close(3).unwrap();
    assert_eq!((ids(&layout), layout.next()), (vec![1, 2], 4));
    let read = Layout::read(VERSION, &layout.encode(), Some(layout.next())).unwrap();
    assert_eq!(read, layout, "the text and next round trip");
    let mut layout = read;
    layout.open(canvas("d"), None, Axis::Row).unwrap();
    assert_eq!(ids(&layout), [1, 2, 4]);
}

#[test]
fn version_one_text_reads_with_ids_in_order_and_writes_them() {
    let text = "row 0.5 canvas:a column 0.5 ui:b canvas:c@7";
    let layout = Layout::read(1, text, None).unwrap();
    assert_eq!((ids(&layout), layout.next()), (vec![1, 2, 3], 4));
    assert_eq!(layout.panes()[2].target, "c@7", "version 1 has no ids: @ is the target's");
    assert_eq!(layout.encode(), "row 0.5 canvas:a@1 column 0.5 ui:b@2 canvas:c@7@3");
    let again = Layout::read(VERSION, &layout.encode(), Some(layout.next())).unwrap();
    assert_eq!(again, layout);
    assert_eq!(Layout::read(3, "canvas:a", None), None, "a version this module does not read");
    assert_eq!(Layout::read(0, "canvas:a", None), None);
}

#[test]
fn missing_zero_and_repeated_ids_get_fresh_ones_after_every_id_and_next() {
    let text = "column 0.5 row 0.5 canvas:a@5 canvas:b@5 row 0.5 canvas:c@0 canvas:d";
    let layout = Layout::read(VERSION, text, Some(3)).unwrap();
    assert_eq!((ids(&layout), layout.next()), (vec![5, 6, 7, 8], 9));
    let layout = Layout::read(VERSION, text, Some(20)).unwrap();
    assert_eq!((ids(&layout), layout.next()), (vec![5, 20, 21, 22], 23));
    let word = Layout::decode("canvas:a@x").unwrap();
    assert_eq!((word.panes()[0].target.as_str(), word.panes()[0].id), ("a@x", 1));
}

#[test]
fn a_missing_or_too_small_next_is_the_highest_id_plus_one() {
    let text = "row 0.5 canvas:a@4 canvas:b@2";
    for (next, want) in [(None, 5), (Some(0), 5), (Some(3), 5), (Some(5), 5), (Some(9), 9)] {
        assert_eq!(Layout::read(VERSION, text, next).unwrap().next(), want, "{next:?}");
    }
}

#[test]
fn a_target_with_an_at_sign_reads_back() {
    let mut layout = Layout::new(canvas("a@b"));
    layout.open(Pane::new(Kind::Ui, "x@2").unwrap(), None, Axis::Row).unwrap();
    assert_eq!(layout.encode(), "row 0.5 canvas:a@b@1 ui:x@2@2");
    assert_eq!(Layout::decode(&layout.encode()), Some(layout));
}

#[test]
fn past_the_last_id_a_new_pane_takes_the_least_id_no_pane_has() {
    let text = format!("row 0.5 canvas:a@1 canvas:b@{}", u32::MAX);
    let mut layout = Layout::decode(&text).unwrap();
    assert_eq!(layout.next(), u32::MAX);
    layout.open(canvas("c"), None, Axis::Row).unwrap();
    assert_eq!(ids(&layout), [1, u32::MAX, 2]);
}
