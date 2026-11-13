use super::*;
use crate::canvas::{Action, Applied, Author, Canvas, End, Op, Time};

const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];

/// A 100 by 100 canvas-unit picture (35 by 35 pixels) with `shapes` drawn on it.
fn drawn(shapes: Vec<Shape>) -> Raster {
    let mut canvas = Canvas::new();
    let page = canvas.pages().next().unwrap().id();
    let mut raster = Raster::new(Rect::new(0.0, 0.0, 100.0, 100.0), 0, 0, 35, 35);
    for shape in shapes {
        let op = Op { by: Author::User, at: Time(0), action: Action::Add { page, shape } };
        let Ok(Applied::Added(id)) = canvas.apply(op) else { panic!("not added") };
        raster.item(canvas.item(id).unwrap());
    }
    raster
}

fn stroke(points: &[(f64, f64)], colour: Colour, width: f64) -> Shape {
    let points = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    Shape::Stroke { points, colour, width }
}

fn ink(raster: &Raster) -> usize {
    (0..35)
        .flat_map(|y| (0..35).map(move |x| (x, y)))
        .filter(|&(x, y)| raster.pixel(x, y) != WHITE)
        .count()
}

#[test]
fn a_stroke_is_its_colour_along_its_line_and_white_away_from_it() {
    let r = drawn(vec![stroke(&[(10.0, 50.0), (90.0, 50.0)], Colour::rgb(255, 0, 0), 6.0)]);
    assert_eq!(r.pixel(17, 17), [255, 0, 0]);
    assert_eq!(r.pixel(17, 5), WHITE);
    assert_eq!(r.pixel(1, 17), WHITE, "nothing past the stroke's end");
}

#[test]
fn a_hairline_still_draws_a_full_pixel() {
    let r = drawn(vec![stroke(&[(10.0, 50.0), (90.0, 50.0)], Colour::BLACK, 0.1)]);
    assert_eq!(r.pixel(17, 17), BLACK);
}

#[test]
fn a_one_point_stroke_is_a_dot() {
    let r = drawn(vec![stroke(&[(50.0, 50.0)], Colour::BLACK, 10.0)]);
    assert_eq!(r.pixel(17, 17), BLACK);
    assert!((9..30).contains(&ink(&r)), "{}", ink(&r));
}

#[test]
fn translucent_ink_blends_with_what_is_under_it() {
    let half = Colour { a: 128, ..Colour::BLACK };
    let r = drawn(vec![stroke(&[(10.0, 50.0), (90.0, 50.0)], half, 6.0)]);
    assert_eq!(r.pixel(17, 17), [127, 127, 127]);
    let twice = drawn(vec![
        stroke(&[(10.0, 50.0), (90.0, 50.0)], half, 6.0),
        stroke(&[(10.0, 50.0), (90.0, 50.0)], half, 6.0),
    ]);
    assert_eq!(twice.pixel(17, 17), [63, 63, 63]);
}

#[test]
fn a_stroke_that_turns_back_on_itself_is_laid_down_once() {
    let half = Colour { a: 128, ..Colour::BLACK };
    let r = drawn(vec![stroke(&[(10.0, 50.0), (90.0, 50.0), (10.0, 50.0)], half, 6.0)]);
    assert_eq!(r.pixel(17, 17), [127, 127, 127]);
}

#[test]
fn a_box_is_an_outline_with_a_white_inside() {
    let r =
        drawn(vec![Shape::Box { frame: Rect::new(20.0, 20.0, 60.0, 60.0), colour: Colour::BLACK }]);
    // The edges fall on the line between two pixels, so each pixel is half covered.
    assert_eq!(r.pixel(7, 17), [128, 128, 128], "left edge");
    assert_eq!(r.pixel(6, 17), [128, 128, 128], "left edge");
    assert_eq!(r.pixel(17, 7), [128, 128, 128], "top edge");
    assert_eq!(r.pixel(17, 17), WHITE, "inside");
    assert_eq!(r.pixel(2, 2), WHITE, "outside");
}

#[test]
fn an_arrow_has_a_head_at_its_end() {
    let (from, to) = (Point::new(10.0, 50.0), Point::new(90.0, 50.0));
    let shape = Shape::Arrow { from: End::free(from), to: End::free(to), colour: Colour::BLACK };
    let r = drawn(vec![shape]);
    assert_eq!(r.pixel(17, 17), BLACK, "shaft");
    // The head's sides reach about 8 units above and below the shaft, 14 back from the tip.
    assert_ne!(r.pixel(27, 15), WHITE, "upper side of the head");
    assert_ne!(r.pixel(27, 19), WHITE, "lower side of the head");
    assert_eq!(r.pixel(5, 15), WHITE, "no head at the start");
}

