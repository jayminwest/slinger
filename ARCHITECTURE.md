# Architecture

A map: where things are and which way the dependencies point. It does not
restate the code or the contracts. Each module's `//!` doc says what it
decides and why; the contracts are `docs/service.md` (the socket),
`docs/cli.md` (the CLI's output), `docs/workspace.md` and `docs/design/`.
Update this file in the same commit as any change to a layer, a boundary or
a contract. `cargo xtask check:agents` fails when a path named here in
backticks is gone; every path here is from the repository root.

## Layers

```
crates/core     slinger-core: the domain logic, typed errors. No terminal, process,
                network or front-end dependencies.
crates/control  slinger-control: the control protocol: requests, replies, the wire
                format, the socket client, the drive protocol and the build stamp.
                Does not link the core.
crates/daemon   slingerd: owns the IO the core leaves out (socket, signals, startup,
                the record's SQLite file, canvas files, agent processes) and serves
                the core. No domain logic.
crates/cli      slinger: argv -> one control request -> the output contract.
crates/app      slinger-app: the macOS app. Control requests -> a scene -> one window.
                The bundled app starts the slingerd beside it when none answers.
xtask           the quality gates. Not shipped.
plugin          the Claude Code plugin (skills). The CLI ships the same skills.
```

One request travels this path, and no other:

```
argv ─ crates/cli ──┐
window ─ crates/app ─┤
                     ├─ control::Client ─ socket ─ crates/daemon ─ crates/core
other programs ──────┘   (one JSON line each way)
```

Only the daemon links the core, so each capability has one implementation
and one path to it. No binary crate depends on another; the clients' tests
dev-depend on the daemon's library, so they run against a real socket.

## Where things are

### `crates/core/src`

| Module | Decides |
|---|---|
| `crates/core/src/canvas` | pages of items changed only by ops; the author rule; undo history; folds, item styles (`crates/core/src/canvas/style.rs`: fill, dash, heads, closed), geometry queries, the delta since a sling |
| `crates/core/src/canvas/file` | the canvas file (`docs/design/canvas-file.md`): the append-only log, `Log` and every request on it (put, edit, fold, look, import) |
| `crates/core/src/packet` | what an agent reads from a page (`docs/design/packet.md`): lines, ids, reading order, clusters |
| `crates/core/src/render` | items to PNGs: scale, tiles, text cells; pure and deterministic |
| `crates/core/src/tldraw` | the tldraw importer, import only |
| `crates/core/src/sling.rs` | the sling message an agent receives |
| `crates/core/src/record` | the record's event and blob-id formats (API: an old reader keeps what it does not know) |
| `crates/core/src/workspace.rs` | workspace files (`docs/workspace.md`) and their rules; panes, kept views and profiles in `crates/core/src/workspace/` |
| `crates/core/src/session.rs` | sessions and statuses; listen delivery, Claude Code's stream format and hooks in `crates/core/src/session/` |
| `crates/core/src/ui.rs` | agent UIs and showings (`docs/design/live-items.md`) |
| `crates/core/src/feedback.rs` | notes about slinger itself, as JSON Lines |
| `crates/core/src/error.rs` | the typed errors and their stable codes |

### `crates/control/src`

One row per verb in `crates/control/src/verb.rs` makes the `Request` enum,
ties each request type to its reply type (`Verb`) and lists `OPS`, so
`Client::call` returns that verb's reply. Each verb group has a module
(`crates/control/src/canvas.rs`, `crates/control/src/session.rs`, ...) with
its request and reply types and its `Client` methods. Also there: the line
format (`crates/control/src/wire.rs`), the socket client
(`crates/control/src/client.rs`), where the daemon is
(`crates/control/src/place.rs`), sockets at any path length
(`crates/control/src/unix.rs`), the drive protocol
(`crates/control/src/drive.rs`) and the build stamp every binary carries
(`crates/control/src/build.rs`).

### `crates/daemon/src`

| Module | Owns |
|---|---|
| `crates/daemon/src/lib.rs` | argv, startup, signals; the `--log` file is `crates/daemon/src/log.rs` |
| `crates/daemon/src/server.rs` | the socket: a thread per connection; `crates/daemon/src/server/answer.rs` has one arm per verb, each handler beside it |
| `crates/daemon/src/store.rs` | the record: one SQLite file, its schema and waiting slings; the only module that knows its layout |
| `crates/daemon/src/canvases.rs` | canvas files, the only writer: a lock per canvas, append, sync, reply; the change feed |
| `crates/daemon/src/workspaces.rs` | the open workspaces' files; kept pane views in `crates/daemon/src/views.rs` |
| `crates/daemon/src/sessions.rs` | statuses, routing a sling (listener or process), closing |
| `crates/daemon/src/listeners.rs` | listeners and their long polls |
| `crates/daemon/src/processes.rs` | agent processes: spawn, limits, session ids, which `claude` |
| `crates/daemon/src/feedback.rs` | the feedback file |
| `crates/daemon/src/lock.rs` | the locks that say who owns the socket and the record |

### `crates/cli/src`

`crates/cli/src/cli.rs` turns argv into a `Command` and runs it; one module
per command group (`crates/cli/src/canvas.rs`, `crates/cli/src/drive.rs`, ...)
sends one request each; `crates/cli/src/output.rs` alone decides how data
reaches the terminal (`docs/cli.md`). `crates/cli/src/guide.md` is the agent
guide `slinger guide` prints.

### `crates/app/src`

IO lives in `backend` and `platform`; everything else is pure and unit-tested.

| Module | Decides |
|---|---|
| `crates/app/src/main.rs` | wiring and the run loop; the only caller of `platform` |
| `crates/app/src/route.rs` | where an event goes: an open entry, a pop-up, the keymap, a tap, the pen |
| `crates/app/src/window.rs` | what a command, a tap or an edit does; its parts in `crates/app/src/window` |
| `crates/app/src/commands.rs` | named commands and chords; one row each in `crates/app/src/commands/table.rs`; menus, enablement and context menus beside it |
| `crates/app/src/canvases.rs` | the canvases the panes show: coordinates the three owners below |
| `crates/app/src/canvases/follow.rs` | `Follow`: what each pane follows, its views, and the live, PDF and view-saving threads |
| `crates/app/src/canvases/editing.rs` | `Editing`: ink, selection, shapes, crop, outbox, kept changes, pen, and the Nvim that edits a text (`crates/app/src/canvases/editing/vim.rs`) |
| `crates/app/src/nvim.rs` | the Nvim UI client's protocol, pure: messages out, what the ones in mean, the ready and confirmed rules, the measure that sizes the grid; Nvim's screen in `crates/app/src/nvim/grid.rs`, painted by `crates/app/src/platform/cells.rs`; the child process is `crates/app/src/backend/nvim.rs` |
| `crates/app/src/canvases/flow.rs` | `SlingFlow`: the sling, the slinger thread, slung, prompt, voice, notes, agents |
| `crates/app/src/mirror.rs` | a copy of one canvas page as the daemon holds it, kept by the feed; folds, steps, clips in `crates/app/src/mirror` |
| `crates/app/src/scene.rs` | what the window shows (a `Scene`); ink, selection, shapes, editor, sidebar, footer in `crates/app/src/scene` |
| `crates/app/src/layout.rs` | sizes in, rectangles out; the pane tree in `crates/app/src/layout` |
| `crates/app/src/panes.rs`, `crates/app/src/shared.rs` | the window's panes and focus; the workspace state they share |
| `crates/app/src/backend.rs` | every daemon request and config read; one thread per kind of work in `crates/app/src/backend`, each waking the main thread (`crates/app/src/backend/wake.rs`) |
| `crates/app/src/config.rs` | the config file, text to `Config`; `config.example.toml` lists every key |
| `crates/app/src/launch.rs` | whether and when the app starts `slingerd` |
| `crates/app/src/drive.rs` | a driven window (`slinger-app --drive SOCKET`): drive requests to events |
| `crates/app/src/ui.rs` | an agent UI's page, sandbox and bridge |
| `crates/app/src/platform` | every `unsafe`, objc2 and AppKit call; draws a `Scene` |

### The app's data flow

```
platform (AppKit event, main thread)
  └─ main.rs ─ route.rs ─┬─ command (keymap, menu, palette) ─ window.rs
                         ├─ tap on a button ─────────────────── window.rs
                         └─ pen / pointer ──────────────────── Canvases
Canvases ─┬─ Follow     (pages, views; follower, Lives, Pdfs, Saver threads)
          ├─ Editing    (ink, selection, shapes; Outbox thread -> canvas_apply)
          └─ SlingFlow  (sling entry; Slinger, Voice, Notes threads -> sling_send)
backend threads ─ Event::Wake ─ main.rs drains news ─ scene::build ─ platform draws
```

The owners never hold each other: `Editing` methods take `&Follow`; `Follow`
and `SlingFlow` take only the scene's `Ink`. The pen draws from local state
(P7): ink shows from `Editing` until the feed brings the daemon's copy.

**Threading rule:** AppKit runs on the main thread only. Every `platform`
function that touches AppKit takes a `MainThreadMarker`. Blocking work runs
on a backend thread, which hands its result to the main thread to draw.
Never block inside an AppKit callback.

## Known duplicates

The app does not link the core, so it keeps its own copies of a few core
rules, to answer at every pen event without a round trip. Change both sides
together.

- Hit tests: `crates/app/src/scene/select/hit.rs` against
  `crates/core/src/canvas/geometry.rs` (`items_erased`, `items_in_lasso`).
- Folds: `crates/app/src/mirror/fold.rs` (what a folded box hides, its
  miniature) against `crates/core/src/canvas/fold.rs`; fold-mark geometry in
  `crates/app/src/canvases/fold.rs` (`on_mark`, `side`, `contains`) and
  `holds` in `crates/app/src/canvases/step.rs`.
- What a stepped-into box shows: `crates/app/src/mirror/step.rs` against the
  core's `Canvas::contents` (wholly inside, or a folded box's list).
