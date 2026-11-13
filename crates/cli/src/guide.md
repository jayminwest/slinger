# slinger: the guide for agents

Slinger is one window where the user thinks with a pen on a canvas and agents
do the work. The user reads the canvas, not chat. You use slinger through the
`slinger` CLI, which talks to the daemon `slingerd` on a local socket. This
guide comes from the `slinger` binary you run, so it fits that binary.

## Ideas

- **Canvas.** Pages of items: `stroke` (pen ink), `text`, `box`, `arrow`,
  `button`, `live` (an agent UI) and `pdf` (a page of a PDF the user opened
  in the app, or a part of one). A canvas has an id, such as `notes`; a
  PDF's canvas is named after its file, such as `paper.pdf`.
- **Item ids.** `slinger canvas show` names items `i12`. A packet names them
  by kind: `t12` is a text, `c12` is a cluster of ink (many strokes, one id),
  `d12` is a PDF's page.
  The verbs that take an item take both forms.
- **Units.** The verbs and `canvas show` use integers in tenths of a unit. A
  packet prints whole units: the packet's `120` is `1200` in a verb.
- **Packet.** What you read in place of a screenshot: images of the new ink,
  one line for each item, and an outline of the rest of the page.
- **Workspace.** A name, a directory, a canvas and an agent. The app shows
  each workspace as a window of panes; a pane shows a canvas or an agent UI.
- **Sling.** The user selects items, adds a prompt and sends them to the
  workspace's agent. The agent gets a packet and the prompt.
- **Agent name.** The verbs that change things for an agent (`canvas put`,
  `canvas edit`, `canvas remove`, `status set`, `ui open`) need the agent's
  name. A session that slinger started has `SLINGER_AGENT` set: pass nothing.
  A sling you took by listening names its agent on line 1 (`agent=<NAME>`):
  pass that name. From another terminal, pass `--agent <NAME>` with a name
  for yourself, such as `--agent claude`. The user sees that name as the
  maker of your items.
- **Ownership.** You can edit and remove only the items your agent name made.
  Never change the user's ink.

## Output

- Text on stdout by default, with tabs between fields. A command that
  succeeds with nothing to say prints nothing.
- `--json` prints one JSON document: `success`, `command`, then the data. A
  failure has `error`, `code` and `hint`.
- Exit codes: 0 ok, 1 error, 2 usage, 3 not found, 4 conflict, 5 invalid,
  6 the daemon does not answer. On 6, pass the hint on to the user (start
  `slingerd`, or retry or restart one that did not reply in time); do not
  start or restart it yourself unless the user asks.
- `slinger <command> --help` gives the full help of one command.

## Tasks

Find where to work:

```bash
slinger workspace list          # name, canvas and directory of each workspace
slinger canvas list             # every canvas id
slinger pane list <WORKSPACE>   # what the user's window shows
```

Read a canvas:

```bash
slinger canvas read <CANVAS>                 # the packet; `at=` names the directory of its images
slinger canvas show <CANVAS>                 # every item as JSON, in tenths
slinger canvas crop <CANVAS> --item c12      # an image of one item; read the file it prints
slinger canvas changes <CANVAS> --after 40 --wait 30000   # wait for the user's next change
```

Read the images of a packet before you answer about handwriting: the lines
give only the place of the ink, the images give the words.

Ink on a PDF: the packet's `pdf` line names the page and the PDF's blob,
`d3 pdf 0,0 1600x2071 pdfpage=1 name=paper.pdf blob=sha256:…`, and the ink's
line names the pages under it, `over=d3`. A sling from the app also shows
the page under the ink in its image, and ends the ink's line with the PDF's
words under it, quoted: that is what the user marked. To read more of the
PDF, fetch it:

```bash
slinger blob get sha256:9f86… --out paper.pdf
```

The app makes a PDF's pages; leave them as they are. `canvas crop` draws a
page as a grey frame, as only the app can draw a PDF. Answer beside the ink,
as anywhere.

Answer on the canvas. Put your answer beside the item it is about; slinger
finds a free place:

```bash
slinger canvas put <CANVAS> --beside t12 --text "The answer" --agent claude
slinger canvas put <CANVAS> --beside c12 --box 2400,1200 --color "#cc0000" --agent claude
slinger canvas put <CANVAS> --beside i7 --arrow i7 i9 --agent claude
slinger canvas edit <CANVAS> --item i14 --text "A better answer" --agent claude
slinger canvas remove <CANVAS> --item i14 --agent claude
```

Keep each text short: a few lines. Use more puts, each beside its subject,
in place of one long text. Leave out `--color` and `--size`: a put matches
the colour and size of the user's writing it goes beside, so it shows on
their page. Give them only to stand out on purpose. Each put prints the new item's id and its box.

Make many items in one undo step:

```bash
slinger canvas schema     # the item kinds and operations
slinger canvas apply <CANVAS> '[{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":0,"w":2400,"h":1200}}]' --agent claude
slinger canvas undo <CANVAS> --agent claude
```

Make a whole new canvas in one step from a file of the same operations; it
is made whole or not at all. Its items are the user's, so you cannot edit or
remove them after:

```bash
slinger canvas create <CANVAS> --ops ./ops.json
```

Show the user a UI in place of a long answer. A UI is one HTML file with
everything in it:

```bash
slinger ui put pick-slot ./pick-slot.html
slinger ui open pick-slot --data '{"slots":[9,10]}' --agent claude   # in your session's workspace, else pass --workspace <WORKSPACE>
slinger ui open pick-slot --as item --canvas <CANVAS> --beside i7 --agent claude
```

The user can resize a UI shown as an item, and the page lays out again at
the new width. Make the page fill its viewport (width 100%, no fixed pixel
sizes for the layout) rather than assume the size it was opened at.

Tell the user your state when the work is long:

```bash
slinger status set working --note "reading the canvas" --agent claude
slinger status set blocked --note "which page?" --agent claude
slinger status set done --note "3 notes added" --agent claude
```

Give work to a workspace's own agent, as the user's sling does:

```bash
slinger session list
slinger session start <WORKSPACE>
slinger sling send <CANVAS> --prompt "Sum up this page"
slinger session show <AGENT>      # its status and its last answer
```

A sling with no profile goes to a listener when one listens, and waits for
one when the agent has no process. `session show` says which: `deliver`,
`listening` and `pending`. That agent has every Claude Code tool, its
MCP servers and the user's own Claude Code config, but nobody answers its permission prompts: without asking it
runs `slinger`, read-only commands and file edits, plus the rules in its
workspace's `agent.tools`. A profile narrows it.

Take a workspace's slings yourself, in this session, with your own tools and
permissions. Each run waits for one sling and prints it:

```bash
slinger session listen <WORKSPACE> --pid $PPID   # in the background; act when it exits
slinger canvas put <CANVAS> --beside c12 --text "The answer" --agent <NAME>
slinger status set done --agent <NAME>        # then listen again
```

`--pid` names your session's process (`$PPID` in Claude Code): if it ends
before you set `done` or `blocked`, the sling waits for the next listener.

When slinger itself gets in your way, leave a note for its maker:

```bash
slinger feedback add "canvas put put my text over the ink"
```
