use super::{Look, Shown, line};

/// Each character's look as one letter: `.` plain, `d` a dimmed mark,
/// `b` bold, `i` italic, `x` both, `c` code, `k` a dimmed code mark,
/// `|` a quote's bar, `_` blank, `o` and `v` a checkbox, open and ticked.
fn said(text: &str, fenced: bool) -> (String, bool) {
    let chars: Vec<char> = text.chars().collect();
    let (looks, open) = line(&chars, fenced);
    let letter = |l: &Look| match (l.shown, l.code, l.dim, l.bold, l.italic) {
        (Shown::Bar, ..) => '|',
        (Shown::Blank, ..) => '_',
        (Shown::Check(false), ..) => 'o',
        (Shown::Check(true), ..) => 'v',
        (_, true, true, ..) => 'k',
        (_, true, false, ..) => 'c',
        (_, _, true, ..) => 'd',
        (_, _, _, true, true) => 'x',
        (_, _, _, true, false) => 'b',
        (_, _, _, false, true) => 'i',
        _ => '.',
    };
    (looks.iter().map(letter).collect(), open)
}

fn looks(text: &str) -> String {
    let (looks, open) = said(text, false);
    assert!(!open, "no fence in {text:?}");
    assert_eq!(looks.chars().count(), text.chars().count(), "a look a character");
    looks
}

#[test]
fn a_heading_is_bold_its_hashes_dimmed() {
    assert_eq!(looks("## Plan"), "ddbbbbb");
    assert_eq!(looks("#no"), "...", "no space: no heading");
    assert_eq!(looks("####### seven"), ".............", "six at most");
}

#[test]
fn emphasis_and_strong_dim_their_marks() {
    assert_eq!(looks("a *b* c"), "..did..");
    assert_eq!(looks("**go** now"), "ddbbdd....");
    assert_eq!(looks("***yes***"), "dddxxxddd");
    assert_eq!(looks("**a *b* c**"), "ddbbdxdbbdd");
    assert_eq!(looks("_it_"), "diid");
}

#[test]
fn a_lone_or_spaced_star_and_a_snake_case_word_stay_as_typed() {
    assert_eq!(looks("2 * 3 * 4"), ".........");
    assert_eq!(looks("snake_case_name"), "...............");
    assert_eq!(looks("*open"), ".....");
}

#[test]
fn a_code_span_sits_on_a_tint_and_keeps_stars_plain() {
    assert_eq!(looks("run `a*b*` now"), "....kcccck....");
    assert_eq!(looks("``x`y``"), "kkccckk");
    assert_eq!(looks("`open"), ".....", "unclosed: as typed");
}

#[test]
fn list_marks_dim_and_checkboxes_show_a_box() {
    assert_eq!(looks("- one"), "d....");
    assert_eq!(looks("12. two"), "ddd....");
    assert_eq!(looks("- [ ] todo"), "d._o_.....");
    assert_eq!(looks("* [x] done"), "d._v_.....");
    assert_eq!(looks("-no"), "...");
}

#[test]
fn a_quote_draws_a_bar_in_its_mark_s_cell() {
    assert_eq!(looks("> said"), "|.....");
    assert_eq!(looks("> > **b**"), "|.|.ddbdd");
    assert_eq!(looks("> # h"), "|.dbb");
}

#[test]
fn a_fenced_block_is_code_to_its_closing_fence() {
    assert_eq!(said("```rust", false), ("kkkkkkk".into(), true));
    assert_eq!(said("let *a* = 1;", true), ("cccccccccccc".into(), true));
    assert_eq!(said("```", true), ("kkk".into(), false));
    assert_eq!(said("~~~", false), ("kkk".into(), true));
}

#[test]
fn an_escaped_mark_is_dimmed_and_opens_nothing() {
    assert_eq!(looks("\\*not\\*"), "d....d.");
}
