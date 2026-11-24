use super::*;
use crate::commands::CTRL;
use crate::layout::Size;

mod undo;

/// Size 10: a cell is 5 wide, so a column of 40 holds eight: "one two"
/// and the space a row breaks at.
const SIZE: f64 = 10.0;
const EIGHT: f64 = 40.0;

/// Ten points a character, a line a size tall.
fn fixed(text: &str, _size: f64) -> Size {
    let width = |l: &str| f64::from(u32::try_from(l.chars().count()).unwrap()) * 10.0;
    let widest = text.split('\n').map(width).fold(0.0, f64::max);
    Size { width: widest, height: SIZE }
}

/// `text` with the caret at the `|` in it.
fn at(text: &str) -> Editor {
    glyphs::set_measure(fixed);
    let caret = text.find('|').unwrap();
    Editor { text: text.replace('|', ""), caret, ..Editor::default() }
}

/// The text with `|` at the caret and `[`, `]` around the selection.
fn shown(e: &Editor) -> String {
    let mut s = e.text.clone();
    match e.selection() {
        Some((a, b)) => {
            s.insert(b, ']');
            s.insert(a, '[');
        }
        None => s.insert(e.caret, '|'),
    }
    s
}

/// `keys` pressed in turn, as `"alt+left"` and so on, in a column of eight cells.
fn pressed(e: &mut Editor, keys: &[&str]) -> String {
    for key in keys {
        let chord = match *key {
            "home" => Chord::new(0, HOME),
            "end" => Chord::new(0, END),
            key => Chord::parse(key).unwrap(),
        };
        assert!(e.key(chord, SIZE, EIGHT), "{key} is the text's");
    }
    shown(e)
}

#[test]
fn left_and_right_move_a_character_and_collapse_a_selection() {
    let mut e = at("ab|c");
    assert_eq!(pressed(&mut e, &["left"]), "a|bc");
    assert_eq!(pressed(&mut e, &["left", "left"]), "|abc", "stops at the start");
    assert_eq!(pressed(&mut e, &["right", "shift+right", "shift+right"]), "a[bc]");
    assert_eq!(pressed(&mut e, &["left"]), "a|bc", "to the selection's start");
    assert_eq!(pressed(&mut e, &["shift+right", "right"]), "ab|c", "to its end");
    let mut e = at("é|ü");
    assert_eq!(pressed(&mut e, &["left"]), "|éü", "by whole characters");
}

#[test]
fn alt_moves_and_deletes_by_words() {
    let mut e = at("one, two_2 three|");
    assert_eq!(pressed(&mut e, &["alt+left"]), "one, two_2 |three");
    assert_eq!(pressed(&mut e, &["alt+left"]), "one, |two_2 three", "_ and digits are in words");
    assert_eq!(pressed(&mut e, &["alt+left", "alt+left"]), "|one, two_2 three");
    assert_eq!(pressed(&mut e, &["alt+right"]), "one|, two_2 three");
    assert_eq!(pressed(&mut e, &["alt+right"]), "one, two_2| three");
    assert_eq!(pressed(&mut e, &["alt+delete"]), "one, | three");
    assert_eq!(pressed(&mut e, &["alt+forward-delete"]), "one, |", "to the next word's end");
}

#[test]
fn cmd_moves_to_the_row_s_ends_and_deletes_to_its_start() {
    // Eight cells: "one two" and "three" are rows.
    let mut e = at("one t|wo three");
    assert_eq!(pressed(&mut e, &["cmd+left"]), "|one two three");
    assert_eq!(pressed(&mut e, &["cmd+right"]), "one two| three", "before the broken space");
    assert_eq!(pressed(&mut e, &["right", "right", "cmd+delete"]), "one two |hree");
    assert_eq!(pressed(&mut e, &["cmd+delete"]), "one two|hree", "at a row's start, its break");
    let mut e = at("ab\ncd|");
    assert_eq!(pressed(&mut e, &["shift+cmd+left"]), "ab\n[cd]");
    assert_eq!(pressed(&mut e, &["cmd+up"]), "|ab\ncd", "the text's start");
    assert_eq!(pressed(&mut e, &["end", "home", "shift+cmd+down"]), "[ab\ncd]");
}

#[test]
fn up_and_down_go_through_the_rows_keeping_the_x() {
    let mut e = at("abcdef\nab\nabcdef|");
    assert_eq!(pressed(&mut e, &["left", "up"]), "abcdef\nab|\nabcdef", "the nearest x");
    assert_eq!(pressed(&mut e, &["up"]), "abcde|f\nab\nabcdef", "the x was kept");
    assert_eq!(pressed(&mut e, &["up"]), "|abcdef\nab\nabcdef", "from the first row, the start");
    assert_eq!(pressed(&mut e, &["right", "down", "down"]), "abcdef\nab\na|bcdef");
    assert_eq!(pressed(&mut e, &["down"]), "abcdef\nab\nabcdef|", "from the last, the end");
    assert_eq!(pressed(&mut e, &["shift+up"]), "abcdef\na[b\nabcdef]", "the kept x");
    // A wrapped line's rows are rows: "one two" and "three".
    let mut e = at("one two thr|ee");
    assert_eq!(pressed(&mut e, &["up"]), "one| two three");
}

