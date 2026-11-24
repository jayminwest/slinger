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
  Never change the user's ink or the user's text. A text is the user's when
  the user wrote it or changed its words, and slinger refuses your edit:
  answer beside it.

## Output

- Text on stdout by default, with tabs between fields. A command that
  succeeds with nothing to say prints nothing.
- `--json` prints one JSON document: `success`, `command`, then the data. A
  failure has `error`, `code` and `hint`.
- Exit codes: 0 ok, 1 error, 2 usage, 3 not found, 4 conflict, 5 invalid
  (also `too_long`: your text is over 12 rows), 6 the daemon does not answer. `slinger` finds a running daemon on any
  socket, so on 6 pass the hint on to the user (on a Mac, opening
  slinger.app starts `slingerd`; or retry or restart one that did not
  reply in time); do not start or restart it yourself unless the user
  asks. `slinger daemon info` says which build
  answers, and where its log is; `slinger daemon stop` is for the user to ask
  for, as a restart is.
- `slinger <command> --help` gives the full help of one command.

## Tasks

Find where to work:

```bash
slinger workspace list          # name, canvas and directory of each workspace
slinger canvas list             # every canvas id
slinger pane list <WORKSPACE>   # what the user's window shows
slinger pane list <WORKSPACE> --json  # also each pane's page, pan and zoom, and the focused pane
```

Read a canvas:

```bash
slinger canvas read <CANVAS>                 # the packet; `at=` names the directory of its images
slinger canvas read <CANVAS> --in b4         # what box b4 holds: a folded box is one line, `folded=7`
slinger canvas read <CANVAS> --in worksheet/1a-answer  # one part of a figure, item by item, and the ink over it
slinger canvas show <CANVAS>                 # every item as JSON, in tenths
slinger canvas crop <CANVAS> --item c12      # an image of one item; read the file it prints
slinger canvas find <CANVAS> "the plan"     # page, item and text of each text that holds it ("i7 in i4": folded in box i4)
slinger canvas changes <CANVAS> --after 40 --wait 30000   # wait for the user's next change
```

Read the images of a packet before you answer about handwriting: the lines
give only the place of the ink, the images give the words. A text is never
an image: its line quotes it whole, but in the `page` section a text over
80 characters ends in `…`; `canvas show` gives it whole.

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
slinger canvas fold <CANVAS> b14 --agent claude     # fold your box to a miniature; unfold opens it
```

A sling of a box names it on its first line (`in=b4`) and its packet holds
only what the box holds. Answer in the box: add `--in b4` to each put
(`--beside` may be the box itself):

```bash
slinger canvas put <CANVAS> --beside c12 --in b4 --text "The answer" --agent claude
```

Draw a picture with code when a few puts would not do (a diagram, a plot, a
table, a worksheet): write a JavaScript file and run it as one figure, a
named set of items the user's pen passes over. Look at it first with
`--dry-run --png` (it writes nothing), then draw it; a draw of the same
`--name` again replaces the figure in place. `--at` is in canvas units, the
packet's:

```bash
slinger canvas draw <CANVAS> plot.js --name growth --beside c12 --dry-run --png /tmp/growth.png --agent claude
slinger canvas draw <CANVAS> plot.js --name growth --beside c12 --data '{"rate": 0.3}' --agent claude
```

Keep each text short: at most 12 rows, or the put exits 5 (`too_long`). Use
more puts, each beside its subject, in place of one long text. A question, a
box or an arrow often says more than prose. Answer the user's writing beside
it with `--beside`, never over it with `--at`, and never by editing it. Leave out `--color` and `--size`: a put matches
the colour and size of the user's writing it goes beside, so it shows on
their page. Give them only to stand out on purpose. Each put prints the new item's id and its box.

Make many items in one undo step:

```bash
slinger canvas schema     # the item kinds and operations
slinger canvas apply <CANVAS> '[{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":0,"w":2400,"h":1200}}]' --agent claude
slinger canvas undo <CANVAS> --agent claude
```

Make a whole new canvas in one step from a file of the same operations
(`--ops -` reads them from standard input); it is made whole or not at all:

```bash
slinger canvas create <CANVAS> --ops ./ops.json --agent claude
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

