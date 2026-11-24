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
  `d12` is a PDF's page, `f12` is a figure (`f12 figure worksheet … parts=15`:
  its name follows, and `worksheet/<part>` names one part).
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
  (also `too_long`: your text is over 12 rows; `script` and `limit`: a
  drawing script failed or went over a limit), 6 the daemon does not answer. `slinger` finds a running daemon on any
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

A sling of a figure or one of its parts names it the same way
(`in=worksheet/1a-answer`). Answer beside it, `--beside worksheet/1a-answer`;
when the figure is yours and the answer belongs in it, redraw it (below).

Pick the form of an answer by what it is:

- A short remark, a question, a word or a box beside the user's writing:
  `canvas put`.
- A picture: a diagram, a plot, a table, a worksheet, or marks over the
  user's work. One `canvas draw` of a drawing script, never a run of puts.
- A change to a figure you drew: `canvas redraw` by its name, never a new
  draw beside the old one.
- Something the user clicks or fills in: a UI (`ui open`).

Draw a picture with code: write a JavaScript file (its API and an example
are under "Drawing scripts" below) and run it as one figure, a named set of
items the user's pen passes over. Look at it first with
`--dry-run --png` (it writes nothing), then draw it; a draw of the same
`--name` again replaces the figure in place. `--at` is in tenths of a unit,
as for `canvas put`:

```bash
slinger canvas draw <CANVAS> plot.js --name growth --beside c12 --dry-run --png /tmp/growth.png --agent claude
slinger canvas draw <CANVAS> plot.js --name growth --beside c12 --data '{"rate": 0.3}' --agent claude
```

Change a figure you drew by its data, not by drawing it anew: a redraw runs
its script again on its last data with yours merged in (an object merges key
by key, `null` takes a key away, anything else replaces), where the figure
now is, and writes only what changed; the user's writing on it stays. List
the figures, and read one's parts, script or data, with `canvas figure`:

