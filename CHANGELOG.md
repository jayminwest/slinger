# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/).

Entries are written for users: what changed for them, not how. Lead each
bullet with the area in bold.

## [Unreleased]

### Fixed

- **Canvas:** text added with `canvas apply` and no size now gets a box that
  fits its words, so `canvas put --beside` it no longer lands on top of it.
  Text already on a canvas with no box gets one when the canvas is read. A
  `set` that changes the words and gives no `w` or `h` fits the box to the
  new words.

### Added

- **App:** the window resizes. Drag its edge or corner, or zoom it, and the
  sidebar and the panes follow. It opens at 960x600 and goes down to 480x320.

- **App:** two new commands, `show-keys` (`cmd+/`) and `show-commands`
  (`cmd+k`). They do nothing yet: the key overlay and the command palette
  that they open come next. Rebind them in `[keys]` as any other command.

- **App:** the sidebar has a `WORKSPACES` heading and a row per workspace:
  its name, bold on a lighter row for the workspace the window shows; under
  it the agent's mark with `<agent> · <status>` (or `no agent`); and, when
  the agent is blocked, its note. The marks differ in shape as well as
  colour: round for working, round and green for done, square and red for
  blocked. The button's mark matches.
- **App:** the focus border shows only when there are two panes or more.
- **App:** a canvas pane's view starts with a margin, so an item at the
  canvas's origin no longer touches the pane's edge.
- **App:** a selection shows as a tinted box with a handle at each corner
  and its number of items above it. A text being typed shows a caret after
  its last character, a dashed frame around it and, under it, how to end it.

- **App:** the "send to agent" button. The `place-button` command
  (`cmd+shift+b`) puts one in the middle of the focused canvas pane. Tap it
  to sling what is new on the page since the last sling to the agent of the
  canvas's workspace, with no prompt. To add a prompt, press `sling` with
  nothing selected: the prompt opens beside the button; type, then tap the
  button or press return. Its mark shows the agent's status, as the sidebar
  does: working, blocked or done. Select, move, delete and undo it like any
  item. `slinger canvas apply <C> '{"op":"add","item":{"page":"p1","kind":"button","x":0,"y":0,"w":1600,"h":400}}'`
  places one from a terminal. A packet lists it as a `button` line; older
  versions of slinger skip it when they read the canvas.

- **Agent UIs:** live items. `slinger ui open <UI> --as item --canvas <C>
  --beside <ITEM> [--size W,H] [--data JSON]` puts an agent's UI on a canvas,
  beside an item, instead of in a pane. It prints the showing's id and the
  item's id. In the app the UI moves and scales with the canvas as you pan
  and zoom, and it is cut off at its pane's edge. Click into it to use it;
  lasso it to select it, then drag it or delete it like any item, and undo
  either. Until its page has loaded, the item shows its box and the UI's
  name. A packet lists it as a `live` line, never an image. Older versions
  of slinger skip a live item when they read the canvas.
- **App:** `slinger-app --preview <UI> --as item` shows a UI as a live item
  would show it.
- **App:** sling from the window. With items selected on the focused
  canvas, the `sling` command (`cmd+return`) opens a one-line prompt at the
  selection. Return sends the selection and the prompt (an empty one too) to
  the agent of the canvas's workspace; escape cancels. The sling waits until
  the ink you drew before it has reached the daemon, so the agent sees what
  you saw. The answer arrives on the canvas, and the agent's status shows
  working, then done. If the sling fails (for example, the agent is not
  running), the prompt stays open with the error and what to do; return
  tries again.
- **App:** speak the sling's prompt. The `dictate` command
  (`cmd+shift+return`) opens the prompt at the selection and starts Flick's
  dictation; press it again to stop, and Flick types what you said into the
  prompt. Escape or return cancels a dictation still going. If Flick is not
  running, or its dictation is not set up, the prompt says so once and you
  can type instead. The prompt now takes cmd+X, C, V and A.
- **Agent UIs:** `slinger ui open <UI> --workspace <W> [--data JSON]` lets an
  agent show a UI in a new pane, with data, and get the user's answer back.
  It prints the showing's id (`pick-slot#12`) and the pane. Each
  `slinger.reply()` from that pane is recorded and reaches the agent's
  running session as one `ui_reply` JSON line, in order. If the agent is not
  running, the reply stays in the record and the pane says so. Data and each
  reply are at most 16 KiB, and a showing sends at most 100 replies.