#[test]
fn an_arrow_with_no_length_is_a_dot() {
    let at = End::free(Point::new(50.0, 50.0));
    let r = drawn(vec![Shape::Arrow { from: at, to: at, colour: Colour::BLACK }]);
    assert!((1..10).contains(&ink(&r)));
}

#[test]
fn text_draws_glyphs_inside_its_frame() {
    let frame = Rect::new(10.0, 10.0, 80.0, 30.0);
    let r =
        drawn(vec![Shape::Text { text: "Hi".into(), frame, colour: Colour::BLACK, size: 24.0 }]);
    assert!(ink(&r) > 10, "{}", ink(&r));
    assert_eq!(r.pixel(30, 30), WHITE, "below the line");
    let blank =
        drawn(vec![Shape::Text { text: "  ".into(), frame, colour: Colour::BLACK, size: 24.0 }]);
    assert_eq!(ink(&blank), 0);
}

#[test]
fn text_draws_at_its_own_size() {
    let frame = Rect::new(0.0, 0.0, 0.0, 0.0);
    let text = |size| Shape::Text { text: "H".into(), frame, colour: Colour::BLACK, size };
    let (small, big) = (ink(&drawn(vec![text(12.0)])), ink(&drawn(vec![text(48.0)])));
    assert!(big > small * 8, "{small} {big}");
}

#[test]
fn a_character_the_font_lacks_draws_as_an_empty_box_not_a_question_mark() {
    for c in ['\u{4e00}', '漢', '한', '😀', '🚀', '\u{10FFFF}'] {
        assert_eq!(glyph(c), glyph::TOFU, "{c:?}");
    }
    assert_ne!(glyph::TOFU, glyph('?'));
    assert_ne!(glyph('é'), glyph::TOFU);
    assert_ne!(glyph('λ'), glyph::TOFU);
}

#[test]
fn an_unknown_character_takes_one_cell_like_any_other() {
    let size = |t| crate::render::text_size(t, usize::MAX, 24.0);
    assert_eq!(size("a漢😀b"), size("abcd"));
}

#[test]
fn japanese_beyond_hiragana_draws_as_its_sound_or_its_ascii() {
    assert_eq!(glyph('カ'), glyph('か'), "katakana as the hiragana of its sound");
    assert_eq!(glyph('ヴ'), glyph('ゔ'));
    assert_eq!(glyph('Ａ'), glyph('A'), "full-width as ASCII");
    assert_eq!(glyph('～'), glyph('~'));
    assert_eq!(glyph('。'), glyph('.'));
    assert_eq!(glyph('「'), glyph('['));
    assert_eq!(glyph('\u{3000}'), glyph(' '));
    assert_eq!(glyph('ー'), glyph('—'));
}

#[test]
fn an_accented_letter_no_set_has_draws_as_its_letter() {
    let word = |w: &str| w.chars().map(glyph).collect::<Vec<_>>();
    assert_eq!(word("Łódź"), word("Lódz"));
    assert_eq!(word("Čeština Ő ſ"), word("Cestina O s"));
    assert_eq!(word("Tiếng Việt Ỹ"), word("Tieng Viet Y"));
    assert_ne!(glyph('Ỳ'), glyph('Y'), "font8x8 has its own Ỳ");
}

/// The Russian alphabet, then the Ukrainian, Belarusian and Serbian letters
/// it lacks, capitals then small letters.
const CYRILLIC: &str = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя\
                        ІЇЄҐЎЈЉЊЋЏЂіїєґўјљњћџђ";

#[test]
fn cyrillic_draws_every_letter_and_no_two_alike() {
    // slinger-caf6ba: Cyrillic drew as empty boxes; slinger-7c8678: beyond Russian too.
    let mut seen = std::collections::BTreeSet::new();
    for c in CYRILLIC.chars() {
        assert_ne!(glyph(c), glyph::TOFU, "{c:?} has no glyph");
        assert!(seen.insert(glyph(c)), "{c:?} draws like another letter");
    }
    assert_eq!(seen.len(), 66 + 22);
}

#[test]
fn a_cyrillic_letter_shaped_like_a_latin_one_draws_as_it() {
    let word = |w: &str| w.chars().map(glyph).collect::<Vec<_>>();
    assert_eq!(word("АВЕКМНОРСТХ"), word("ABEKMHOPCTX"));
    assert_eq!(word("аеорсух Ёё"), word("aeopcyx Ëë"));
    assert_eq!(word("ГПФЗ"), word("ΓΠΦ3"));
    assert_eq!(word("ІЇЈ іїј"), word("IÏJ iïj"));
    assert_ne!(glyph('И'), glyph('N'));
    assert_ne!(glyph('У'), glyph('Y'));
}

#[test]
fn cyrillic_takes_one_cell_a_letter() {
    let size = |t| crate::render::text_size(t, usize::MAX, 24.0);
    assert_eq!(size("Привет, мир"), size("Privet, mir"));
    assert_eq!(size("Їжак, Ђорђе, Љубљана"), size("Izak, Dorde, Lubjana"));
}