- Bound arrows following a move or resize: `follow_frames` in
  `crates/app/src/scene/shapes/bind.rs` against the core's `Log::followed`.
- The page as shown: `shown_in` and `present` in
  `crates/app/src/canvases/step.rs` and `pane_shown` in
  `crates/app/src/canvases/find.rs` against the core's `Canvas::visible`.
- Drawing order: `crates/app/src/scene/ink/shown.rs` puts pdf items first,
  as the core's `Canvas::drawn` does.
- Reading order (likely a copy, unconfirmed): `crates/app/src/scene/select/keys.rs`
  against the packet's order in `crates/core/src/packet`.
- Text cells: `CELL` in `crates/app/src/scene/glyphs.rs` against the core's
  `render::CELL`; the rows a text wraps to, read again in the mirror.
- The canvas file's item objects: read in `crates/app/src/mirror.rs` and
  written in `crates/app/src/mirror/write.rs`, against
  `crates/core/src/canvas/file`.

## Boundaries

Dependencies point inward: front ends depend on the core, never the other
way. Each capability has one implementation, in the core; a front end that
reimplements logic is a bug (the list above is the known exceptions). Every
failure a caller can act on is a typed error with a stable code; front ends
map codes, they never parse messages.

Each boundary has something that fails when it is broken. The rules are in
`budgets/layer-rules.json` (`cargo xtask check:layers`); to exempt one file,
add an `allow` entry with a `why`.

