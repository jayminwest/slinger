//! No title and no chip is cut short: each column is as wide as its widest
//! row as the font sets it, at any window size, for any keys and any font.

use super::*;
use crate::scene::glyphs;

/// Each line of `overlay` whose text does not fit its frame, as the font
/// sets it, in a window of `size`, at every scroll.
fn cut(overlay: &mut KeyOverlay, keys: &Keymap, size: Size) -> Vec<String> {
    let mut cut = Vec::new();
    for step in 0..40 {
        overlay.scroll = f64::from(step) * ROW * 3.0;
        let Some(shown) = overlay.shown(keys, &T, size) else { return cut };
        for line in shown.lines.iter().filter(|l| !l.label.text.is_empty()) {
            if line.label.frame.width + 1e-9 < place::wide(&line.label.text, line.size) {
                cut.push(format!("{:?} at {size:?}", line.label.text));
            }
        }
    }
    cut
}

/// The left edge of each column the rows fill in a window of `size`.
fn lefts(overlay: &mut KeyOverlay, keys: &Keymap, size: Size) -> usize {
    overlay.shown(keys, &T, size);
    let place = place::place(size, 0.0, &overlay.widths, false);
    let mut lefts: Vec<f64> = place.headings.iter().map(|(_, f)| f.x).collect();
    lefts.sort_by(f64::total_cmp);
    lefts.dedup();
    lefts.len()
}

const MEDIUM: Size = Size { width: 700.0, height: 600.0 };
const SIZES: [Size; 4] = [layout::SMALLEST, MEDIUM, layout::WINDOW, TALL];

#[test]
fn every_title_and_chip_shows_whole_at_every_window_size() {
    let keys = Keymap::default();
    for size in SIZES {
        assert_eq!(cut(&mut open(), &keys, size), Vec::<String>::new());
    }
}

/// About a wide font: each character 0.7 of the size across.
fn wide_font(text: &str, size: f64) -> Size {
    let longest = text.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    let n = f64::from(u32::try_from(longest).unwrap());
    Size { width: n * size * 0.7, height: size * 1.2 }
}

#[test]
fn a_wider_font_or_a_longer_key_takes_fewer_columns_not_shorter_words() {
    let mut keys = Keymap::default();
    let before = lefts(&mut open(), &keys, layout::WINDOW);
    keys.bind(Command::Quit, Some(Chord::parse("ctrl+alt+shift+cmd+forward-delete").unwrap()));
    let longer = lefts(&mut open(), &keys, layout::WINDOW);
    assert!(longer <= before);
    for size in SIZES {
        assert_eq!(cut(&mut open(), &keys, size), Vec::<String>::new(), "a longer key");
    }

    glyphs::set_measure(wide_font);
    let wider = lefts(&mut open(), &Keymap::default(), layout::WINDOW);
    assert!(wider < before, "{wider} columns, {before} before");
    for size in SIZES {
        assert_eq!(cut(&mut open(), &Keymap::default(), size), Vec::<String>::new(), "wide font");
    }
}

#[test]
fn a_tap_finds_the_row_drawn_for_the_keys_last_shown() {
    let mut keys = Keymap::default();
    keys.bind(Command::Quit, Some(Chord::parse("ctrl+alt+shift+cmd+forward-delete").unwrap()));
    let size = TALL;
    let mut overlay = open();
    let shown = overlay.shown(&keys, &T, size).unwrap();
    let title = shown.lines.iter().find(|l| l.label.text == Command::Quit.title()).unwrap();
    let f = title.label.frame;
    let at = Point { x: f.x + 4.0, y: f.y + f.height / 2.0 };
    overlay.press(at, size);
    assert_eq!(overlay.lift(at, size), Taken::Run(Command::Quit));
}