Make a UI look like part of the canvas, not a web page. It starts on the
canvas's paper, in its ink and the system font, light or dark as the app is,
so leave out your own font, page background, frame and shadows. For any
colour you need, use the app's: `var(--slinger-ink)`, `var(--slinger-muted)`,
`var(--slinger-accent)`, `var(--slinger-error)` and `var(--slinger-paper)`.

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

A sling goes to a listener when one listens, else to the agent's process,
which the sling starts when it does not run (`session start` is only needed
to start it early). `session show` says which: `deliver`, `listening` and
`pending`. That agent has every Claude Code tool, its
MCP servers and the user's own Claude Code config, but nobody answers its
permission prompts: without asking it runs `slinger`, read-only commands,
file edits, web fetch and search and skills, plus the user's own allow
rules and the rules in its workspace's `agent.tools` (an MCP tool needs
one). A profile narrows it.

Take a workspace's slings yourself, in this session, with your own tools and
permissions. Each run waits for one sling and prints it:

```bash
slinger session listen <WORKSPACE> --pid $PPID   # in the background; act when it exits
slinger canvas put <CANVAS> --beside c12 --text "The answer" --agent <NAME>
slinger status set done --agent <NAME>        # then listen again
```

`--pid` names your session's process (`$PPID` in Claude Code): if it ends
before you set `done` or `blocked`, the sling waits for the next listener.

Check the app yourself before you ask the owner to. A driven window is the
real app on a daemon of your own; it is never shown on a screen, and it does
not start unless its config names that daemon's socket:

```bash
cd "$(mktemp -d)" && mkdir ws
slingerd --socket d.sock --data-dir data --scratch & sleep 1
slinger --socket d.sock workspace open ws
printf '[daemon]\nsocket = "d.sock"\n[workspace]\nname = "ws"\n' > config.toml
SLINGER_CONFIG=$PWD/config.toml slinger-app --drive a.sock &
export SLINGER_DRIVE=$PWD/a.sock
slinger drive key cmd+4                 # a key, as the keyboard sends it
slinger drive press 300,200             # a mouse press; --tablet for the pen
slinger drive lift 300,200
slinger drive type "A note"
slinger drive stroke 300,300 340,320 380,300 --tablet
slinger drive space                     # hold the space bar (a pan with the pointer); --up lets go
slinger drive press 300,200 --button right  # the right button (--button middle, or --control for ctrl+click)
slinger drive lift 300,200 --button right   # within 4 points of its press: a context click; past them a drag pans
slinger drive pick New text             # choose an item of the context menu it opened (drive scene lists it)
slinger drive tick                      # take the daemon's news now
slinger drive scroll 300,200 0,-40      # scroll over a point (a negative DY scrolls up)
slinger drive scene                     # what the window shows, a line each
slinger drive shot window.png           # the window as a PNG, to look at
```

A driven window types canvas texts with slinger's own editor. Start it with
`SLINGER_NVIM=clean` (no config) or `SLINGER_NVIM=user` (yours) to have nvim
edit them as the app does: `drive scene` then prints nvim's grid, its mode,
cursor and rows, under a `cells` line. nvim's copies go to the file
`SLINGER_NVIM_CLIP` names (else `$TMPDIR/slinger-nvim/clip`), not to `drive clip`.

Points are in the window's points from its top left, as `drive scene`
prints them, written `X,Y` or `X Y`. `taken` means the app used the event, `passed` that it did
not. A cut or a copy stays in the driven window (`drive clip` prints it).
Stop the app and the daemon by their pids when you are done. A driven
window cannot check what comes before the app hears an event (a real
tablet, a key the system takes, another keyboard layout), a file panel, or
a click inside an agent UI's page: those stay hand checks.

When slinger itself gets in your way, leave a note for its maker:

```bash
slinger feedback add "canvas put put my text over the ink"
```
