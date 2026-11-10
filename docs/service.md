# Service contract

`slingerd` serves the core over a local control socket. Programs, scripts
and agents rely on this contract. Breaking it is a breaking change.

## The daemon

```
slingerd [--socket <PATH>] [--data-dir <PATH>]
```

- It says nothing on success and runs until SIGTERM or SIGINT.
- **One instance per socket path.** The daemon holds a lock on
  `<socket>.lock` for its whole life. A second daemon on the same path exits
  4 with `✗ socket <path> is in use by another slingerd` and a hint. A
  socket file left by a daemon that died is replaced.
- **Clean stop.** On SIGTERM or SIGINT the daemon stops accepting, answers
  every request on a connection made before the signal, removes the socket
  file and exits 0. The lock file stays. It is empty and safe to delete while
  no daemon runs.
- **Errors** follow `docs/cli.md`: `✗ message` and an indented hint on
  stderr, and a typed exit code.

| Code | Name | Meaning |
|---|---|---|
| 0 | ok | Stopped by a signal |
| 1 | error | Unexpected failure, such as a socket path or data directory that cannot be created |
| 2 | usage | Bad arguments |
| 4 | conflict | Another daemon serves the socket, or the record was laid out by a newer slingerd |

## Data directory

The daemon keeps this machine's record (SPEC section 9) in one SQLite file,
`record.sqlite`, in its data directory. It is the file's only reader and
writer; other programs reach the record through the socket.

- Default: `$XDG_DATA_HOME/slinger`. When `XDG_DATA_HOME` is not set:
  `~/Library/Application Support/slinger` on macOS and
  `~/.local/share/slinger` elsewhere. `--data-dir <PATH>` overrides it.
- The daemon creates the directory (mode `0700`) and the file at startup,
  after it takes the socket. A directory it cannot create stops it with
  exit 1 and a hint.
- The event log is append-only: the file refuses updates and deletes. The
  daemon gives each event its number and time and tags it with this
  machine's name, the host name up to its first dot. An event the writer
  gave no workspace tag gets the tag of the open workspace whose canvas is
  its `canvas` tag, else of the one whose agent is its `agent` tag; none
  when no open workspace, or more than one, matches.
- Blobs are keyed by their id (`sha256:<hex>`). Storing the same bytes again
  stores nothing new.
- The file's layout has a version. A daemon refuses a file laid out by a
  newer one (exit 4).

The open workspaces are listed in `workspaces/open.json`, and a workspace
with no root keeps its file in `workspaces/<id>/` (see "Workspaces" below).

Canvases live beside the record, one file per canvas:
`canvases/<id>.canvas`, in the format of `docs/design/canvas-file.md`. The
daemon is their only writer (SPEC K5). It makes the directory (mode `0700`)
with the first canvas, reads a canvas on first use and keeps it open. A torn
last line (a crash in the middle of a write) is cut off the file, with a line
on stderr; nothing acknowledged is lost.

## Socket

- A Unix stream socket, mode `0600`: only the daemon's user can connect.
- Default path: `$XDG_RUNTIME_DIR/slinger.sock`. When `XDG_RUNTIME_DIR` is
  not set: `slinger.sock` in the temporary directory (`$TMPDIR`, per user on
  macOS). The daemon and every client take `--socket <PATH>`.
- There is no network transport.

## Protocol

One connection carries one request and one reply. Each is one line of JSON
(UTF-8, ended by a newline). Try it:

```bash
echo '{"v":1,"op":"hello","name":"ada"}' | nc -U "$SOCKET"
```

**Request:** `v` is the protocol version, `op` names the verb, and the other
fields are the verb's arguments.

```json
{ "v": 1, "op": "hello", "name": "ada" }
```

**Reply:** `ok` and then the verb's result, or the error. `code` is stable
and `error` is for people. Branch on `code`, never on `error`.

```json
{ "ok": true, "greeting": "hello, ada" }
{ "ok": false, "code": "invalid", "error": "name is empty", "hint": "pass a non-empty name" }
```

