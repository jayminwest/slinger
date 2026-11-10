# Architecture

Read this before you change a contract. Update it in the same commit.

## Layers

```
crates/core   slinger-core: domain logic, typed errors. No terminal, process or front-end deps.
crates/cli    slinger: argv -> control-socket requests -> output contract (docs/cli.md).
              Does not link the core.
crates/control  slinger-control: the control protocol (docs/service.md): requests, replies,
              the wire format and the socket client. Does not link the core.
crates/daemon slingerd: owns the IO the core leaves out (socket, signals, startup, the
              record's SQLite file) and serves the core. A thin main over its library.
              No domain logic.
crates/app    slinger-app: the macOS app. Control-socket requests -> a scene -> one window.
              Does not link the core.
xtask         quality gates. Not shipped.
```

One request travels this path, and no other:

```
argv ─ crates/cli ─┐
window ─ crates/app ─┤
                     ├─ control::Client ─ socket ─ crates/daemon ─ core
other programs ──────┘        (one JSON line each way)
```

Only the daemon links the core, so each capability has one implementation
and one path to it. No binary crate depends on another. The clients' tests
dev-depend on the daemon's library, so they run against a real socket.

### Inside the core

```
canvas/      pages of items, changed only by operations. Build (Canvas::new),
             apply (Canvas::apply(Op)), query (pages, items, item). Ops are data,
             so a log of them replays to the same canvas, ids included. The
             storage layout is private. Keeps the author rule: an agent changes
             only items it made; removing what is already gone succeeds.
             History: undo and redo per author; a step is one op or a group
             (lasso move, erase gesture). Undo applies inverse ops (Restore,
             PageRestore put things back under their old ids) and returns
             them, so the op log stays append-only. Not persisted.
canvas/delta.rs  what is new after the last sling (R1): an op log and a
             marker (ops applied before it) in; the items new, changed or
             removed against the canvas at the marker out. Compares the two
             canvases, so added-then-removed is nothing. No errors.
canvas/geometry.rs  pure queries on a page: item at a point, items in a
             lasso (nonzero winding), strokes an eraser touches, items touching a
             box, bounds of a set. No errors: degenerate input finds nothing.
canvas/file/ the canvas file (docs/design/canvas-file.md): a versioned, append-only
             JSON Lines log of operations. Header and line encode, read (cuts a
             torn last line), replay into a canvas, snap to the stored tenths. No IO.
             Log holds a file open for the daemon: requests (ops as JSON, the
             author, the time) in, numbered lines to append out, with the
             History beside it. Canvas ids (check_id) never name a path.
             Log::import stores a whole canvas (an import) as a new file
             that replays to it, ids, makers and times included.
             Log keeps every op it applied with its line's number, so
             Log::packet builds a page's packet with the op range in the
             file's line numbers and takes item ids as the file or a packet
             writes them.
packet/      what an agent reads from a page (docs/design/packet.md). Index::new takes a
             canvas page, the clusters of the user's ink and what changed, and gives
             the `new`, `made` and `page` lines in reading order, plus the render list:
             the new clusters that need images. Agent-made items are lines, never in
             the render list (R2). Owns the line grammar, ids and reading order.
             cluster(canvas, strokes) groups strokes into notes by distance, colour
             and time, sized by each pen's median stroke; a long stroke (frame, arrow,
             divider) and an agent's stroke stay alone. Same strokes, same clusters.
             build(log, ask) is the one entry: an op log, a page, the last sling's
             marker and a selection in; a Packet (header, sections, images) out.
             It hides the delta, clustering (whole page), rendering and the index.
             Packet::text / Packet::read own the versioned format (VERSION, B5).
tldraw/      the tldraw importer (SPEC D7): import(bytes, time) -> canvas and a
             report of what it made and skipped. Reads the zip + SQLite file
             and the older JSON one itself (no IO, no new dependencies); every
             item goes through Canvas::apply. Import only, never export.
render/      images of items (docs/design/packet.md, Images): a set of items or a
             box of a page in, PNGs out, at a fixed scale, cut into overlapping
             tiles past the size limit (SCALE, MAX_EDGE, OVERLAP, MARGIN). Owns
             the rasterizer (raster.rs) and the PNG container (png.rs). Pure and
             deterministic: the same input gives the same bytes.
sling.rs     the sling message (SPEC section 6, D10): a packet, the prompt, the
             selected packet ids, first-or-later and the verbs the caller serves in;
             text and image blocks out, each image after the packet line naming it.
             Line 1 is `slinger-sling <VERSION>`; then the packet, a `# reply` note
             (whole on a session's first sling, one line after) and the prompt last.
             The note names only the verbs passed in. No errors.
record/      the record's formats (SPEC section 9): events as versioned JSON
             (encode, decode, to_value), the kinds a writer may append
             (Body::new checks a payload against its kind) and blob ids
             (SHA-256, `sha256:<hex>`). No IO; the daemon stores what it encodes.
ui.rs        agent UIs (docs/design/live-items.md): the rules for a name and its
             code, the versions rebuilt from `ui_put` events (Catalog), what a
             put adds and what a reference (`name`, `name@3`, blob id) names.
             No IO; the daemon hands it the events.
workspace.rs workspaces (SPEC section 10): the definition file's format
             (docs/workspace.md), the rules a set of them keeps (one primary
             root, name and canvas each), what a new one gets (slug of its
             name) and which workspace an event's canvas or agent tags. No IO;
             callers read and write the bytes.
error.rs     the typed errors and their stable codes.
```

**The record's formats are API (B5).** A reader keeps an event of a kind
it does not know (`Body::Other`) and every field it does not know, at any
depth, and `encode` writes them back unchanged. A payload changes only by
adding fields, and the change bumps that kind's `version`.

### Inside the daemon

```
lib.rs       argv, startup and signals: `run`.
server.rs    the control socket and `answer`, one arm per verb. `serve` takes the
             store, so a verb that reads or writes the record reaches it here.
             Blob and UI verbs read and write the file the client names: a
             blob's bytes travel as a file, never in the line (docs/service.md).
             `ui_put` decides its version inside the store's write
             (`Store::append_if`), so two puts cannot take one number.
store.rs     the record on disk: one SQLite file (rusqlite, bundled) with
             the event log and the blobs. The only module that knows its
             layout. Stores each event as the line `record::encode` wrote.
             Hands out the canvases and the workspaces, so `answer` reaches
             all data one way.
canvases.rs  canvas files on disk, the only writer (K5): one lock per canvas,
             so changes are numbered in the order they apply; append, sync,
             then reply. A failed write closes the canvas; the next request
             reads the file again. Cuts a torn last line. A new canvas's
             file (empty or imported) is written aside and renamed in whole.
server/import.rs  `canvas_import`: reads the client's file, calls the tldraw
             importer, stores the canvas, replies with the counts.
workspaces.rs the open workspaces: the list of directories that hold their
             files (`workspaces/open.json`), written whole; the files are read
             on every request and checked by the core's rules. The store asks
             it for the workspace tag of each event it appends.
server/read.rs  `canvas_read`: the marker is the `to` of the packet the
             canvas's last `sling_sent` names (0 before one), so it lives in
             the record and survives a restart. Stores the text (no `at`)
             and each image as blobs, then writes them into `--out` or the
             store's `packets/<hex>/`.
```

### Inside the app

```
main.rs      wires the modules below. The only caller of platform.
backend.rs   the app's IO: the one module that talks to the daemon, and the
             one that reads and watches the config files.
config.rs    the config file's format: text in, a typed config or an error
             with its line out; merges a machine's overlay over it. Pure.
commands.rs  key commands: the closed list of commands, the chord parser and
             the keymap from a chord to a command. Pure.
scene.rs     decides what the window shows. Pure.
scene/canvas.rs  decides what a canvas pane shows: a page's items and a viewport in,
             the visible marks in screen points and the box to repaint out. Pure.
layout.rs    geometry: sizes in, rectangles out, and the canvas viewport (pan, zoom
             about a point, limits; the canvas has no edge). Pure. 100% coverage floor.
platform/    draws a scene and turns a key press into a chord. All unsafe, objc2
             and AppKit code. No decisions.
```

**Threading rule:** AppKit runs on the main thread only. Every `platform`
function that touches AppKit takes a `MainThreadMarker`, which only the main
thread can make, so the compiler holds the rule. Long blocking work runs
before the run loop starts or on another thread, which hands its result to
the main thread to draw. Never block for long inside an AppKit callback.

**Config:** `config.example.toml` at the repository root lists every key.
The app reads `config.toml` and merges `config.local.toml` (the machine's
overlay) over it. A main-thread timer stats both files twice a second and,
on a change, rereads them and draws a new scene; it asks the daemon again
only when the socket changed. No worker thread: the app binary has a size
budget, and the release profile builds the app at `opt-level = "z"`. A file
with an error keeps the last good config, and the window shows the file,
the line and what to fix.
A new setting changes `config.rs` (the key, its default, its test) and the
example file, which a test checks against the keys.

**Keys:** every action is a named command (SPEC P4), bound to a chord in the
config's `[keys]` table. The window's `sendEvent:` turns each key press into
a chord and hands it to `main`, which looks it up in the keymap: a command
runs and the press stops there; any other key goes on to the window as
usual. The app has no menu bar, so no key reaches AppKit's menus first, and
no action has a second path outside the keymap (the window's close button
quits too). A new command adds a variant and a name to `Command::ALL` in
`commands.rs`, an arm in `main`'s match, and a line in the example file
(with its default chord, if it has one).
## Boundaries

- Dependencies point inward. Front ends depend on the core. The core never
  depends on a front end.
- Each capability has one implementation, in the core. A front end that
  reimplements logic is a bug.
- Every failure a caller can act on is a typed error with a stable code.
  Front ends map codes to exit codes or statuses. They never parse messages.

Each boundary has something that fails when it is broken:

| Boundary | Held by |
|---|---|
| The core does no process, terminal or network IO and no FFI | `check:layers` rule `core-is-pure` |
| The protocol crate does not link the core | `check:layers` rule `control-is-transport-only` |
| The CLI reaches the core only through the socket | `check:layers` rule `cli-uses-the-socket` |
| The app reaches the core only through the socket | `check:layers` rule `app-uses-the-socket` |
| The record's event and blob-id formats are readable by older readers | `crates/core/tests/record.rs`, with stored fixtures |
| The canvas file format is readable by older readers | `crates/core/tests/canvas_file.rs`, with a stored v1 fixture |
| The wire format (docs/service.md) | the tests of `crates/control/src/wire.rs` |
| The packet's line format (docs/design/packet.md); no agent item is rendered | `crates/core/tests/packet.rs`, with a stored snapshot |
| The packet's image rules: scale, margin, tiles, same bytes for the same input | `crates/core/tests/render.rs`, with golden images |
| The packet format reads back, an old packet keeps decoding, the board costs under half its screenshots | `crates/core/tests/packet_build.rs`, with a stored v1 packet, on the real board |
| The sling message: first and later snapshots; the note names only the verbs given | `crates/core/tests/sling.rs`, with stored snapshots |
| Clustering: colours never mix, long strokes join nothing, each fixture page's count | `crates/core/tests/packet_cluster.rs`, on the real board |
| The workspace file format and rules (docs/workspace.md) | the tests of `crates/core/src/workspace.rs` and its stored fixture |
| The record on disk is append-only and keeps fields it does not know | `crates/daemon/src/store/tests.rs`, against a real file |
| One daemon per socket; a stop loses no reply | `crates/daemon/src/tests.rs`, through a real socket |
| A blob of megabytes comes back byte for byte; a put twice gives one id | `crates/daemon/src/tests.rs`, through a real socket |
| Every UI version stays; the same bytes add none; an export is the bytes put | `crates/daemon/src/tests.rs`, through a real socket |
| A canvas survives a restart; concurrent changes are numbered with no gap | `crates/daemon/src/tests/canvas.rs`, through a real socket |
| `canvas read`: only ink after the marker is new; each packet and image is a blob; the stored text has no `at` | `crates/core/tests/canvas_packet.rs` and `crates/daemon/src/tests/read.rs`, through a real socket |
| An imported canvas reads back with the importer's pages, ids and arrow ends | `crates/core/tests/canvas_import.rs` and `crates/daemon/src/tests/import.rs`, on the real board |
| Open workspaces survive a restart; each has its canvas; one primary root per workspace | `crates/daemon/src/tests/workspace.rs`, through a real socket |
| SIGTERM ends the daemon cleanly | `crates/daemon/tests/signals.rs`, against the real binary |
| Only `platform` holds `unsafe`, objc2 or AppKit code | `check:layers` rule `platform-owns-ffi`, and `unsafe_code = "deny"` everywhere else |
| Only `main.rs` starts the platform | `check:layers` rule `app-logic-is-pure` |
| Layout is fully tested | the 100% floor on `crates/app/src/layout.rs` in `budgets/coverage-budgets.json` |
| The app binary does not grow unnoticed | `check:binary-size` |

The rules are in `budgets/layer-rules.json`. To exempt one file, add an
`allow` entry with a `why`. An entry that matches no violation fails the gate.

## Adding things

- **A capability:** add it to the core with tests. Then expose it from each
  front end that needs it.
- **A command:** see "Adding a command" in `docs/cli.md`.
- **A verb:** see "Adding a verb" in `docs/service.md`.
- **Something on screen:** get the data in `backend.rs`, decide what to show
  in `scene.rs`, place it in `layout.rs`, and draw it in `platform/`. If
  `platform` needs an `if` about content, the decision is in the wrong module.
- **A front end:** add it as a new consumer of the control socket. Do
  not move logic out of the core to make that easier.
- **A boundary:** add a rule to `budgets/layer-rules.json`, or a test, and a
  row to the table above.