```bash
slinger canvas redraw <CANVAS> growth --data '{"rate": 0.5}' --agent claude
slinger canvas figure <CANVAS>                    # name page x,y wxh by= parts=
slinger canvas figure <CANVAS> growth --data      # the data it last ran on
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

## Drawing scripts

The API of a `canvas draw` script.

You write one JavaScript file (ES2023, as QuickJS runs it). Slinger runs it
in a sandbox and turns what it draws into ordinary canvas items, placed as
one figure.

- Coordinates are canvas units in the figure's own frame: (0, 0) is its top
  left, x grows right, y grows down. Slinger decides where the figure goes on
  the page (`--at` or `--over` place (0, 0); `--beside` and `--in` place
  its frame). Never assume page coordinates.
- Text is monospace: a character is `size` tall and `size / 2` wide, so
  `measure('hello', {size: 24})` is `{w: 60, h: 24}`.
- No imports, no network, no files, no timers, no `Date`, no `console`.
  `Math.random` is seeded: the same script and data draw the same figure.
- The global `data` holds the JSON passed with `--data` (or `null`).
- Nothing is on the canvas until the script ends, so build a thing, read its
  size, then place it.

**Draw.** Every call returns a handle. The last argument is always an
options object.

```js
fig.text(str, x, y, {size = 24, w, align = 'left'})   // w wraps the text to that width; align: 'left' | 'center' | 'right' (x is then the centre or the right edge)
fig.box(x, y, w, h, {label, size = 20})               // label: text inside the box, top left, wrapped
fig.slot(x, y, w, h, {label, by = 'user'})            // a box meant to be filled in: dashed for the user, dotted when by: 'agent'
fig.line(x1, y1, x2, y2)
fig.path([[x, y], ...], {closed = false})             // a polyline; closed joins the last point to the first
fig.circle(cx, cy, r)
fig.ellipse(cx, cy, rx, ry)
fig.arrow(x1, y1, x2, y2, {heads = 'end', label})     // as line; heads: 'end' | 'both' | 'none'
fig.arrow(from, to, {heads, label})                   // from, to: [x, y] or a handle (the arrow then meets its edge where it is now)
```

Options every call takes:

- `color`: `'#rrggbb'`, `'#rrggbbaa'` or a name: `agent`, `ink`, `muted`,
  `accent`, `good`, `bad`, `warn`, `series1` to `series6`, `black`,
  `white`, `red`, `green`, `blue`, `yellow`, `orange`, `purple`, `grey`.
  Default: the colour of the group or part it is in, else `fig.color` (the
  agent colour; assign it to change every later default).
- `fill`: a colour, stored as given and drawn at 40% opacity at most, or
  `true` (a 20% tint of `color`), for a box, circle, ellipse or closed
  path. Fills draw under all ink and text, so they never hide writing.
- `dash`: `'dashed'`, `'dotted'` or `'solid'` (boxes, lines, paths, arrows).
- `width`: line width (lines, paths, arrows; default 2). An arrow with heads
  is always 2 wide.
- `name`: names this part of the figure, such as `'answer-a'` (`a-z`, `0-9`,
  `-`, up to 40). A later command finds it as `<figure>/<name>`. Name every
  area that will be written in or updated.
- `locked`: default `true`: the user's pen and eraser pass over it. Pass
  `false` for something the user should be able to move.

An unknown option, or one the call cannot take, fails the call and lists
what it takes.

**Handles.**

```js
h.x  h.y  h.w  h.h  h.right  h.bottom  h.cx  h.cy     // its bounds, read-only
h.moveTo(x, y)   h.moveBy(dx, dy)                     // each move returns h
h.below(other, gap = 24)   h.above(other, gap)   h.rightOf(other, gap)   h.leftOf(other, gap)
h.centerIn(other)                                     // other: a handle or {x, y, w, h}
h.name('part-name')
```

`below` and `above` move only y, `rightOf` and `leftOf` only x. A box's
label and an arrow's label move with it.

**Helpers.**

```js
measure(str, {size = 24, w})                 // {w, h} of text, wrapped to w when given
fig.group(h1, h2, ..., {color, fill, dash, locked, name})   // one handle over several; its options reach every shape in it that sets none
fig.part(name, {color, fill, dash, locked, size}, () => { ... })   // every shape the function draws is in part name; returns a handle over them. Parts do not nest
fig.stack([h1, h2, ...], {dir = 'down', gap = 24})   // dir: 'down' | 'right'; moves each after the one before; returns a group
fig.grid([h1, ...], {cols = 3, gap = 24, x = 0, y = 0})    // rows of cols; each column as wide as its widest
fig.table(rows, x, y, {size = 20, header = true, widths, rowH, parts})   // rows: arrays of cells; columns fit their longest cell
const p = fig.plot(x, y, w, h, {xRange: [0, 1], yRange: [0, 1], ticks = 0, frame = 'dotted'})
p.x(v)  p.y(v)                               // data value -> figure coordinate (y is up in data)
p.bars(values, {labels})                     // one bar per value across the plot
p.fn((x) => y, {color, width, steps = 50})   // a curve; or an array of [x, y] in data
p.vector([x1, y1], [x2, y2], {label})        // an arrow in data coordinates
p.dots([[x, y], ...])                        // filled dots of radius 4
p.bounds                                     // a handle over the whole plot: p.bounds.name('chart')
```

- `fig.table`: one box per cell and one text per non-empty cell; `header`
  tints the first row; `widths` is one width or one per column; `rowH`
  defaults to 1.6 of the size; `parts` names cells (an array of rows of part
  names, `null` for none). It returns a group with `t.cell(r, c)`, a cell's
  `{x, y, w, h}`: a cell is boxed already, draw in it, not another box on it.
- `fig.plot`: `frame` is `'dotted'`, `'dashed'`, `'solid'`, `'axes'` (two
  axis lines) or `'none'`. `ticks: n` labels n steps along both axes;
  `xticks`/`yticks` list them instead (a value, or `[value, "label"]` such
  as `[0, "Jan"]`); `xlabel`, `ylabel`, `title` (above the plot); `size`
  (labels, default 18). `bars`, `fn` and `dots` each take the next series
  colour unless given one.

**Context.** `fig.name` (the figure's name), `fig.view.target`
(`{x: 0, y: 0, w, h}` of the `--over` or `--in` item, else `null`),
`fig.view.page` (the page's name). The global `anchor` is the item the
figure goes beside or over, `{id, kind, w, h, text}` (`text` is `null` but
for a text), else `null`; the global `around`, with `--over` or `--at`,
holds the items within 500 units of that item or point, nearest first (at
most 500), each `{id, kind, x, y, w, h}` in the figure's coordinates, else
`[]`. `warn(message)` adds a warning.

**Warnings** do not fail the draw: a label taller than its box, a word wider
than its text's frame (it is cut), two named parts that overlap, text over a
line, texts overlapping, a box drawn again on the same frame without a fill,
an arrow's label longer than the arrow, a shape far from the others.

**Errors** name the call and the line: `script:12:7: fig.box: w must be a number above 0, got "wide"`,
then the source line with a `^` under the column and, inside a function,
the calls it was in (`at row (script:12:7)`). Its code is `script` (exit 5).
A script stopped at 1 s names a line its endless loop is at or after.
Limits: 1 s, 32 MB, a 512 KB stack, 5,000 shapes, a 1 MiB script, 256 KiB
of data. Over one is code `limit` (exit 5), and the message says which.

**An example.** `bars.js` draws a title, a bar chart of its data and an
answer area beside it that a later sling can name (`rain/why`):

```js
const title = fig.text(data.title, 0, 0, { size: 32 });
const top = Math.max(...data.rows.map(([, v]) => v));
const p = fig.plot(0, title.bottom + 40, 380, 300, { yRange: [0, top], yticks: [0, top] });
p.bars(data.rows.map(([, v]) => v), { labels: data.rows.map(([l]) => l) });
p.bounds.name('chart');
fig.slot(420, title.bottom + 40, 500, 300, { label: 'Why is March the wettest?', name: 'why' });
```

```bash
slinger canvas draw <CANVAS> bars.js --name rain --beside c12 --data '{"title": "Rain (mm)", "rows": [["Jan", 80], ["Feb", 65], ["Mar", 90]]}' --agent claude
slinger canvas redraw <CANVAS> rain --data '{"rows": [["Jan", 80], ["Feb", 65], ["Mar", 90], ["Apr", 40]]}' --agent claude
```