- **App:** an agent UI pane shows the UI. `slinger pane open <workspace>
  --type ui --target <name>` shows the latest version of that UI in a
  sandbox: no network, no files, no way to navigate away or open a window.
  The UI reads what it was given with `slinger.context()`; a UI opened by
  name has no agent, so `slinger.reply()` answers `unavailable` there. A UI that cannot show
  says its name and why, and the rest of the window keeps working.
  `slinger-app --preview <UI> [--data JSON]` shows one UI alone, with that
  data, and prints each reply as a `ui_reply` JSON line.

- **Canvas:** text, boxes and arrows. With the text tool, tap to place a
  text and type it; `escape` or a tap outside ends it. Tap a text with the
  text tool, or double tap it with any tool but the pen, to edit it; empty it
  to remove it. With the box tool, drag from corner to corner; with the arrow
  tool, drag from start to end. An arrow end that lands on an item stays on
  it: move the item and the arrow follows. Each text, box, arrow and edit is
  one step. Pick a tool from the tool and shape pop-ups or with the new
  `tool-pen`, `tool-eraser`, `tool-lasso`, `tool-text`, `tool-box` and
  `tool-arrow` keys (`cmd+1` to `cmd+6`).
- **Canvas:** pages, undo and redo in the app. Each pane shows its own page,
  named quietly at its foot. `cmd+]` and `cmd+[` turn the focused pane to
  the next or previous page, `cmd+n` adds a page, `cmd+shift+r` renames
  the page (type the name, then `return`), and `cmd+shift+delete` removes
  it; a page with items asks first, and the last page stays. `cmd+z` undoes
  your last step on the canvas and `cmd+shift+z` redoes it. Undo is yours
  alone: it never takes back what an agent put. New keys: `previous-page`,
  `new-page`, `rename-page`, `remove-page`, `undo`, `redo`.
- **App:** resize panes and move things between them with the pen. Drag
  the divider between two panes (it is easy to catch: 8 points either
  side) to resize them; the workspace keeps the new sizes, and so do the
  `grow-pane` and `shrink-pane` keys now. Drag a selection into a pane on
  another canvas and its items move there, where the pen let go: one undo
  step on each canvas. Let go outside any canvas pane and nothing moves.
  `slinger pane resize <workspace> <pane> <ratio>` moves the divider after
  a pane from a terminal.
- **Canvas:** erase, select and move. With the eraser, every stroke the pen
  passes over goes at once; the whole drag is one step. With the lasso, draw
  a loop: the items wholly inside it are selected, with a box around them.
  Drag inside the box to move them, press `delete` to remove them, or
  `escape` to select nothing (the `delete-selection` and `clear-selection`
  keys). Each drag, and each delete, is one step. Anything on the canvas can
  be erased or moved, an agent's items too.
- **Canvas:** draw with the pen. With the pen tool, a stroke on a canvas pane
  shows under the pen at once, at a constant width in the pen's colour, and
  is saved to the canvas when the pen lifts (one stroke, one undo step). If
  the daemon cannot be reached, your strokes stay on screen and are saved, in
  the order you drew them, once it is back. A palm or finger on a tablet
  display does not draw. The app writes each stroke's pen-to-screen delay to
  stderr as an `ink:` line.
- **App:** a sidebar lists the open workspaces and each agent's status with
  one mark: working, blocked or done. `toggle-sidebar` (cmd+b) opens or
  collapses it, and `[window] sidebar = "open"` or `"closed"` sets how the app
  starts. When an agent becomes blocked or done, one small notice at the
  bottom of the window names it, with its note; nothing else interrupts.
  `dismiss-notice` (cmd+.) hides it, and it goes on its own once the agent's
  status moves on.
- **Panes:** the server keeps each workspace's panes, so you, the app and
  agents share them. `slinger pane list <workspace>` numbers them,
  `slinger pane open <workspace> --type canvas|ui --target <id> [--beside
  <pane>] [--below]` opens one, and `slinger pane close <workspace> <pane>`
  closes one (one already gone is fine). The app shows the panes of
  `[workspace] name` in its config, else the first open workspace; its
  splits and closes go to the server, and a pane opened from a terminal shows
  in the window within a second. The layout survives a restart. Each change
  is recorded as `pane_moved`. Growing and shrinking a pane stays in the
  window for now.

