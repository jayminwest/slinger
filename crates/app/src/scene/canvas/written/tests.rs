use super::{Run, runs};
use crate::scene::canvas::markdown::Look;

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