- A request line is at most 65536 bytes. The daemon reads a longer line to
  its end, then answers `too_large`.
- A request must arrive within 5 seconds of the connection. A connection
  that closes without a request gets no reply.
- The daemon ignores fields it does not know.

### Verbs

| `op` | Arguments | Result |
|---|---|---|
| `hello` | `name`: string | `greeting`: string |
| `record_append` | `kind`: string, `source`: string, `tags`: object (optional), `payload`: object (default `{}`) | `event`: the event as stored |
| `record_list` | `kind`, `machine`, `workspace`, `agent`, `canvas`: string (each optional); `after`: number (optional) | `events`: list of events, lowest number first |
| `blob_put` | `path`: absolute path of a file | `id`: the blob id |
| `blob_get` | `id`: blob id, `path`: absolute path of a file | `size`: number of bytes written |
| `ui_put` | `name`: string, `path`: absolute path of an HTML file, `source`: string | `ui`: the version, `new`: whether this put made it |
| `ui_list` | none | `uis`: the latest version of each UI, by name |
| `ui_show` | `name`: string | `uis`: every version of that UI, oldest first |
| `ui_export` | `ui`: a UI reference, `dir`: absolute path of a directory | `ui`: the version, `path`: the file written |
| `canvas_create` | `canvas`: canvas id | `canvas`: the id |
| `canvas_list` | none | `canvases`: list of `{canvas}`, sorted by id |
| `canvas_show` | `canvas`: canvas id, `page`: page id or name (optional, default the first page) | `canvas`, `next`: the next operation's number, `page`: the page's id, `pages`: list of `{id, name}`, `items`: list of items |
| `canvas_apply` | `canvas`: canvas id, `by`: author (default `user`), `ops`: one operation or a list | `seq`: list of numbers, `made`: list of ids |
| `canvas_undo`, `canvas_redo` | `canvas`: canvas id, `by`: author (default `user`) | `seq`, `made` (always empty) |
| `canvas_import` | `path`: absolute path of a tldraw file, `canvas`: canvas id (optional, default made from the file's name) | `canvas`, `pages`: number, `items`: count by kind, `skipped`: count by tldraw shape type |
| `workspace_open` | `dir`: absolute path of a directory (optional), `name`: string (optional); one of them | `workspace`: the workspace, `new`: whether this open made its file |
| `workspace_list` | none | `workspaces`: list of workspaces, sorted by name |
| `workspace_show` | `workspace`: a name, or the absolute directory of its file | `workspace` |
| `workspace_close` | `workspace`: as `workspace_show` | `dir`: the directory of its file |

**`record_append`** appends one event to this machine's record. `tags`
may set `workspace`, `agent` and `canvas`; the daemon sets `machine`, the
event's `number` and its `time` (milliseconds since the Unix epoch, UTC).
The kind must be one this daemon writes, and the payload must fit it, or
the reply is `invalid` with a hint. A payload field the kind does not have
is dropped.

```json
{ "v": 1, "op": "record_append", "kind": "agent_done", "source": "scout", "tags": { "agent": "scout" }, "payload": { "summary": "wrote the plan" } }
{ "ok": true, "event": { "number": 1, "time": 1762790400000, "kind": "agent_done", "version": 1, "source": "scout", "tags": { "machine": "my-laptop", "agent": "scout" }, "payload": { "summary": "wrote the plan" } } }
```

| `kind` | `payload` |
|---|---|
| `sling_sent` | `prompt`: string, `packet`: blob id |
| `reply_landed` | `sling`: number (the `sling_sent` event), `items`: list of item ids |
| `file_touched` | `path`: string, `hash`: blob id (never the contents) |
| `pane_moved` | `pane`: string, `x`, `y`, `width`, `height`: number (points) |
| `agent_blocked` | `reason`: string |
| `agent_done` | `summary`: string (optional) |

A blob id is `sha256:` and 64 lowercase hex digits.

`record_list` also returns `ui_put` events (`name`, `version`, `hash`: blob
id, `bytes`), but `record_append` refuses that kind: only `ui_put` writes
it, so a name's versions stay 1, 2, 3 in log order.

**`record_list`** returns the events that match every filter given:
`kind`, each tag, and `after` (only events numbered above it). No match
is an empty list, not an error. An event of a kind this daemon does not
know is returned as it was stored, unknown fields included.

**`blob_put`** stores the bytes of the file at `path` and returns their
id. Putting bytes already stored stores nothing new and returns the same
id. **`blob_get`** writes the bytes of the blob `id` to the file at `path`,
creating or replacing it. A blob this record does not hold is `not_found`,
and the file is not touched.

```json
{ "v": 1, "op": "blob_put", "path": "/tmp/packet.json" }
{ "ok": true, "id": "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" }
{ "v": 1, "op": "blob_get", "id": "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "path": "/tmp/copy.json" }
{ "ok": true, "size": 3 }
```

A path that is not absolute is `invalid`: the daemon's working directory is
not the client's. A file to put that does not exist is `not_found`; one the
daemon cannot read or write is `invalid`. Both carry a hint.

**`ui_put`** stores the file at `path` as a blob and, unless it holds the
same bytes as the name's latest version, appends a `ui_put` event that
makes the next version (1 for a new name). Every version stays. The bytes
of an older version make a new version: a revert is a change like any
other. The rules for the name and the code are in
`docs/design/live-items.md`; a name or code they refuse is `invalid`.
**`ui_export`** writes one version into `dir` (created if missing) as
`<name>.v<version>.html`, replacing a file of that name. A reference is
`<name>` (the latest version), `<name>@<version>` or a blob id (the latest
version holding those bytes). A name with no versions, or a reference that
matches none, is `not_found` with a hint.

A version is `{name, version, hash, bytes, source, time}`: `hash` is the
code's blob id, `source` who put it, `time` when (milliseconds since the
Unix epoch, UTC). The versions are rebuilt from the `ui_put` events on each
request; they are not stored anywhere else.