#[test]
fn emoji_people_type_as_marks_draw_as_the_mark() {
    assert_eq!(glyph('✅'), glyph('☑'));
    assert_eq!(glyph('❌'), glyph('✗'));
    assert_eq!(glyph('❗'), glyph('!'));
    assert_eq!(glyph('➡'), glyph('→'));
    assert_eq!(glyph('⇒'), glyph('→'));
    assert_eq!(glyph('🔴'), glyph('●'));
    assert_eq!(glyph('⭐'), glyph('★'));
}

#[test]
fn typography_people_and_agents_type_has_a_glyph() {
    let typed = "—–‐‑−‘’‚“”„′″‹›…•←→↑↓↔✓✔ é ü ñ ç ß ¿ ¡ « » ° ± × ÷ © λ π Ω ┌─┐ █ ░ あ ≤ ≥ \
                 ★☆●○■□☐☑☒✗▲▶▼◀◆♥⚠≠≈∞";
    for c in typed.chars() {
        assert_ne!(glyph(c), glyph('?'), "{c:?} (U+{:04X}) draws as `?`", u32::from(c));
        assert_ne!(glyph(c), glyph::TOFU, "{c:?} (U+{:04X}) has no glyph", u32::from(c));
    }
}

#[test]
fn dashes_are_bars_of_growing_length_and_quotes_look_like_quotes() {
    let bar = |c| glyph(c)[3].count_ones();
    assert!(bar('-') < bar('–') && bar('–') < bar('—'), "{} {} {}", bar('-'), bar('–'), bar('—'));
    assert_eq!(glyph('—')[3], 0xFF, "an em dash spans its cell, so dashes join up");
    assert_eq!(glyph('−'), glyph('-'));
    assert_eq!(glyph('’'), glyph('\''));
    assert_eq!(glyph('“'), glyph('"'));
    assert_eq!(glyph('\u{200B}'), [0; 8], "a zero-width space draws nothing");
}

#[test]
fn an_em_dash_draws_as_a_line_not_a_question_mark() {
    let frame = Rect::new(10.0, 10.0, 80.0, 30.0);
    let dash =
        drawn(vec![Shape::Text { text: "—".into(), frame, colour: Colour::BLACK, size: 24.0 }]);
    let question =
        drawn(vec![Shape::Text { text: "?".into(), frame, colour: Colour::BLACK, size: 24.0 }]);
    assert!(ink(&dash) > 0);
    assert_ne!(ink(&dash), ink(&question));
}

#[test]
fn a_shape_outside_the_tile_draws_nothing() {
    let r = drawn(vec![stroke(&[(500.0, 500.0), (600.0, 600.0)], Colour::BLACK, 6.0)]);
    assert_eq!(ink(&r), 0);
}

fn rows(text: &str, columns: usize) -> Vec<String> {
    wrap(text, columns).into_iter().map(|r| r.into_iter().collect()).collect()
}

#[test]
fn text_wraps_at_spaces_and_cuts_words_longer_than_a_line() {
    assert_eq!(rows("one two three", 7), ["one two", "three"]);
    assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(rows("a\nb c", 10), ["a", "b c"]);
    assert_eq!(rows("  a", 10), ["  a"], "spaces are kept");
    assert_eq!(rows("", 10), [""]);
    assert_eq!(rows("no wrap at all", usize::MAX), ["no wrap at all"]);
}

#[test]
fn zero_width_characters_take_no_cell() {
    // A joiner, a zero-width space, a variation selector and a BOM: none
    // draws, so none counts toward the line or the frame.
    assert_eq!(rows("ab\u{200D}cd\u{200B}ef\u{FE0F}\u{FEFF}", 6), ["abcdef"]);
    assert_eq!(rows("one\u{2060} two", 7), ["one two"]);
    assert_eq!(
        crate::render::text_size("\u{200B}\u{200B}hi", usize::MAX, 24.0),
        crate::render::text_size("hi", usize::MAX, 24.0)
    );
}

#[test]
fn a_pdf_page_is_a_grey_frame_with_nothing_drawn_inside_but_its_label() {
    let hash = format!("sha256:{}", "9f".repeat(32));
    let frame = Rect::new(10.0, 10.0, 80.0, 80.0);
    let page = (612.0, 792.0);
    let pdf = Shape::Pdf { frame, hash, pdf_page: 3, page_size: page, crop: None, name: None };
    let r = drawn(vec![pdf]);
    // The left edge, at x = 10 units, is pixel 3.5; the frame is 2 units wide.
    assert_eq!(r.pixel(3, 28), [0x99, 0x99, 0x99]);
    assert_eq!(r.pixel(17, 28), WHITE, "the page's inside is not drawn");
}