- **App:** a canvas pane draws its canvas from the server and follows it:
  pen lines, text, boxes and arrows in their colours, and a change made
  anywhere (`slinger canvas put`, another client) shows within about half a
  second. Scroll or alt+arrows pan, pinch or alt+= and alt+- zoom, and
  `next-page` (cmd+]) steps through the pages. Ink too dark to see on the
  theme's background draws in the theme's text colour.
- **App:** pop-ups at the pen. `pick-colour` (cmd+shift+c), `pick-shape`
  (cmd+shift+s) and `pick-tool` (cmd+shift+t) open a small pop-up where the
  pen or pointer last was, kept inside the window. Pick with the pen, with a
  digit, or with the arrow keys and return; escape or the same command again
  closes it. The colours come from `[pen] colours` in the config file (one to
  nine, `"#rrggbb"`, split by spaces or commas). The canvas that uses the
  chosen tool and colour comes later.
- **Sessions:** an agent reports without being asked. Each Claude Code
  process slingerd starts gets hooks of its own (your Claude Code settings
  are untouched): its status is `working` while a turn runs, `done` when it
  ends and `blocked` while it waits for your permission or input, and every
  file it writes is recorded by path and hash. `slingerd --slinger <path>`
  names the CLI the hooks run; its folder also comes first on the agent's
  `PATH`, so the agent's own `slinger` commands run the same CLI.
- **CLI:** `slinger file touched <path>` records that the calling agent wrote
  a file: its path and hash, never its contents. `--hook` reads a Claude Code
  hook's payload on stdin instead.

- **Daemon:** a session runs within stated limits. By default the agent
  reads only its workspace's roots, runs only the `slinger` CLI and read-only
  commands, and does not load your own Claude Code settings and
  instructions; anything else it would ask for is denied. The workspace's
  `[agent]` widens it with `dirs`, `tools` (Claude Code permission rules,
  such as `"Edit"` or `"Bash(git *)"`) and `user_config = true`.
- **CLI:** sling. `slinger sling send <canvas> --select <ids> --prompt
  <text|->` sends the ink you drew since the canvas's last sling, your
  selection and your prompt to the agent of the canvas's workspace, and
  returns as soon as it is delivered; the answer lands on the canvas. Every
  sling goes into the same conversation, and selecting old ink again sends
  no ink twice. Each sling is in the record with its packet. If the agent is
  not running, it exits 6 and tells you to `session start` it: slinger never
  starts an agent for you.
- **App:** the window holds panes. Split the focused pane right
  (`cmd+d`) or down (`cmd+shift+d`), close it (`cmd+w`; its space goes to
  the pane beside it), move focus to the next pane (`ctrl+tab`) or the one
  beside it (`cmd+alt+arrow`), and grow or shrink it (`cmd+=`, `cmd+-`).
  One border, in the theme's new `focus` colour, marks the focused pane;
  there is no other chrome: the daemon's greeting is gone, and a problem
  with the daemon or the config file shows in the focused pane. Each pane
  shows its kind and name for now. The keys are in `[keys]`, and
  `config.example.toml` lists them.

- **CLI:** agents answer on the canvas. `slinger canvas put <canvas> --beside
  <id> --text <text>` puts text beside an item, where it covers nothing:
  to the right first, else below, left or above. `--box w,h` puts a box and
  `--arrow <from> <to>` an arrow between two items instead. The item carries
  the agent's name (`--agent` or `SLINGER_AGENT`) and the time, and the
  command prints its id and box. After a sling to that agent, the put also
  records that the reply landed.
- **CLI:** agents change or take back what they put. `slinger canvas edit
  <canvas> --item <id>` gives a text new words (`--text`), a text or a box a
  new size (`--box w,h`) or a new place (`--move x,y`), and `canvas remove
  <canvas> --item <id>` removes it; removing it again succeeds. An agent can
  change only items it made, never your ink.