```json
{ "v": 1, "op": "ui_put", "name": "pick-slot", "path": "/tmp/pick-slot.html", "source": "scout" }
{ "ok": true, "ui": { "name": "pick-slot", "version": 1, "hash": "sha256:9f2c…", "bytes": 1234, "source": "scout", "time": 1762790400000 }, "new": true }
{ "v": 1, "op": "ui_export", "ui": "pick-slot@1", "dir": "/tmp/out" }
{ "ok": true, "ui": { "name": "pick-slot", "version": 1, … }, "path": "/tmp/out/pick-slot.v1.html" }

#### Canvases

A **canvas id** is 1 to 64 of `a-z`, `0-9`, `-`, `_` and `.`, not starting
with `.`. It names the canvas's file, so anything else is `invalid`. A new
canvas has one page, `p1`, named `Page 1`. Creating a canvas that exists is
`conflict`; any other verb on a canvas that does not exist is `not_found`, with
a hint that says how to create it. A canvas file that is damaged or written by
a newer slinger is `canvas_corrupt` or `canvas_too_new`, and the daemon does
not write to it.

**One writer.** Every change to one canvas goes through one lock in the
daemon. Each operation the canvas writes gets the next number of that canvas,
`1, 2, 3, ...` with no gap, in the order the changes apply, and its line is
synced to disk before the reply is sent. Changes from clients at the same time
all succeed, one after another. `seq` in the reply lists the numbers this
change wrote; a change that alters nothing (removing an item already gone, an
undo with nothing to undo) writes nothing and succeeds.

**Authors.** `by` is `user` or `agent:<name>`. The user may change anything.
An agent may move, edit or remove only items it made, and gets `invalid` with a
hint for anything else. That stays `invalid` rather than a code of its own:
the caller does the same thing either way (make a new item instead), and a
new code would need a new CLI exit code (docs/cli.md: add one only when a
caller must tell the failure apart). Undo and redo are per author and live in
the daemon's memory: a restart starts with nothing to undo.

**Canvas operations.** `ops` of `canvas_apply` is one operation object or a
list of them, applied in order as one undo step: all of them apply, or none
do. Ids, items and units are as in the canvas file (`docs/design/canvas-file.md`):
item ids `i<n>`, page ids `p<n>`, and positions and lengths as integers in
tenths of a unit.

| `op` | fields |
|---|---|
| `add` | `item`: an item object with `page`, `kind` and the kind's fields, without `id`; `color` defaults to `#000000` |
| `remove` | `ids`: list of item ids. Ids already gone are fine |
| `move` | `ids`, `dx`, `dy` |
| `set` | `id`, `fields`: item fields to replace, laid over the item's current ones |
| `page.add` | `name`. The page goes last |
| `page.rename` | `page`, `name` |
| `page.remove` | `page`. Its items go with it; the last page stays |

