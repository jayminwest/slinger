---
name: slinger
description: Operate the user's slinger canvas app through the `slinger` CLI. Read what the user drew or wrote on a canvas, answer on the canvas with text, boxes, arrows and ink, draw a diagram, plot, table or worksheet as one figure, show an HTML UI in a pane, and manage workspaces, panes and agent sessions. Use when a task names slinger, a slinger canvas, workspace, pane or sling, or the daemon slingerd.
---

# slinger

Slinger is the user's canvas app: the user writes with a pen on a canvas and
agents answer with items on that canvas, not with chat. You use it through
the `slinger` CLI.

Run this first, and follow what it prints:

```bash
slinger guide
```

The guide comes from the installed `slinger` binary, so it always fits the
commands that binary has. Do not work from memory of an older guide. For one
command, run `slinger <command> --help`.

If you cannot run the guide, these rules hold:

- `slinger canvas list` names the canvases. `slinger canvas read <CANVAS>`
  prints what the user put on one, with the directory of its images.
- Answer on the canvas: `slinger canvas put <CANVAS> --beside <ITEM> --text
  "..." --agent <NAME>`. Leave out `--agent` when `SLINGER_AGENT` is set.
- An answer that is a picture (a diagram, a plot, a table, a worksheet, or
  marks over the user's work) is one figure: write a drawing script (the
  guide's "Drawing scripts" has its API) and run `slinger canvas draw
  <CANVAS> <FILE> --name <FIGURE> --beside <ITEM>`. Change it later with
  `slinger canvas redraw <CANVAS> <FIGURE> --data <JSON>`, by its name. A script
  that fails exits 5 with code `script` (its line, source line and calls) or,
  over a limit, `limit`: fix it and try `--dry-run`. A short remark stays a
  `canvas put`.
- Prefer a figure to a long text. Steps in an order, parts and how they
  join, a comparison, numbers or a layout are a picture: draw them with the
  script API's plots, tables, grids, arrows and fills, not rows of words.
- Change only the items you made. Never change the user's ink or text: reply
  beside it, in short texts (at most 12 rows each; longer exits 5,
  `too_long`).
- Exit code 6 means no daemon `slingerd` answers. Tell the user (on a Mac,
  opening slinger.app starts it); do not start it unless the user asks.
- If `slinger` is not installed: `cargo install --locked --git
  https://github.com/jayminwest/slinger slinger`.
