# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/).

Entries are written for users: what changed for them, not how. Lead each
bullet with the area in bold.

## [Unreleased]

### Added

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