`made` lists the ids the `add` and `page.add` operations made, in order. An
operation of another kind, or one whose fields do not fit, is `invalid` with
a hint. `canvas_show` gives a page's items in drawing order as the file's
whole item objects: `id`, `page`, `by`, `t` (when it was made), `kind`,
`color` and the kind's fields.

```json
{ "v": 1, "op": "canvas_apply", "canvas": "main", "by": "agent:scout", "ops": [{ "op": "add", "item": { "page": "p1", "kind": "text", "x": 0, "y": 0, "text": "plan" } }] }
{ "ok": true, "seq": [7], "made": ["i5"] }
```

**`canvas_import`** reads the tldraw file at `path` (a `.tldraw` file or an
older `.tldr`; its bytes travel as a file, as a blob's do) and makes a new
canvas that holds it: tldraw's pages with their names, and its drawings, text,
rectangles and arrows as items, all made by `user`. The file is written whole
before the reply, so a crash leaves the canvas complete or absent. Without
`canvas`, the id is the file's name lowercased, with each run of other
characters made one `-` (`My Board.tldraw` is `my-board`). `items` counts what
it made by kind (`stroke`, `text`, `box`, `arrow`; a rectangle's label is a
`text`), and `skipped` counts the shapes with no item by tldraw type
(`group`, `line`, `geo:ellipse`, ...). A file that is not there is
`not_found`; one that is not a tldraw file is `invalid`; a canvas that exists
is `conflict`.

```json
{ "v": 1, "op": "canvas_import", "path": "/tmp/idea.tldraw" }
{ "ok": true, "canvas": "idea", "pages": 5, "items": { "arrow": 57, "box": 80, "stroke": 7563, "text": 125 }, "skipped": { "group": 8, "line": 4 } }
```

#### Workspaces

A workspace is defined by its file, `slinger-workspace.toml`
(`docs/workspace.md`). The daemon keeps the list of open workspaces, the
directories that hold their files, in `workspaces/open.json` in its data
directory, so they stay open across a restart. It reads each file on every
request: an edit to one shows at once.

**`workspace_open`** with `dir` opens that directory (links resolved). With no
file there, it writes one: the name is `name` or else the directory's last
part, the directory is the primary root on this machine, and the canvas is
the name made into an id (`My Notes` gives `my-notes`). With no `dir`, it
opens the workspace with no root named `name`, whose file lives in
`workspaces/<id>/` in the data directory. Either way the daemon then makes
the workspace's canvas if it is missing. Opening an open workspace returns it
with `new: false`, and so does opening one whose file exists. A `name` that
differs from the name in an existing file is `conflict`.

Every open workspace keeps the rules of `docs/workspace.md` with the others:
a second workspace on one primary root, a second workspace of one name, or a
second workspace on one canvas is `conflict`, with a hint. A refused open
writes nothing. A `dir` that is not there is `not_found`; one that is not an
absolute directory, or a request with neither `dir` nor `name`, is `invalid`.

A workspace is `{name, tag, canvas, dir, agent, roots}`: `dir` is the
directory of its file, `agent` (absent when it has none) is `{name, durable}`
and `roots` is a list of `{machine, path, primary}`.

**`workspace_show`** and **`workspace_close`** take the workspace's name, or
the absolute directory of its file. One that is not open is `not_found`.
**`workspace_list`** fails with the error of a file it cannot read, and a hint
to close that workspace by its directory, which works whatever the file holds.
Closing removes the workspace from the list only: its file, its canvas and
its agent stay.

```json
{ "v": 1, "op": "workspace_open", "dir": "/Users/me/notes" }
{ "ok": true, "workspace": { "name": "notes", "tag": "notes", "canvas": "notes", "dir": "/Users/me/notes", "roots": [{ "machine": "my-laptop", "path": "/Users/me/notes", "primary": true }] }, "new": true }
```

#### How a blob's bytes travel

The bytes travel as a file on this machine, named in the line. They never
travel in the line itself. The other choice was base64 in the line. Files
won because:

- **Size.** A request line is at most 65536 bytes, and blobs (packets with
  images, UI code) run to megabytes. Base64 would need a far larger limit for
  every verb, a limit that guards the daemon against a client that never
  stops writing, and it adds a third to every blob.
- **Small clients.** The control crate needs no encoder, and the app binary
  that links it has a size budget.
- **It costs nothing.** The socket is local and only its owner can connect
  (mode `0600`), so the client and the daemon are the same user on the same
  machine and see the same files with the same rights.

A client that holds bytes in memory writes them to a file of its own first,
and a client that wants them in memory reads the file the daemon wrote. The
CLI does this for `blob put -`, `ui put -` and for `blob get` without `--out`, in the
temporary directory, and removes the file after.

### Error codes

The core's codes pass through unchanged (`not_found`, `conflict`, `invalid`, for
a damaged or newer canvas file `canvas_corrupt` and `canvas_too_new`, and for a
packet of a version the core does not read `unsupported_version`).
The protocol adds:

