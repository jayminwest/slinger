# Measured: slinger on a synthetic 5 MB+ canvas

slinger-6c0f10, measured 2026-11-20. These are inputs for the storage design
(slinger-7c18bc). Every number below was taken with the commands in "How to
measure again". A number that could not be taken is listed under "Not taken",
with the reason.

## The fixture

The owner's real canvases are 10-15x the largest canvas on this machine and
hold private data, so none could be copied here. A generator makes one
instead. Nothing large is committed: the same settings give the same bytes.

- `Synthetic` in `crates/core/src/canvas/file/synthetic.rs` writes the file
  through `Log`, the same code the daemon writes with. The test is in
  `crates/core/tests/canvas_synthetic.rs`.
- `crates/daemon/examples/synthetic_canvas.rs` is the command-line front end:
  `--scale N` (N times the default 120 sittings), `--sessions N`, `--seed N`,
  `--pdf BLOB`.
- The shape is 6 pages, with most of the writing on the first three.
  Handwriting is clusters of 3-30 strokes. 70% of strokes have 6-32 points,
  25% have 32-120, and 5% have 120-1500, as a sketch does. There are typed
  texts and agent texts, boxes (one in ten folded by the agent), arrows bound
  to texts and boxes, and 4 pages of a PDF on page `p4` ("paper") with ink
  over them. The churn is lasso moves, erases, text edits, and undo with and
  without redo.

| file | bytes | op lines | strokes added | points | median / p99 / max points | moves | removes | sets | restores | page 1 items |
|---|---|---|---|---|---|---|---|---|---|---|
| scale 1 (`big`) | 5 658 561 | 12 753 | 7 401 | 542 480 | 24 / 1 234 / 1 495 | 3 559 | 551 | 672 | 77 | 2 197 |
| scale 5 | 28 888 568 | 64 655 | 37 802 | 2 734 161 | 24 / 1 234 / 1 499 | 17 769 | 2 663 | 3 718 | 310 | 10 212 |
| `small` (6 sittings, seed 2) | 268 623 | 625 | 294 | 22 999 | 24 / 1 341 / 1 496 | 190 | 19 | 82 | 4 | 129 |

The counts come from `jq` over files the example wrote with the same settings; the page 1 item
counts come from the first `canvas_show`, before the script adds anything. Writing
the file takes 0.7 s at scale 1 and 12.7 s at scale 5 (release); the second
is long because a lasso move costs more as the canvas grows (below). A move
line is one moved item, so a lasso of 15 strokes writes 15 lines.
Compared with the real board in `docs/design/canvas-file.md` (7 563 strokes,
170 938 points, median 21, 99th percentile 63), scale 1 has about as many
strokes and about the same median, and a far longer tail on purpose (the seed
asks for long point arrays): per stroke it is heavier than the real board.

## Machine and load

- Apple M2 Ultra, 24 cores, 192 GB. macOS (Darwin 25.6). Release builds
  (`--release`: fat LTO, `codegen-units = 1`).
- Other agents were working in sibling worktrees at the same time. The
  script prints the load before and after the daemon runs. The tables below
  are the quietest run of each scale. Scale 1: load 3.3 then 2.8 (1 min),
  no `rustc` running. Scale 5: 2.3 then 2.1, no `rustc` running. Two more
  scale-1 runs (load 6.4 and 4.1) and one more scale-5 run (load 5.1) are in
  "Spread between runs". The `SLINGER_TRACE` rows come from the last run of
  each scale, after lane a's trace reached `main`. Compare the big and small
  rows of one run, not runs from different times.
- Each row is timed from one Unix-socket request to its reply (Python
  `time.perf_counter`), which includes the JSON encode and decode on both
  sides. A drive request that does nothing costs 0.0-0.1 ms.

## Results

Medians in ms, with the p90 after the slash. A row with one run has no p90.
`small` is the 0.27 MB canvas, on the same daemon and in the same run.

### Daemon (scratch slingerd, socket requests)

| what | small | big, scale 1 (5.7 MB) | big, scale 5 (28.9 MB) |
|---|---|---|---|
| `canvas_show`, first (opens and replays the file) | 4.1 | 104.9 | 596.2 |
| `canvas_show` p1, warm | 1.3 / 1.4 | 22.5 / 24.4 | 306.1 / 313.0 |
| `canvas_show` p4 (PDF page), warm | 0.1 | 13.5 / 14.1 | 74.2 / 110.2 |
| `sling_backdrops` p4 (ink over PDF pages) | 0.2 / 0.4 | 283.7 / 291.2 | 1306.3 / 1433.5 |
| `canvas_apply`: add one 31-point stroke (user) | 4.5 / 5.3 | 4.7 / 6.9 | 5.2 / 18.0 |
| `canvas_apply`: lasso move of the strokes just added | 5.2 / 5.7 (20 ids) | 8.4 / 9.2 (20 ids) | 18.8 / 23.2 (8 ids) |
| `canvas_read` p1, all new (no sling yet) | 34.3 | 691.0 (196 images) | 3680.7 (829 images) |
| `canvas_read` p1, only the last 2 lines new | 25.4 / 27.3 | 507.7 / 510.9 | 2383.3 / 2606.9 |
| `sling_send` p1, first (all new; starts the agent) | 45.1 | 996.9 (2 074 strokes) | 5665.1 (9 543 strokes) |
| `sling_send` p1, one new stroke | 26.1 / 26.7 | 511.8 / 515.7 | 2545.6 / 2595.7 |