#[test]
fn delete_and_forward_delete_take_the_selection_or_a_character() {
    let mut e = at("ab|cd");
    assert_eq!(pressed(&mut e, &["delete"]), "a|cd");
    assert_eq!(pressed(&mut e, &["forward-delete"]), "a|d");
    assert_eq!(pressed(&mut e, &["shift+right", "delete"]), "a|");
    assert_eq!(pressed(&mut e, &["delete", "delete"]), "|", "nothing left to delete");
}

#[test]
fn typing_and_pasting_replace_the_selection_or_go_in_at_the_caret() {
    let mut e = at("one| three");
    e.typed(" two");
    assert_eq!(shown(&e), "one two| three");
    assert_eq!(pressed(&mut e, &["shift+alt+left"]), "one [two] three");
    e.insert("2\n");
    assert_eq!(shown(&e), "one 2\n| three");
    e.typed("\r\u{7f}\u{f702}x");
    assert_eq!(shown(&e), "one 2\nx| three", "return, delete, an arrow's character, a letter");
}

#[test]
fn cmd_a_selects_all_and_cut_takes_the_selection_out() {
    let mut e = at("ab|cd");
    assert_eq!(e.cut(), None, "nothing selected");
    assert_eq!(pressed(&mut e, &["shift+left"]), "a[b]cd");
    assert_eq!(e.selected(), Some("b"));
    assert_eq!(e.cut().as_deref(), Some("b"));
    assert_eq!(shown(&e), "a|cd");
    assert_eq!(pressed(&mut e, &["cmd+a"]), "[acd]");
    assert_eq!(e.cut().as_deref(), Some("acd"));
    assert_eq!(shown(&e), "|");
}

#[test]
fn other_keys_go_on_to_the_keymap() {
    let mut e = at("ab|");
    for key in ["cmd+z", "ctrl+left", "a", "cmd+alt+left", "alt+up", "f5", "escape", "return"] {
        assert!(!e.key(Chord::parse(key).unwrap(), SIZE, EIGHT), "{key}");
    }
    assert!(
        !e.key(Chord::new(CTRL, 'z'), SIZE, EIGHT),
        "ctrl and a letter that is not an emacs key"
    );
    assert_eq!(shown(&e), "ab|");
}

#[test]
fn a_press_places_the_caret_and_a_drag_or_shift_selects() {
    let mut e = at("abc\ndef|");
    let laid = Laid::new("abc\ndef", Point { x: 100.0, y: 50.0 }, SIZE, EIGHT);
    assert_eq!(laid.index_at(Point { x: 114.0, y: 52.0 }), 1, "nearer b's start than its end");
    assert_eq!(laid.index_at(Point { x: 116.0, y: 52.0 }), 2);
    assert_eq!(laid.index_at(Point { x: 500.0, y: 65.0 }), 7, "right of a row: its end");
    assert_eq!(laid.index_at(Point { x: 0.0, y: -40.0 }), 0, "above: the first row");
    e.place(1, false);
    e.place(6, true);
    assert_eq!(shown(&e), "a[bc\nde]f");
    e.place(2, false);
    assert_eq!(shown(&e), "ab|c\ndef");
    e.place(5, true);
    assert_eq!(shown(&e), "ab[c\nd]ef", "shift runs it from the caret");
}

#[test]
fn the_caret_and_the_selection_are_drawn_in_the_rows() {
    glyphs::set_measure(fixed);
    let laid = Laid::new("one two three", Point { x: 100.0, y: 50.0 }, SIZE, EIGHT);
    let row = |start, end| glyphs::Row { start, end };
    assert_eq!(laid.rows(), [row(0, 7), row(8, 13)]);
    let rect = |x, y, width| Rect { x, y, width, height: SIZE };
    assert_eq!(laid.caret(2, 1.0), rect(120.0, 50.0, 1.0));
    assert_eq!(laid.caret(10, 1.0), rect(120.0, 60.0, 1.0), "in the second row");
    assert_eq!(laid.selection(4, 10, 2.0), [rect(140.0, 50.0, 30.0), rect(100.0, 60.0, 20.0)]);
    let empty = Laid::new("a\n\nb", Point { x: 0.0, y: 0.0 }, SIZE, EIGHT);
    assert_eq!(empty.selection(0, 4, 2.0)[1], rect(0.0, 10.0, 2.0), "an empty line shows");
}