| Boundary | Held by |
|---|---|
| The core does no process, terminal or network IO and no FFI | rule `core-is-pure` |
| The protocol crate does not link the core | rule `control-is-transport-only` |
| The CLI reaches the core only through the socket | rule `cli-uses-the-socket` |
| The app reaches the core only through the socket | rule `app-uses-the-socket` |
| Only `crates/app/src/platform` holds `unsafe`, objc2 or AppKit code | rule `platform-owns-ffi` |
| Only `crates/app/src/main.rs` starts the platform | rule `app-logic-is-pure` |
| Only `crates/app/src/platform/pdf.rs` reads a PDF | rule `pdf-lives-in-platform-pdf` |
| Drive lines are only `DriveAsk` and `DriveReply`, never hand-built JSON | rule `drive-speaks-typed`; `crates/control/tests/fixtures/drive.jsonl` |
| `Follow`, `Editing` and `SlingFlow` hold no `impl Canvases` | rule `canvas-owners-are-not-canvases` |
| Every verb's request and reply line is byte-stable; `docs/service.md`'s verb table matches `OPS` | `crates/control/tests/golden` |
| The wire format | the tests of `crates/control/src/wire.rs` |
| The record's formats are readable by older readers | `crates/core/tests/record.rs` |
| The canvas file is readable by older readers | `crates/core/tests/canvas_file.rs` |
| The packet's lines, images and read-back | `crates/core/tests/packet.rs`, `crates/core/tests/render.rs`, `crates/core/tests/packet_build.rs` |
| The sling message | `crates/core/tests/sling.rs` |
| The workspace file format and rules | the tests of `crates/core/src/workspace.rs` |
| The guide, the skills and the sling note name only real commands and flags | `crates/cli/src/tests/guide.rs` |
| Every command's name, title, group, key and menu place | `crates/app/src/commands/tests/golden.txt` |
| Layout is fully tested | the 100% floor on `crates/app/src/layout.rs` in `budgets/coverage-budgets.json` |