| Code | Meaning |
|---|---|
| `bad_request` | The line is not JSON, has no `v`, names no known `op`, or has wrong arguments |
| `too_large` | The request line is over the limit |
| `unsupported_version` | `v` is not a version this daemon speaks |
| `unavailable` | Made by the client, never sent: the daemon did not answer |
| `internal` | The daemon could not read or write its record, or the client could not read the reply |

`not_found` also covers a blob the record does not hold, a file to put
that is not there, a UI with no such name or version, and a canvas, page or
item that is not there, and a directory or workspace that is not there or
not open.

## Versioning

- `v` is `1`. Within a version, changes only add: a new verb, a new optional
  argument, a new result field. Clients ignore result fields they do not
  know, and the daemon ignores arguments it does not know.
- Removing or renaming anything, changing a type, or making an argument
  required is a new version. Raise `VERSION` in
  `crates/control/src/wire.rs`. A daemon answers a version it does not speak
  with `unsupported_version`.
- Error codes are never renamed.

## Adding a verb

1. Add the capability to `crates/core` with tests.
2. In `crates/control/src/lib.rs`, add a `Request` variant, a reply type and
   its `Reply` variant. Add a typed method to `Client` in
   `crates/control/src/client.rs`. Every variant `Request` serializes adds
   code to the app, which links the client: a verb the app never sends can
   be `#[serde(skip_serializing)]` there and written by a small type of its
   own, as `crates/control/src/workspace.rs` does, with a test that the two
   write the same line.
3. Add one arm to `answer` in `crates/daemon/src/server.rs`. It calls the
   core and nothing else.
4. Add tests to `crates/daemon/src/tests.rs`. They go through a real socket.
5. Add the verb to the table above, then expose it from each client that
   needs it.

## Running it as a system service

The template ships no launchd or systemd unit. To add one, run
`slingerd --socket <PATH> --data-dir <PATH>` as the user who owns the data, restart it on
failure, and stop it with SIGTERM. The daemon does not fork and does not
write a pid file, so use the service manager's simple (foreground) type. Do
not use socket activation: the daemon creates and locks its own socket.