#[test]
fn the_caret_moves_and_deletes_by_whole_grapheme_clusters() {
    // A family joined by ZWJ, an e with a combining acute, a flag.
    let (family, e_acute, flag) = ("👩\u{200d}👩\u{200d}👧", "e\u{301}", "🇯🇵");
    let mut e = at(&format!("a{family}{e_acute}{flag}|"));
    assert_eq!(pressed(&mut e, &["left"]), format!("a{family}{e_acute}|{flag}"));
    assert_eq!(pressed(&mut e, &["left"]), format!("a{family}|{e_acute}{flag}"));
    assert_eq!(pressed(&mut e, &["left"]), format!("a|{family}{e_acute}{flag}"));
    assert_eq!(pressed(&mut e, &["shift+right"]), format!("a[{family}]{e_acute}{flag}"));
    assert_eq!(pressed(&mut e, &["right", "forward-delete"]), format!("a{family}|{flag}"));
    assert_eq!(pressed(&mut e, &["delete"]), format!("a|{flag}"));
    e.typed("\u{7f}");
    assert_eq!(shown(&e), format!("|{flag}"), "a typed delete too");
    let mut e = at(&format!("caf{e_acute}| x"));
    assert_eq!(pressed(&mut e, &["alt+left", "alt+right"]), format!("caf{e_acute}| x"), "a word");
    let text = format!("{family}b");
    let laid = Laid::new(&text, Point { x: 0.0, y: 0.0 }, SIZE, 1e7);
    // The stand-in measure gives the family's five scalars 50 points.
    assert_eq!(laid.index_at(Point { x: 20.0, y: 0.0 }), 0, "a press snaps out of a cluster");
    assert_eq!(laid.index_at(Point { x: 30.0, y: 0.0 }), family.len());
}

/// `text` being typed at the origin, size 10, in `column`.
fn typing(text: &str, column: f64) -> Editing {
    glyphs::set_measure(fixed);
    Editing {
        canvas: "main".to_string(),
        page: "p1".to_string(),
        at: Point { x: 0.0, y: 0.0 },
        colour: Rgb { r: 0, g: 0, b: 0 },
        size: SIZE,
        column,
        item: None,
        editor: Editor::new(text.to_string()),
    }
}

#[test]
fn a_double_tap_selects_a_word_and_a_triple_tap_a_row() {
    let mut e = typing("one, two\n\nthree", glyphs::UNWRAPPED);
    let tap = |e: &mut Editing, x: f64, y: f64, taps: u8| {
        e.tap(Point { x, y }, false, taps);
        shown(&e.editor)
    };
    assert_eq!(tap(&mut e, 12.0, 2.0, 1), "o|ne, two\n\nthree");
    assert_eq!(tap(&mut e, 12.0, 2.0, 2), "[one], two\n\nthree");
    assert_eq!(tap(&mut e, 30.0, 2.0, 2), "[one], two\n\nthree", "at a word's end, that word");
    assert_eq!(tap(&mut e, 42.0, 2.0, 2), "one[, ]two\n\nthree", "between words, the gap");
    assert_eq!(tap(&mut e, 999.0, 2.0, 2), "one, [two]\n\nthree", "past the row, its last word");
    assert_eq!(tap(&mut e, 5.0, 12.0, 2), "one, two\n|\nthree", "an empty line: the caret");
    assert_eq!(tap(&mut e, 12.0, 22.0, 3), "one, two\n\n[three]");
    assert_eq!(tap(&mut e, 12.0, 2.0, 4), "[one, two]\n\nthree", "and on");
    let mut e = typing("one two three", EIGHT);
    assert_eq!(tap(&mut e, 12.0, 12.0, 3), "one two [three]", "a wrapped row, not the line");
}

#[test]
fn the_emacs_keys_move_and_delete() {
    let mut e = at("one two th|ree\nfour");
    assert_eq!(pressed(&mut e, &["ctrl+a"]), "|one two three\nfour", "the line, not its row");
    assert_eq!(pressed(&mut e, &["ctrl+e"]), "one two three|\nfour");
    assert_eq!(pressed(&mut e, &["ctrl+b", "ctrl+b"]), "one two thr|ee\nfour");
    assert_eq!(pressed(&mut e, &["ctrl+f"]), "one two thre|e\nfour");
    assert_eq!(pressed(&mut e, &["ctrl+n"]), "one two three\nfour|");
    assert_eq!(pressed(&mut e, &["ctrl+p", "ctrl+a"]), "|one two three\nfour");
    assert_eq!(pressed(&mut e, &["shift+ctrl+e"]), "[one two three]\nfour", "shift selects");
    assert_eq!(pressed(&mut e, &["ctrl+f", "ctrl+h", "ctrl+d"]), "one two thre|four");
}

#[test]
fn the_emacs_keys_kill_yank_open_and_transpose() {
    let mut e = at("one |two\nthree");
    assert_eq!(pressed(&mut e, &["ctrl+k"]), "one |\nthree");
    assert_eq!(pressed(&mut e, &["ctrl+k"]), "one |three", "at the line's end: the line end");
    assert_eq!(pressed(&mut e, &["ctrl+e", "ctrl+y"]), "one three\n|", "what was cut last");
    assert_eq!(pressed(&mut e, &["ctrl+o"]), "one three\n|\n");
    let mut e = at("ab|c\nde");
    assert_eq!(pressed(&mut e, &["ctrl+t"]), "acb|\nde");
    assert_eq!(pressed(&mut e, &["ctrl+t"]), "abc|\nde", "at the line's end, the two before");
    assert_eq!(pressed(&mut e, &["ctrl+f", "ctrl+t"]), "abc\n|de", "not across lines");
}
