# Figures: acceptance, 2026-11-24

The acceptance list of plan pl-9f74c2 ("Figures: an agent draws with code"),
run for real on 2026-11-24 (seed slinger-9dcf50): a scratch daemon
(`--socket`, `--data-dir`, `--scratch`), the CLI, and a driven window
(`slinger-app --drive`, `slinger drive`) at 1160 by 1480 points. The
script is the worksheet of docs/design/figures.md, Appendix B
(`crates/script/src/tests/worksheet.{js,json}`), drawn by `agent:scout`
with `slinger canvas draw ws worksheet.js --name worksheet --at 20,20
--data @worksheet.json`.

| # | Item | Result | Evidence |
|---|------|--------|----------|
| 1 | One script makes the worksheet: 56 items, 15 parts, fills, dashed and dotted areas, the same in the app and the PNG | met; met by test `tests::worksheet::the_worksheet_from_one_script_takes_the_pen_and_is_redrawn_by_name` (crates/app) | The draw replied 56 items, 15 parts, frame 900x1390. `drive scene` listed all 56 `locked`, 4 `dashed`, 2 `dotted`, 6 `filled #8a5cf533` (the header cells). `drive shot` and `canvas crop --box 0,0,9400,14300` show the same layout, the same dashes and dots and the same header tint. The test checks every item's dash and fill against the outline. The look on a real screen is a hand check (docs/hand-check.md). |
| 2 | The user writes over it: the eraser, lasso and text tool pass over the scaffold; the packet says over=figure/part | met; met by the same test and `tests::figure::the_pen_eraser_and_lasso_pass_over_a_figure_and_the_outline_says_it_is_locked` | Letter-sized pen strokes in 1a, 1b and 2 read back as `c61 ink … over=worksheet/1a-answer`, `c65 … over=worksheet/1b-answer` and `c68 … over=worksheet/2-working`. An eraser stroke across 1b's dashed edge took one ink stroke and no figure item. A lasso round the whole sheet selected the 11 ink strokes only. The text tool tapped in 1b and on the locked question text each made a new user text; all 56 figure items were unchanged. Pen feel is a hand check. |
| 3 | `canvas redraw` with a data patch fills the agent areas by name; every pen stroke unchanged; the same redraw again writes no op | met; met by the same test and `tests::redraw::a_redraw_patches_the_data_in_place_and_the_same_again_writes_nothing` (crates/daemon) | The patch `{"actual":["C","A","E","D","B"],"feedback":"1a is right."}` wrote seq 80-86 (6 new texts, the check circle's fill), and the window showed them. The 13 user items were byte-identical before and after. The same redraw again replied `seq: []`. The agent's undo took the redraw back, and a second undo took the whole figure away in one step, leaving the ink; redo brought it back. |
| 4 | A runaway script stops within its limit with an error that names the line; the daemon stays up | met; met by test `tests::draw::a_runaway_script_stops_at_its_limit_and_the_daemon_draws_on` (crates/daemon) and crates/script/src/tests/limits.rs | Through `canvas draw`: `while (true) {}` gave `script:1:1: the script ran longer than 1 s` after 1.03 s; a memory hog gave `script:2:40: the script used more than 32 MB` in 0.13 s; endless recursion gave `script:1:29: the script recursed too deep (more than 512 KB of stack)`. Each exited 5 and wrote nothing, and the daemon kept serving. The loop's line is 1:1, not 2 (slinger-71c6cb, open). |
| 5 | Five small-model scripts from the guide alone | not this step | Done in step 12 (slinger-b0b24a); docs/design/figures.md, "Fluency". |
| 6 | A canvas file with the new fields opens in a build from before them with no item lost | met by a real old build | A build of 43c92855 (the parent of 30615b6, the first figures commit) from `git archive`, with its own target directory. Its daemon read the worksheet canvas after the redraw and the user's ink: 75 items, the same ids, kinds, places, sizes, points and texts as the new build; its packet and crop drew them. It dropped fill, dash, heads, closed, locked, fig and part, as designed. It then added a stroke, and the new build read that file back with all 75 styled and locked items kept, plus the new stroke. `crates/core/tests/canvas_style.rs` and `canvas_figure.rs` hold the old-reader rule as tests. |
| 7 | `scripts/e2e-sling.sh` has a sling that comes back as a figure | needs the owner | It spends tokens and needs a login; not run here. |
| 8 | crates/script linked only by the daemon by a layer rule; cargo deny passes; slingerd builds for all four targets | partly met | The layer rule and `cargo deny` run in `cargo xtask check-all`. Only aarch64-apple-darwin was built (slinger-ca3cfe, open). |
| 9 | `cargo xtask check-all` is green | met | On the commit that adds this file. |

## Found on the way

- In the worksheet's table, the balance row has 5 cells under a header of 6.
  The script gives `'balance'` and the year-0 value, then only 3 blanks, not 4
  (Appendix B and the fixture). Fixing it changes the 56-item count that the
  tests and this table pin.
- Four big scribbles in four areas, drawn within a second, joined into one
  cluster (`over=worksheet/2-working`). The cluster's size unit comes from
  the strokes themselves, so marks 60 units long may sit 180 units apart.
  Letter-sized marks cluster per area as expected. This is the cluster rule,
  not figures.
- A redraw needs the figure's `figure_drawn` event, which lives in the
  record, not the canvas file. A canvas file copied to another data
  directory keeps its figure's items but answers `not_found` to `canvas
  redraw` until the figure is drawn again.
- A redraw that writes lists `parts` in script order. One that writes
  nothing lists them in canvas order, so `check` and `feedback` swap.