Runs: 20 at scale 1 (5 for `sling_send`), and 8 at scale 5 (3 for
`sling_send`).

The sling numbers are the daemon's part only. The agent is the daemon tests'
stand-in for Claude Code (`crates/daemon/src/tests/claude`, given as
`--claude`), so a sling spends no tokens. The time runs from the request to
the daemon's reply: building the packet and backdrop, storing the blobs and
event, and writing to the agent's stdin. It does not include the agent's
turn.

### App (driven window, `slinger-app --drive`)

The window is 1440x900. A pen sample is one `drag` request with
`tablet: true` and the pen tool chosen. Its time is the event's dispatch on
the main thread, which includes the scene rebuild. It does not include
AppKit painting the frame, which happens later in the run loop and is not
timed here.

| what | small | big, scale 1 | big, scale 5 |
|---|---|---|---|
| switch: tap a sidebar row, until the window shows the new title | 978.4 / 984.4 | 1009.6 / 1016.2 | 1004.1 / 1008.0 (to big), 837.8 (to small) |
| switch: the new title until the final outline (the canvas's part) | 10.6 / 22.2 | 41.6 / 57.1 | 197.9 / 228.0 |
| pen sample, zoom as opened (marks in view: 20 / 36 / 58) | 0.1 | 0.4 / 0.4 | 1.7 / 2.4 |
| pen sample, samples 1900-2000 of one 2000-sample stroke | 0.1 | 0.4 / 0.5 | 1.6 / 10.4 |
| pen sample, zoomed out to 17% (marks in view: 27 / 104 / 460) | 0.1 | 0.6 / 0.6 | 2.6 / 3.3 |

The switch rows above are timed from outside, by polling: each round is a
tick and then a scene, about 1-40 ms apart. So they are an upper bound, by
up to one round. Switch runs: 10 at scale 1 and 4 at scale 5, for each direction. Pen: 100
samples a row. The `small` column is from the scale-1 run; the scale-5 run's
small rows agree (switch part 5.1 ms, pen 0.1 ms) but for the tap to the
title (837.8 ms), which depends on where the `Watch` poll was when the tap came.

### The switch by `SLINGER_TRACE=switch`

These come from the app's own trace line (slinger-9fd400), from the last run
of each scale. Each phase is the time since the phase before it, starting at
the pick. Lines with whole milliseconds. The runs are 10 (scale 1) and 4
(scale 5) for each direction.

| phase | to small (scale-1 run) | to big, scale 1 | to big, scale 5 |
|---|---|---|---|
| pick to `watch` (the `Watch` reads the wanted name) | 958.5 / 967 | 989.5 / 997 | 982 / 993 |
| `watch` to `layout` (the daemon's layout came; `layout` to `taken` was 0-1 ms in every run) | 1 / 2 | 1 / 1 | 2 / 3 |
| `taken` to `shown` (a `Follower`'s `canvas_show` came) | 4 / 4 | 34 / 34 | 168 / 178 |
| `shown` to `heard` (the main thread took the page) | 7.5 / 11 | 0 / 1 | 0 / 0 |
| `heard` to `drawn` (the scene was handed to the platform) | 0 / 1 | 4 / 8 | 23 / 45 |

In the scale-5 run, the small canvas's `taken` to `shown` was 7.5 ms, and
pick to `watch` was 799 ms. That depends only on where the `Watch`'s
1-second sleep was when the tap came.

### Spread between runs

The same rows from every run, as medians in ms. Scale 1: runs A, B and C,
with 1-minute loads of 3.3, 6.4 and 4.1. Scale 5: runs A and B, with loads
of 2.3 and 5.1. Run A of each scale is the one in the tables above. Run C
(scale 1) and run B (scale 5) were taken after merging `main`, with the
trace.

| row | scale 1: A / B / C | scale 5: A / B |
|---|---|---|
| `canvas_show` big, first | 104.9 / 82.5 / 83.7 | 596.2 / 592.2 |
| `canvas_show` big p1, warm | 22.5 / 26.3 / 23.4 | 306.1 / 280.7 |
| `sling_backdrops` big p4 | 283.7 / 320.6 / 325.9 | 1306.3 / 1306.6 |
| `canvas_apply` big, add a stroke | 4.7 / 14.5 / 4.5 | 5.2 / 4.5 |
| `canvas_apply` big, lasso move | 8.4 / 21.9 / 7.1 | 18.8 / 18.6 |
| `canvas_read` big, 2 lines new | 507.7 / 592.5 / 563.3 | 2383.3 / 2533.5 |
| `sling_send` big, one new stroke | 511.8 / 585.1 / 541.3 | 2545.6 / 2607.3 |
| switch to big, title to final outline | 41.6 / 42.9 / 40.7 | 197.9 / 197.1 |
| pen sample on big, zoom as opened, first 100 | 0.4 / 0.4 / 0.6 | 1.7 / 1.8 |

Most rows move by 10-20% with the load. `canvas_apply` moves by 3x at load
6.4 (run B): it waits on `fsync`, which the other lanes' builds share.

### What grows with file size

- **Opening a canvas** grows about linearly with the file: about 19 ms per MB
  at scale 1 and 21 ms per MB at scale 5.
- **A warm `canvas_show`** of one page grows faster than the page: 4.6x the
  items cost 13.6x the time (22.5 to 306 ms). It is what each `Follower`
  pays when the app (re)opens a canvas: on a switch, and for every canvas
  not shown (`crates/app/src/canvases.rs`, seed finding 3).
- **Packets and backdrops replay the whole history.** A read with only 2 new
  lines costs 508 ms at 5.7 MB and 2 383 ms at 28.9 MB. That is
  proportional to the file, not to what is new. A sling of one new stroke
  costs the same (512 and 2 546 ms). `sling_backdrops` on a page with
  nothing but 4 PDF pages and some ink costs 284 and 1 306 ms. The app asks
  for it before each sling on that sling's thread
  (`crates/app/src/backend/sling.rs`), so a sling waits for both.
  This confirms the seed's first finding at scale.
- **A lasso move** grows with the canvas: 8 ids cost 18.8 ms at scale 5,
  against 4.8 ms on the small canvas. The generator applies the same ops
  through `Log`. A `sample` profile of a debug build at scale 2 put 59% of
  its time in `Log::carried`, and 34% in the `Canvas::held` scan under it.
  `Canvas::held` filters every item, and runs for each moved or removed id. This is a finding of this measurement, not of the
  seed. Adding a stroke stays flat (about 5 ms, which is mostly the fsync).
- **The workspace switch** takes about 1 s at every size. The trace puts
  almost all of it before `watch`: the app's `Watch` asks the daemon for the
  layout once a second (`crates/app/src/backend.rs`, `EVERY`), and a pick
  waits for the next round. That is not storage. The canvas's own part is
  the `Follower`'s `canvas_show` (`taken` to `shown`), plus building the
  first scene (`heard` to `drawn`). It grows from 4 + 0 ms (small) to
  34 + 4 ms (5.7 MB) to 168 + 23 ms (28.9 MB). That agrees with the warm
  `canvas_show` rows and with the polled "title to final outline" rows.
- **A pen sample's scene rebuild** grows mostly with the page's items, less
  with the marks in view: 0.1 ms (129 items), 0.4 ms (2 197), 1.6-1.7 ms
  (10 212). Zooming out to 460 marks in view gives 2.3-2.6 ms. Within one
  stroke (a zigzag, 2 points between samples), samples 1900-2000 cost the
  same as samples 0-100. So the seed's "smoothing from point 0" cost did not
  show at this level. At 120 Hz (8.3 ms a frame), the rebuild
  takes 20-30% of the frame at scale 5, before the paint, which was not
  timed.

## Not taken

- **AppKit paint time per frame.** A driven window times the dispatch, which
  includes the scene build, not the `drawRect` that follows. Taking it needs
  Instruments or a trace inside `platform`.
- **A real Claude Code sling, end to end.** It spends tokens and needs a
  login (brief). Only the daemon's part was timed, with the stand-in agent.
- **A real tablet's sample rate and coalescing.** Coalescing is off for the
  mouse (`platform/mod.rs:375`, seed finding 2). Real hardware events come
  before the app hears them, which a driven window cannot show.
- **The owner's real canvases.** They hold private data and are not on this
  machine. Scale 5 (28.9 MB) is at the low end of 10-15x the largest
  local board. No scale above 5 was run.
- **A quiet machine.** Every run had other agents' work beside it (load
  2.1-6.4 on 24 cores). No run was taken with the machine idle.

## How to measure again

```sh
scripts/perf-canvas.py --runs 20                 # scale 1, about 2 minutes
scripts/perf-canvas.py --scale 5 --runs 8        # scale 5, about 4 minutes
scripts/perf-canvas.py --no-app                  # daemon rows only
scripts/perf-canvas.py --keep --dir DIR          # keep the scratch directory and its logs
```

The script builds the release binaries. It writes both canvases into a fresh
scratch data directory, starts a scratch `slingerd` (its own `--socket`,
`--data-dir`, `--scratch`, and the stand-in `--claude`) and a driven
`slinger-app`. It times the rows above, stops both by pid, and prints the
table. It never touches the user's daemon, data directory or config. To
write the fixture alone:

```sh
cargo run --release -p slingerd --example synthetic_canvas -- big.canvas --id big [--scale N] [--pdf BLOB]
```