Convention only (no gate): the daemon holds no domain logic; IO in the app
lives in `backend` and `platform`; AppKit runs on the main thread (the
`MainThreadMarker` makes most of it a compile error). Behaviour inside a
module is held by its tests; their `//!` docs say what each one covers.

## Adding things

- **A capability:** in the core with tests, then exposed by each front end
  that needs it.
- **A verb:** 5 places: the core, the module's request and reply types and
  `Client` method, one `verbs!` row in `crates/control/src/verb.rs`, the
  `answer` arm, a daemon test (and the row in `docs/service.md`'s table,
  which the golden test enforces). See "Adding a verb" in `docs/service.md`.
- **A CLI command:** see "Adding a command" in `docs/cli.md`.
- **A drive op:** a `DriveAsk` variant and its `DRIVE_OPS` entry in
  `crates/control/src/drive.rs`, the CLI's `Op` and `request` arm in
  `crates/cli/src/drive.rs`, the `parse` arm in `crates/app/src/drive.rs`,
  and a line in `crates/control/tests/fixtures/drive.jsonl`.
- **An app command:** 7 places: the `Command` variant in
  `crates/app/src/commands.rs`, its row in `crates/app/src/commands/table.rs`,
  an item in one menu of `crates/app/src/commands/menu.rs`, an arm in
  `can` in `crates/app/src/commands/able.rs` (no `_`) with its entry in
  `crates/app/src/commands/able/tests.rs`, a line in `config.example.toml`,
  and its handler. Then `SLINGER_BLESS=1 cargo test -p slinger-app golden`
  and read the diff of `crates/app/src/commands/tests/golden.txt`.
- **A command a person would use:** it comes to the app too, as a command
  or a tap target, or with a row in `docs/app-parity.md` saying why not.
- **A setting:** the key, its default and its test in
  `crates/app/src/config.rs`, and the line in `config.example.toml`.
- **Something on screen:** get the data in `backend`, decide what to show in
  `scene`, place it in `layout`, draw it in `platform`. If `platform` needs
  an `if` about content, the decision is in the wrong module.
- **A front end:** a new client of the control socket. Do not move logic out
  of the core to make that easier.
- **A boundary:** a rule in `budgets/layer-rules.json` or a test, and a row
  in the table above.
