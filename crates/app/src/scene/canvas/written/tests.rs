use std::sync::Arc;

use super::{Block, Run, written};
use crate::config::{Rgb, Theme};
use crate::scene::canvas::markdown::Look;
use crate::scene::glyphs;

/// The runs of `text`, `size` high in its `column`, in `rows`.
fn runs(text: &str, size: f64, column: f64, rows: (usize, usize)) -> Vec<Run> {
    super::runs(text, &glyphs::layout(text, size, column), rows)
}

/// The fenced blocks of `text` at size 10 in `rows`, unwrapped.
fn blocks(text: &str, rows: (usize, usize)) -> Vec<(usize, usize, usize)> {
    let blocks = written(text, (10.0, 1e7), rows, INK, &Theme::default()).blocks.clone();
    blocks.iter().map(|&Block { row, rows, cells }| (row, rows, cells)).collect()
}

const INK: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// Each run as `row,col text`, its look as `*` when bold.
fn said(text: &str, column: f64, rows: (usize, usize)) -> Vec<String> {
    let bold = |r: &Run| if r.look.bold { "*" } else { "" };
    let runs = runs(text, 10.0, column, rows);
    runs.iter().map(|r| format!("{},{} {}{}", r.row, r.col, bold(r), r.text)).collect()
}

#[test]
fn each_stored_line_and_each_wrapped_row_starts_a_row_from_its_first_cell() {
    // 5 cells a row: "cd" ends on the last cell with a space after it, so
    // it goes to the next row, as Nvim wraps it.
    let column = 5.0 * 10.0 * 0.5;
    assert_eq!(said("ab cd ef\n\nx", column, (0, usize::MAX)), ["0,0 ab", "1,0 cd ef", "3,0 x"]);
}

#[test]
fn a_run_holds_one_look_and_its_cells_are_counted_by_character() {
    let runs = runs("# Hé *a*", 10.0, 1e7, (0, 99));
    let cells: Vec<(usize, usize, &str)> =
        runs.iter().map(|r| (r.col, r.cells, r.text.as_str())).collect();
    // é is not plain: alone in its cell.
    assert_eq!(
        cells,
        [
            (0, 1, "#"),
            (1, 2, " H"),
            (3, 1, "é"),
            (4, 1, " "),
            (5, 1, "*"),
            (6, 1, "a"),
            (7, 1, "*")
        ]
    );
    assert!(runs[2].look.bold && runs[0].look.dim);
    assert_eq!(runs[0].look, Look { dim: true, ..Look::default() });
}

#[test]
fn only_the_rows_asked_for_are_given_counted_from_the_first() {
    let text = "a\n**b**\nc";
    assert_eq!(said(text, 1e7, (1, 2)), ["0,0 **", "0,2 *b", "0,3 **"]);
    assert_eq!(said(text, 1e7, (2, 3)), ["0,0 c"]);
}

#[test]
fn a_fence_carries_over_lines_and_a_zero_width_character_takes_no_cell() {
    let runs = runs("```\n*a*\n```\nb\u{200B}c", 10.0, 1e7, (0, 99));
    assert!(runs.iter().filter(|r| r.row == 1).all(|r| r.look.code && !r.look.italic));
    let last: Vec<(usize, usize, &str)> =
        runs.iter().filter(|r| r.row == 3).map(|r| (r.col, r.cells, r.text.as_str())).collect();
    assert_eq!(last, [(0, 1, "b\u{200B}"), (1, 1, "c")], "the next character is in the next cell");
}

#[test]
fn a_fenced_block_is_one_tint_as_wide_as_its_widest_row_blank_lines_too() {
    let text = "a\n```\nshort\n\na longer line\n```\nb\n~~~\nopen to the end";
    assert_eq!(blocks(text, (0, 99)), [(1, 5, 13), (7, 2, 15)]);
}

#[test]
fn a_block_cut_by_the_rows_asked_for_keeps_the_width_of_the_whole_block() {
    let text = "```\nx\na much longer line\n```";
    assert_eq!(blocks(text, (1, 2)), [(0, 1, 18)]);
    assert_eq!(blocks(text, (4, 9)), [], "no block in the rows");
}

#[test]
fn a_wrapped_code_line_counts_each_of_its_rows() {
    // 5 cells a row, wrapped as Nvim does ("ab", "cd ef", "gh"): the
    // block's widest row is 5 cells, not its 11-character line.
    let column = 5.0 * 10.0 * 0.5;
    let text = "```\nab cd ef gh\n```";
    let laid = written(text, (10.0, column), (0, 99), INK, &Theme::default());
    assert_eq!(laid.blocks, [Block { row: 0, rows: 5, cells: 5 }]);
}

#[test]
fn a_text_written_again_is_the_one_kept_and_a_change_is_laid_anew() {
    let theme = Theme::default();
    let once = written("# kept", (10.0, 1e7), (0, 99), INK, &theme);
    let again = written("# kept", (10.0, 1e7), (0, 99), INK, &theme);
    assert!(Arc::ptr_eq(&once, &again), "laid out once");
    let other = written("# kept!", (10.0, 1e7), (0, 99), INK, &theme);
    assert_eq!(other.runs.last().map(|r| r.text.as_str()), Some(" kept!"));
    let red = written("# kept", (10.0, 1e7), (0, 99), Rgb { r: 255, g: 0, b: 0 }, &theme);
    assert_ne!(red.dim, once.dim, "another ink is another entry");
}
