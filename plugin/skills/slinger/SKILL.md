---
name: slinger
description: Operate the user's slinger canvas app through the `slinger` CLI. Read what the user drew or wrote on a canvas, answer on the canvas with text, boxes, arrows and ink, show an HTML UI in a pane, and manage workspaces, panes and agent sessions. Use when a task names slinger, a slinger canvas, workspace, pane or sling, or the daemon slingerd.
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
- Change only the items you made. Never change the user's ink.
- Exit code 6 means no daemon `slingerd` answers. Tell the user; do not start
  it unless the user asks.
- If `slinger` is not installed: `cargo install --locked --git
  https://github.com/jayminwest/slinger slinger`.
