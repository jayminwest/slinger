# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/).

Entries are written for users: what changed for them, not how. Lead each
bullet with the area in bold.

## [Unreleased]

### Added

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
- **Canvas:** undo and redo. Your undo takes back only your own changes, never an agent's, and an agent's undo never touches yours. A lasso move or one erase gesture is one undo step.
