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
record/      the record's formats (SPEC section 9): events as versioned JSON
             (encode, decode) and blob ids (SHA-256, `sha256:<hex>`). No IO;
             the daemon stores what it encodes.
workspace.rs workspaces (SPEC section 10): the definition file's format
             (docs/workspace.md) and the rules a set of them keeps. No IO;
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
server.rs    the control socket and `answer`, one arm per verb.
store.rs     the record on disk: one SQLite file (rusqlite, bundled) with
             the event log and the blobs. The only module that knows its
             layout. Stores each event as the line `record::encode` wrote.
```

### Inside the app

```
main.rs      wires the modules below. The only caller of platform.
backend.rs   gets the data: the one module that talks to the daemon.
scene.rs     decides what the window shows. Pure.
scene/canvas.rs  decides what a canvas pane shows: a page's items and a viewport in,
             the visible marks in screen points and the box to repaint out. Pure.
layout.rs    geometry: sizes in, rectangles out, and the canvas viewport (pan, zoom
             about a point, limits; the canvas has no edge). Pure. 100% coverage floor.
platform/    draws a scene. All unsafe, objc2 and AppKit code. No decisions.
```

**Threading rule:** AppKit runs on the main thread only. Every `platform`
function that touches AppKit takes a `MainThreadMarker`, which only the main
thread can make, so the compiler holds the rule. Blocking work (files,
sockets) runs before the run loop starts or on another thread, which hands
its result to the main thread to draw. Never block inside an AppKit callback.
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
| The wire format (docs/service.md) | the tests of `crates/control/src/wire.rs` |
| The workspace file format and rules (docs/workspace.md) | the tests of `crates/core/src/workspace.rs` and its stored fixture |
| The record on disk is append-only and keeps fields it does not know | `crates/daemon/src/store/tests.rs`, against a real file |
| One daemon per socket; a stop loses no reply | `crates/daemon/src/tests.rs`, through a real socket |
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