- **CLI:** run a workspace's agent. `slinger session start <workspace>`
  starts Claude Code in the agent's home and keeps it running; `session send
  <workspace> <text|->` delivers one message to it, and every message goes to
  the same conversation. `session show` has the process's state, its session
  id and its last answer. After slingerd restarts, `session start` resumes
  the same conversation. If the process dies, the session shows `exited` and
  a send exits 6 until you start it again; `session stop` ends it, and so
  does stopping slingerd. The agent's own `slinger` calls find it through
  `SLINGER_AGENT` and the new `SLINGER_SOCKET`, which `--socket` overrides.
  `slingerd --claude <path>` runs another Claude Code program.

- **Sling:** what an agent receives for one sling: your prompt, the page's
  packet with each image under its line, the items you selected, and a short
  note on how to answer on the canvas instead of in chat. The first sling of
  a session gets the whole note, later ones a single line. An empty prompt
  with a selection is a valid sling.
- **CLI:** sessions and agent status. `slinger session list` shows each agent
  of an open workspace with its status, home directory and note, and
  `session show <agent>` prints one. An agent reports what it is doing with
  `slinger status set <working|blocked|done> [--note <text>]`; `blocked` and
  `done` also go into the record, tagged with the agent and its workspace, and
  survive a restart. The agent's name comes from `--agent` or the
  `SLINGER_AGENT` environment variable; with neither, the command exits 2.
- **CLI:** workspaces. `slinger workspace open <dir>` opens a directory as a
  workspace: it writes `slinger-workspace.toml` in the directory if there is
  none, makes the workspace's canvas, and prints its name; opening it again
  returns the same workspace. `workspace open --name <name>` opens one with no
  directory, kept in slingerd's data directory. `workspace list`,
  `workspace show <name|dir>` and `workspace close <name|dir>` list, print and
  close them; closing keeps the file and the canvas. Open workspaces survive a
  restart, and an event on a workspace's canvas or from its agent is tagged
  with the workspace.
- **CLI:** `slinger canvas read <id>` prints a page's packet: what an agent
  reads instead of a screenshot. New ink since the last sling is images, the
  rest of the page an outline. `--after <n>` reads from another point,
  `--page` picks the page, `--select c42,t12` narrows the outline and
  `--out <dir>` says where the images go. Every packet and image is stored
  in the record, so `blob get` fetches them later.
- **CLI:** `slinger canvas crop <id> --item <item>` (or `--box x,y,w,h`
  with `--page`) writes an image of one item, one ink cluster or one box at
  the packet's scale, in tiles when it is large, and prints each file's
  path. `slinger canvas strokes <id> --item <item>` prints the raw points of
  a stroke or of every stroke of a cluster.
- **CLI:** agent UIs are stored in the record, named and versioned.
  `slinger ui put <name> <file|->` stores an HTML file as the UI's next
  version and prints `<name>@<version>`; the same bytes again add no version.
  `slinger ui list` shows each UI's latest version, `slinger ui show <name>`
  every version with its hash, size, author and time, and
  `slinger ui export <name>[@<version>] [--out <dir>]` writes one version
  back as `<name>.v<version>.html`.
- **CLI:** `slinger canvas import <file.tldraw> [--name <id>]` makes a new
  canvas from a tldraw board, with its pages, drawings, text, rectangles and
  arrows, and prints its id and what it holds. Shapes slinger has no item for
  (groups, lines, ellipses) are counted as skipped.
- **CLI:** canvases. `slinger canvas create <id>` makes one, `canvas list`
  lists them, `canvas show <id> [--page]` prints a page's items, `canvas apply
  <id> <ops|->` adds, moves, edits and removes items and pages, and `canvas
  undo` / `canvas redo` step back and forth. `--agent <name>` acts as an agent,
  which may change only what it made. slingerd keeps each canvas in a file in
  its data directory and every change is on disk before the command returns.
- **CLI:** `slinger canvas changes <id> --after <number>` prints the canvas's
  operations after that number, one JSON line each. `--wait <ms>` waits for
  the next change when there is none yet, so a client can follow a canvas
  without polling it.
- **CLI:** `slinger blob put <file|->` stores a file's bytes (or stdin's) in
  this machine's record and prints the blob id. `slinger blob get <id>`
  writes the bytes to stdout, or to a file with `--out`. Storing the same
  bytes twice gives the same id; an unknown id exits with `not_found`.
- **CLI:** `slinger record append <kind> [payload]` adds an event to this
  machine's record and prints its number. `slinger record list` prints the
  events, filtered by `--kind`, `--machine`, `--workspace`, `--agent`,
  `--canvas` and `--after <number>`. A payload that does not fit its kind
  is refused with a hint.
- **App:** a plain-text config file, `~/.config/slinger/config.toml` (or
  `$SLINGER_CONFIG`), sets the theme's colours and the daemon's socket. A
  `config.local.toml` next to it holds one machine's values. Edits show in
  the running app within a second; a file with an error keeps the last good
  config and the window shows the file, the line and what to fix.
  `config.example.toml` lists every key.
- **App:** every action is a key command, bound to a key chord in the
  config's `[keys]` table (`quit = "cmd+q"`). `quit` (⌘Q) and
  `reload-config` (⌘R, read the config files now) are the first commands;
  `""` leaves a command without a key. An unknown command, a chord that does
  not parse, or two commands on one chord is a config error with its line. A
  tablet button works by sending a key chord from the tablet's own software.
  The app no longer has a menu bar: quit with the `quit` key or by closing
  the window.

- **Record:** events (sling sent, reply landed, file touched, pane moved,
  agent blocked, agent done) and blob ids have a versioned JSON format. A
  reader keeps events of kinds and fields it does not know, so records from
  newer versions stay readable.
- **App:** the groundwork for the canvas pane: an endless canvas you can pan and zoom (within limits) about any point, drawing only what is on screen and repainting only what changed. Not yet shown in the window.
- **Workspaces:** a workspace is defined by a `slinger-workspace.toml` file in its primary root: a name, its directories, an optional agent, its record tag and its default canvas. See `docs/workspace.md`.
- **Canvas:** a canvas file format: a versioned, append-only log of every change.
  A crash in the middle of a save loses only that unsaved change, and canvases
  written by newer versions stay readable.
- **Canvas:** the canvas model: pages of strokes, text, boxes and arrows, each with its author and time. An agent can change only the items it made, never your ink, and removing something already gone is not an error.
- **Canvas:** tldraw boards can be read into a canvas: pages, pen strokes,
  text, rectangles (with their labels) and arrows (still pointing at what
  they were attached to), in their colours and drawing order. Shapes with no
  Slinger counterpart are skipped and counted. There is no export to tldraw.
- **Daemon:** `slingerd` keeps this machine's record (events and blobs) in
  one SQLite file in its data directory. The directory defaults to
  `$XDG_DATA_HOME/slinger` (on macOS, `~/Library/Application Support/slinger`)
  and `--data-dir` sets it.
- **Canvas:** the canvas knows what is new, changed or removed since the last sling, so a sling sends only that. Something added and then erased since is not new, and nothing new is a valid answer.
- **Canvas:** geometry queries on a page: the item under a point, the items inside a lasso (even one that crosses itself), the strokes an eraser touches, the items a box touches, and the box around a selection. Fast enough for pen input on a page of 7500 strokes.
- **Packet:** the text an agent will read from a canvas page instead of a screenshot: one line per item with its id, box, kind and what it points at, the new ink first, then what agents made, then an outline of the rest. What an agent made is always text, never an image.
- **Packet:** images of your ink at a fixed scale, so handwriting reads the same however the window is zoomed. An image too big to send in one piece is cut into tiles that overlap, so no word is lost at a tile's edge.
- **Packet:** a whole packet for a page: what is new since the last sling (your ink with its images, and what agents made), then an outline of the rest, or only of what you selected. It carries a format version, and nothing new is still a valid packet. The whole fixture board costs under half of what its screenshots did.
- **Packet:** your new ink is grouped into notes by distance, colour and time, so two notes of different colours side by side stay apart, and a frame, arrow or divider you drew never glues notes together.
- **Canvas:** undo and redo. Your undo takes back only your own changes, never an agent's, and an agent's undo never touches yours. A lasso move or one erase gesture is one undo step.

### Fixed

- **App:** a canvas pane no longer leaves a stale copy of its items over the
  sidebar or the pane beside it. Text, boxes or buttons that a pan or a
  sidebar toggle put part past the pane's edge drew past it and stayed
  there; now each pane draws only inside its own frame.
- **Canvas:** the first box, arrow, stroke, erase or lasso after a text you
  were typing is no longer lost. A press outside the text with any tool but
  the text tool ends the text and starts that tool's gesture at once; with
  the text tool, a tap outside still only ends the text.
