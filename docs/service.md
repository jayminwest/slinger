# Service contract

`slingerd` serves the core over a local control socket. Programs, scripts
and agents rely on this contract. Breaking it is a breaking change.

## The daemon

```
slingerd [--socket <PATH>] [--data-dir <PATH>] [--claude <PATH>] [--slinger <PATH>]
```

- It says nothing on success and runs until SIGTERM or SIGINT.
- `--claude` is the program an agent's process runs (see "Sessions"):
  Claude Code, `claude` on `PATH` by default. `--slinger` is the CLI its
  hooks run (see "Sessions", "Hooks"): the `slinger` beside `slingerd`, else
  `slinger` on `PATH`, by default.
- **One instance per socket path.** The daemon holds a lock on
  `<socket>.lock` for its whole life. A second daemon on the same path exits
  4 with `✗ socket <path> is in use by another slingerd` and a hint. A
  socket file left by a daemon that died is replaced.
- **Clean stop.** On SIGTERM or SIGINT the daemon stops accepting, answers
  every request on a connection made before the signal, removes the socket
  file and exits 0. A request waiting on a canvas feed gets its reply at
  once, with what it has. The agents' processes end with it. The lock file stays. It is empty and safe to delete while
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
- `packets/<hex>/` holds each packet `canvas_read` wrote, named by the hex
  of its text's blob id: `packet.txt` and its images. It is derived from
  the record and safe to delete; the next read writes it again.
- The file's layout has a version. A daemon refuses a file laid out by a
  newer one (exit 4).

`sessions.json` keeps each agent's session id, `{agent: id}`, so a start
after a restart resumes the conversation (see "Sessions").

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
| `ui_open` | `ui`: a UI reference, `agent`: string, `host`: `pane` (default) or `canvas`, `data`: any JSON (optional). A pane: `workspace`: as `workspace_show`, `beside`: a pane number (optional), `below`: bool (default `false`). A canvas: `canvas`, `item`: the item it goes beside, `size`: `[w, h]` in tenths (default `[3200, 2400]`) | `open`: the showing's id, `ui`: the version, `workspace`, `pane`: the new pane's number, or `item`: the live item (`canvas`, `item`, `page`, `x`, `y`, `w`, `h`, `seq`), `event`: the `ui_opened` appended |
| `ui_shown` | `ui`: a showing's id or a UI reference | `ui`: the version, then for a showing `open`, `data` (absent when `null`) and `agent` |
| `ui_reply` | `open`: a showing's id, `value`: any JSON | `event`: the `ui_reply` appended, `agent`, `delivered`: whether its session took it |
| `canvas_create` | `canvas`: canvas id | `canvas`: the id |
| `canvas_list` | none | `canvases`: list of `{canvas}`, sorted by id |
| `canvas_show` | `canvas`: canvas id, `page`: page id or name (optional, default the first page) | `canvas`, `next`: the next operation's number, `page`: the page's id, `pages`: list of `{id, name}`, `items`: list of items |
| `canvas_apply` | `canvas`: canvas id, `by`: author (default `user`), `ops`: one operation or a list | `seq`: list of numbers, `made`: list of ids |
| `canvas_undo`, `canvas_redo` | `canvas`: canvas id, `by`: author (default `user`) | `seq`, `made` (always empty) |
| `canvas_import` | `path`: absolute path of a tldraw file, `canvas`: canvas id (optional, default made from the file's name) | `canvas`, `pages`: number, `items`: count by kind, `skipped`: count by tldraw shape type |
| `canvas_changes` | `canvas`: canvas id, `after`: number (default 0), `wait`: milliseconds (default 0, at most 60000) | `next`: the number after the last op given, `ops`: list of operation lines |
| `workspace_open` | `dir`: absolute path of a directory (optional), `name`: string (optional); one of them | `workspace`: the workspace, `new`: whether this open made its file |
| `workspace_list` | none | `workspaces`: list of workspaces, sorted by name |
| `workspace_show` | `workspace`: a name, or the absolute directory of its file | `workspace` |
| `workspace_close` | `workspace`: as `workspace_show` | `dir`: the directory of its file |
| `pane_list` | `workspace`: as `workspace_show` | `workspace`: its name, `layout`: the pane tree |
| `pane_open` | `workspace`, `kind`: `canvas` or `ui`, `target`: canvas id or UI name, `beside`: a pane number (optional), `below`: bool (default `false`) | `workspace`, `layout`, `pane`: the new pane's number, `event`: the `pane_moved` appended |
| `pane_close` | `workspace`, `pane`: a pane number | `workspace`, `layout`, `event`: the `pane_moved` appended (absent when nothing changed) |
| `pane_resize` | `workspace`, `pane`: a pane number, `ratio`: number | `workspace`, `layout`, `event`: the `pane_moved` appended (absent when nothing changed) |
| `canvas_read` | `canvas`: canvas id, `page`: page id or name (optional, default the first page), `after`: number (optional, default the last sling's), `select`: list of item ids (optional), `out`: absolute path of a directory (optional) | `packet`: blob id of the text, `dir`: the directory written, `text`: the packet with `at`, `images`: list of `{name, id}` |
| `canvas_crop` | `canvas`: canvas id, `of`: an item id or a box, `path`: absolute path of a PNG file | `images`: list of `{path, x, y, width, height}` |
| `canvas_strokes` | `canvas`: canvas id, `item`: an item id | `strokes`: list of stroke items with `points` |
| `session_list` | none | `sessions`: list of sessions, sorted by agent |
| `session_show` | `agent`: string | `session` |
| `status_set` | `agent`: string, `status`: `working`, `blocked` or `done`, `note`: string (optional) | `agent`, `status`, `note` (absent when none), `event`: the number of the event appended (absent for `working`) |
| `canvas_put` | `canvas`: canvas id, `agent`: string, `beside`: an item id, then one of `text`: string, `box`: `[w, h]` in tenths, `arrow`: `[from, to]` item ids | `item`: the new item's id, `page`: its page's id, `x`, `y`, `w`, `h`: its box in tenths, `seq`: the operation's number, `event`: the `reply_landed` appended (absent when none) |
| `canvas_edit` | `canvas`: canvas id, `agent`: string, `item`: an item id, then any of `text`: string, `box`: `[w, h]` in tenths, `move`: `[x, y]` in tenths | `item`, `page`, `x`, `y`, `w`, `h`: its box now, `seq`: the numbers written (empty when nothing changed) |
| `canvas_remove` | `canvas`: canvas id, `agent`: string, `item`: an item id | `seq`: the numbers written (empty when it was already gone) |
| `session_start` | `workspace`: string, its name or the directory of its file | `session` |
| `session_stop` | `workspace`: string | `session` |
| `session_send` | `workspace`: string, `text`: string | `session` |
| `sling_send` | `canvas`: canvas id, `page`: page id or name (optional, default the first page), `select`: list of item ids (optional), `prompt`: string (default empty) | `event`: the `sling_sent` appended, `packet`: blob id of the text, `from`, `to`: the canvas operations it covers, `strokes`: how many of the user's strokes were new, `agent`, `session_id` |
| `file_touched` | `agent`: string, then one of `path`: absolute path of a file, `hook`: a Claude Code `PostToolUse` hook's payload, as a string | `agent`, `path`, `hash`: the file's blob id, `event`: the `file_touched` appended (all three absent when the payload names no file) |

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
| `sling_sent` | `prompt`: string, `packet`: blob id, `images`: list of `{name, hash}` (optional), `session`: the session id it went into (optional) |
| `reply_landed` | `sling`: number (the `sling_sent` event), `items`: list of item ids |
| `file_touched` | `path`: string, `hash`: blob id (never the contents) |
| `pane_moved` | `pane`: string, `x`, `y`, `width`, `height`: number (points; from the pane verbs, the pane's share of the window, see "Panes") |
| `agent_blocked` | `reason`: string |
| `agent_done` | `summary`: string (optional) |

A blob id is `sha256:` and 64 lowercase hex digits.

`record_list` also returns `ui_put` events (`name`, `version`, `hash`: blob
id, `bytes`), but `record_append` refuses that kind: only `ui_put` writes
it, so a name's versions stay 1, 2, 3 in log order. The same holds for
`ui_opened` (`name`, `version`, `hash`, `host`, `data`), which only
`ui_open` writes, and `ui_reply` (`open`, `name`, `version`, `hash`,
`value`), which only `ui_reply` writes (see "Showings").

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
```

#### Showings

A **showing** is one `ui_open`: an agent shows a version of a UI, with
data, and each reply from it goes back to that agent's session. Its id is
`<name>#<number>`, the number of its `ui_opened` event (`pick-slot#12`).
No UI reference holds a `#`, so a pane's target is either.

**`ui_open`** resolves `ui` to one version, checks the agent (not blank),
the host (`pane` or `canvas`), the data (at most 16384 bytes as JSON) and
the workspace's window (`beside` there, room for a pane), then appends a
`ui_opened` event tagged with the agent and the workspace and opens a pane
whose target is the showing's id, as `pane_open` does. With `host`
`canvas` it checks the put instead (`item` on the canvas, a size above 0),
then, under the canvas's lock, appends the `ui_opened` event tagged with
the agent and the canvas and adds a live item (`kind` `live`, with `ui`,
`hash` and `open`, `docs/design/canvas-file.md`) beside `item`, as
`canvas_put` places, one undo step. A refused open records nothing.
**`ui_shown`** says what a pane's UI target shows: for a showing, the
version it opened, its data and its agent; for a reference, the version
alone. The app asks it for each UI pane and each live item it shows.

**`ui_reply`** appends a `ui_reply` event (source `user`, the showing's
tags), then sends the agent's running session one message: the line
`{"v":1,"kind":"ui_reply","ui","version","hash","open","value"}`, JSON on
one line. A session that is not running does not get it: the reply is
still in the record, and `delivered` is `false`. A value over 16384 bytes,
or a showing's 101st reply, is `invalid` with a hint, and records nothing.
A showing id that names no `ui_opened` event is `not_found`; a target that
is not a showing id is `invalid`.

```json
{ "v": 1, "op": "ui_open", "ui": "pick-slot", "workspace": "notes", "agent": "scout", "data": { "slots": ["Tue 10:00", "Wed 14:00"] } }
{ "ok": true, "open": "pick-slot#12", "ui": { "name": "pick-slot", "version": 1, … }, "workspace": "notes", "pane": 2, "event": 12 }
{ "v": 1, "op": "ui_open", "ui": "pick-slot", "agent": "scout", "host": "canvas", "canvas": "notes", "item": "c3" }
{ "ok": true, "open": "pick-slot#14", "ui": { "name": "pick-slot", "version": 1, … }, "workspace": "notes", "item": { "canvas": "notes", "item": "i15", "page": "p1", "x": 1240, "y": 0, "w": 3200, "h": 2400, "seq": 9 }, "event": 14 }
{ "v": 1, "op": "ui_reply", "open": "pick-slot#12", "value": { "slot": "Wed 14:00" } }
{ "ok": true, "event": 13, "agent": "scout", "delivered": true }

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
| `add` | `item`: an item object with `page`, `kind` and the kind's fields, without `id`; `color` defaults to `#000000`. A `text` without `w` or `h` gets a frame that fits its words (the rule is in `docs/design/canvas-file.md`); a given size is kept |
| `remove` | `ids`: list of item ids. Ids already gone are fine |
| `move` | `ids`, `dx`, `dy` |
| `set` | `id`, `fields`: item fields to replace, laid over the item's current ones. New `text` without `w` or `h` reads the missing one as 0, so the frame fits the new words by the same rule |
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
**`canvas_read`** builds the packet of one page (`docs/design/packet.md`):
the user's ink drawn after operation `after` as images, an index of what
is new and an outline of the rest. Without `after`, the marker is the
canvas's last sling: the `to` of the packet that the latest `sling_sent`
event tagged with the canvas names, and 0 (everything is new) before the
first. A read never moves it; a sling does. `after` at or past the last
operation is a valid packet with nothing new. `select` takes ids as the
file (`i12`) or a packet (`c12`, `t12`) writes them, and keeps only those
items in the `page` section. The text, without `at`, is stored as a blob,
so the same page gives the same id; each image is a blob too. The daemon
writes `packet.txt` and the images into `out` (made if missing), or into
`packets/<hex>/` in its data directory, and `text` carries
`at=<that directory>`. A canvas or page that is not there is `not_found`;
a selected id that is not one, a directory it cannot write, or a last
sling whose packet it cannot read is `invalid`, with a hint.

```json
{ "v": 1, "op": "canvas_read", "canvas": "idea", "after": 41 }
{ "ok": true, "packet": "sha256:5e1f…", "dir": "/data/slinger/packets/5e1f…", "text": "slinger-packet 1 canvas=idea page=Page%201 from=42 to=58 scale=0.35 at=/data/slinger/packets/5e1f…\n# new\n…", "images": [{ "name": "c42.png", "id": "sha256:9a0c…" }] }
```

#### Panes

The server keeps each workspace's panes in its file (`[layout]`,
`docs/workspace.md`), so the app, the CLI and agents share one arrangement:
the app sends its own changes with these verbs and follows the changes others
make. A pane shows a canvas or an agent UI. A pane is named by its number: 1
for the first in order, depth first, which is left to right and top to
bottom. Numbers move as panes open and close; list them again before naming
one.

`layout` is a tree: a pane, `{pane, kind, target}`, or a split,
`{split, ratio, first, second}`, where `split` is `row` (side by side) or
`column` (stacked) and `ratio` (0.1 to 0.9) is the first side's share. A
workspace whose layout is unset, or one this daemon cannot read, has one
pane on its canvas.

**`pane_open`** splits pane `beside` in half: it keeps the first side, and
the new pane takes the second, to its right, or below it with `below`.
Without `beside`, the whole window splits that way. The canvas or UI must be
there (`not_found`, with a hint), and so must pane `beside` (`not_found`). A
window holds at most 64 panes; one more is `invalid`. **`pane_close`** gives
the pane's space to its sibling. A pane that is already gone, or the last
pane, which stays, changes nothing and succeeds with no `event`.
**`pane_resize`** moves the divider after pane `pane` (on its right, or
below it): the split whose first side ends with that pane gives its first
side `ratio`, held in 0.1 to 0.9. Every divider is after exactly one pane,
so this names each one; the app sends it when the pen drags a divider or
the grow and shrink keys run. A pane that is not there, the last pane (no
divider comes after it), or a share the split already has changes nothing
and succeeds with no `event`. A workspace that is not open is `not_found`.

Each change is written to the workspace's file, whole, before the reply, and
appends one `pane_moved` event tagged with the workspace, from `slingerd`:
the pane it opened (`canvas:notes`) with its frame, the pane before the
divider it moved with its new frame, or the pane it closed, with no size. The server has no window, so a frame there is the pane's share
of the window: `x`, `y`, `width` and `height` from 0 to 1, origin at the top
left.

```json
{ "v": 1, "op": "pane_open", "workspace": "notes", "kind": "canvas", "target": "board", "beside": 1 }
{ "ok": true, "workspace": "notes", "layout": { "split": "row", "ratio": 0.5, "first": { "pane": 1, "kind": "canvas", "target": "notes" }, "second": { "pane": 2, "kind": "canvas", "target": "board" } }, "pane": 2, "event": 7 }
```

#### The change feed

`canvas_changes` gives the operations numbered above `after`, lowest first,
so a client that holds a canvas learns what changed without reading all of
it again (SPEC section 5). Each op is the canvas file's line as stored, as a
JSON object with every field it has, including fields and kinds this daemon
does not know. `next` is the number of the first op not given: ask again
with `after` one below it. `canvas_show`'s `next` minus 1 is the `after` that
follows a page just read. `after` the newest number is an empty list and
succeeds; an `after` at or above the canvas's next number is `invalid`, as
the client holds a number this canvas never wrote (show it again).

A reply holds at most 1 MiB of lines, and always at least one when there is
one, so a long backlog comes in parts.

**Waiting.** With `wait` above 0 and nothing newer, the daemon holds the
reply until a change to that canvas, `wait` milliseconds pass, or the daemon
stops, whichever is first; then it replies with what there is, which may be
nothing. A `wait` above 60000 waits 60000. A client loops: ask, apply what
came, ask again from `next`.

The protocol stays one line each way, and that settles the design. The other
choice was a stream: a request that subscribes and then many reply lines.
A stream would need framing for many replies, a way to end them, and a client
that reads lines as they arrive; every verb today is one exchange the client
can time out. Holding one reply back (a long poll) keeps all of that. It
costs one request per change burst, which is nothing on a local socket. The
client's read timeout is its usual 5 seconds plus the wait. A waiting
request holds one daemon thread, and a stop answers it at once.

```json
{ "v": 1, "op": "canvas_changes", "canvas": "main", "after": 6, "wait": 30000 }
{ "ok": true, "next": 8, "ops": [{ "n": 7, "t": "2025-11-10T16:00:00.000Z", "by": "agent:scout", "op": "add", "item": { "id": "i5", "page": "p1", "kind": "text", … } }] }
```

#### Crops and strokes

What an agent asks a canvas for when the packet (`docs/design/packet.md`)
is not enough: an image of one item or one box (R5), and the raw points of
ink (R6).

**Ids.** `of` and `item` take an item id as `canvas_show` gives it (`i12`)
or as a packet gives it (`c12`, `t12`, `b12`, `a12`). The number is the
item's. `c<n>` on a stroke of the user's names every stroke of the cluster
it is in, clustered on the page as it is now; on anything else it names the
item alone. An id that is not one is `invalid`; an item that is not there
is `not_found`.

**`canvas_crop`** draws by the packet's image rules: the scale `0.35`, ink
in its colour on white, and tiles of at most 1568 pixels a side, overlapping
by 64. An item is framed by a 16-unit margin. A box `of` is `x,y,w,h` in
tenths of a unit on the first page, or `<page>:x,y,w,h` on the page with
that id or name; it is cut at its edges, and a box with nothing in it is a
white image. A crop is at most 16000 units a side (at most 4 by 4 tiles):
a larger one, or a negative size, is `invalid` with a hint. The daemon
writes one image to `path`, making its directory if missing; tiles go
beside it as `<stem>-1.png`, `<stem>-2.png`, ... in reading order. Each image
in the reply gives its file, its place in the whole picture (`x`, `y`) and
its size in pixels. The bytes travel as files, as a blob's do (below).

**`canvas_strokes`** gives the strokes `item` names, in drawing order, each
as `canvas_show` gives an item, except that `points` (absolute `[x, y]`
pairs in tenths) replaces the file's `pts`. An item that is not ink is
`invalid`.

```json
{ "v": 1, "op": "canvas_crop", "canvas": "main", "of": "p1:0,0,4000,2000", "path": "/tmp/box.png" }
{ "ok": true, "images": [{ "path": "/tmp/box.png", "x": 0, "y": 0, "width": 140, "height": 70 }] }
{ "v": 1, "op": "canvas_strokes", "canvas": "main", "item": "c4" }
{ "ok": true, "strokes": [{ "id": "i4", "page": "p1", "by": "user", "t": "…", "kind": "stroke", "color": "#000000", "width": 20, "points": [[0, 0], [100, 0]] }] }
```

A client writes both verbs with its own type (`crates/control/src/look.rs`,
"Adding a verb" below), so the app, which never sends them, carries no code
for them.

#### Sessions

A session is an agent the daemon can reach (SPEC section 5): the agent of an
open workspace, one session per agent name. A session is `{agent, home,
deliver, status, note}`: `home` is the directory of its workspace's file (the
first workspace by name, when several name the agent), `deliver` is how a
message reaches it (`process`, a process the daemon holds, is the one kind in
v1), and `status` and `note` are what the agent last set, absent until it sets
one. **`session_show`** of an agent no open workspace names is `not_found`.

**`status_set`** sets an agent's status. `blocked` appends `agent_blocked`
with the note as its `reason` (empty without one) and `done` appends
`agent_done` with the note as its `summary`; the event's source and `agent`
tag are the agent, and its workspace tag follows the rule under "Data
directory". `working` has no event in the record: the daemon holds it until
it stops. A session shows the status set last since the daemon started, else
the last `agent_blocked` or `agent_done` with its agent tag in the record, so
`blocked` and `done` survive a restart. An agent no open workspace names may
set a status too: the record keeps it, and the session shows it once a
workspace with that agent opens. A blank agent or another status is
`invalid`.

```json
{ "v": 1, "op": "status_set", "agent": "scout", "status": "blocked", "note": "need the API key" }
{ "ok": true, "agent": "scout", "status": "blocked", "note": "need the API key", "event": 12 }
{ "v": 1, "op": "session_list" }
{ "ok": true, "sessions": [{ "agent": "scout", "home": "/Users/me/notes", "deliver": "process", "status": "blocked", "note": "need the API key" }] }
```

**The process.** The daemon holds one Claude Code process per session (SPEC
D10): `<claude> --print --input-format stream-json --output-format
stream-json --verbose` and the session's limits (below), in the agent's home,
with `SLINGER_AGENT` set to the agent and `SLINGER_SOCKET` to the daemon's
socket, so the agent's own `slinger` calls act as it and reach this daemon.
When `--slinger` is an absolute path, its folder comes first on the process's
`PATH`, so those bare `slinger` calls run the CLI its hooks run. Its stderr goes to the
daemon's. A session adds what the daemon knows of it: `process` (`running`,
or `exited` when it ended without a stop; absent when there is none), and
while it runs its `pid` and `pending`, the messages sent that it has not
answered (a `result` line ends an answer). `answer` is the text of its last
answer. `session_id` names its conversation.

**Limits.** A session has stated limits, set by its workspace's file
(`[agent]` in docs/workspace.md). By default it:

- reads its home and the workspace's other roots on this machine, and
  nothing else (`--add-dir` for each root that is not the home);
- has the built-in tools that read and run commands (`--tools
  Read,Glob,Grep,Bash`), and runs only the `slinger` CLI
  (`--allowedTools 'Bash(slinger *)'`) and read-only commands inside those
  directories. It cannot write. Whatever else would need a permission is
  denied, since nobody is there to ask (`--permission-mode dontAsk`);
- loads the settings and the instructions (`CLAUDE.md`) of its home, but not
  the owner's own (`--setting-sources project,local`), and no MCP server
  (`--strict-mcp-config`).

The agent's `dirs` adds directories, its `tools` adds permission rules (and
the built-in tool each rule names), and `user_config = true` loads the
owner's settings and instructions too (`--setting-sources
user,project,local`). The home's own `.claude/settings.json` belongs to the
workspace and still applies, so the owner can widen a session there too.

**`session_start`** starts the process of the workspace's agent. The first
start gives it a new session id (`--session-id`); once its output names that
id, the daemon keeps it in `sessions.json`, and every later start, after a
restart too, resumes it (`--resume`). Starting one that runs changes nothing;
one that exited is started again. **`session_stop`** closes the process's
input, waits up to 3 seconds and then kills it; stopping one that is not
running succeeds. **`session_send`** writes `text` to the process as one user
message and returns: the answer is the agent's to deliver, through the CLI.
Blank text is `invalid`. A send to a process that was never started, or that
exited, is `unavailable` with a hint to start it: the daemon does not restart
an agent on its own (D20). A workspace that is not open is `not_found`, one
with no agent is `invalid`, and a program that is not there is `not_found`.

```json
{ "v": 1, "op": "session_start", "workspace": "notes" }
{ "ok": true, "session": { "agent": "scout", "home": "/Users/me/notes", "deliver": "process", "process": "running", "pid": 4242, "pending": 0, "session_id": "6f1c0d52-8a3e-4b7d-9c1e-2f4a5b6c7d8e" } }
{ "v": 1, "op": "session_send", "workspace": "notes", "text": "what is in this directory?" }
```

**Hooks.** Each process starts with `--settings <JSON>`: hooks of its own
(SPEC section 5), so a session reports to Slinger without the agent's help
and no settings file of the user's changes. Each hook runs the CLI, which
takes the agent and the socket from the process's environment:

| Claude Code event | Matcher | Command |
|---|---|---|
| `UserPromptSubmit` (a turn starts) | | `slinger status set working` |
| `Stop` (the turn ends) | | `slinger status set done` |
| `Notification` | `permission_prompt` | `slinger status set blocked --note 'waits for permission'` |
| `Notification` | `elicitation_dialog\|elicitation_url_dialog` | `slinger status set blocked --note 'waits for input'` |
| `PostToolUse` | `Write\|Edit\|MultiEdit\|NotebookEdit` | `slinger file touched --hook` |

A hook's failure exits 1, never 2, which would block the prompt or keep the
turn going: Claude Code notes it and goes on. The core makes the settings
(`slinger_core::session::hooks`).

**`file_touched`** records that an agent wrote a file: the daemon reads it,
hashes it and appends `file_touched` with its `path` and `hash` (its blob id;
the bytes are not stored, D14), the agent as source and agent tag. `hook` is
the payload Claude Code gave the hook on stdin; the file is its tool input's
`file_path` (`notebook_path` for a notebook), against its `cwd` when
relative. A payload that names no file touches none and succeeds. A blank
agent, a relative `path`, or both or neither of `path` and `hook` is
`invalid`; a file that is not there, or cannot be read, is `not_found`.

```json
{ "v": 1, "op": "file_touched", "agent": "scout", "path": "/Users/me/notes/plan.md" }
{ "ok": true, "agent": "scout", "path": "/Users/me/notes/plan.md", "hash": "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824", "event": 15 }
```

**A check against the real Claude Code.** The tests run a stand-in that
speaks the same format. To see the real one keep a conversation, use a
private daemon and a scratch directory. It spends a few tokens, and the
conversation lands in Claude Code's own session store like any other.

```bash
tmp=$(mktemp -d) && mkdir "$tmp/ws"
printf 'version = 1\nname = "check"\n[agent]\nname = "checker"\n' > "$tmp/ws/slinger-workspace.toml"
slingerd --socket "$tmp/s.sock" --data-dir "$tmp/data" & daemon=$!
s="slinger --socket $tmp/s.sock"
$s workspace open "$tmp/ws" && $s session start check
$s session send check 'Remember the word plum. Reply with only: ok'
$s session send check 'What word did I ask you to remember? Reply with only the word.'
$s session show checker    # once pending is 0: answer plum, and a session_id
kill $daemon               # the process ends with the daemon
```

Start `slingerd` again on the same `--data-dir`, `session start check` and
ask once more: `session show checker` has the same `session_id`, and the
answer is still the word. `scripts/e2e-sling.sh` does the same for three
slings and checks what they leave on the canvas and in the record.

#### Slings

**`sling_send`** is the sling (SPEC section 6): the packet of one canvas
page and the user's prompt, as one message to the agent of the open
workspace whose canvas it is. The packet holds the ink drawn after the
canvas's last sling (the marker of `canvas_read`), with `select` naming the
selection as `canvas_read` takes it. The message
(`slinger_core::sling::message`) is line 1 `slinger-sling 1` with the
selection as packet ids (a stroke is named by its cluster), the packet with
each image under the line that names it, a `# reply` note naming only the
agent verbs this daemon serves (`canvas put`, `canvas crop`), and the
prompt last. The first sling into a conversation (no `sling_sent` of the
agent names its session id) gets the whole note, later ones one line. It
reaches the process as one stream-json user message: text blocks and a
base64 PNG image block per image.

The reply comes once the message is written to the process; the daemon
does not wait for the answer, which arrives as the agent's `canvas_put`s
and its `status_set`. Only after delivery is the packet stored (its text
without `at` and each image as blobs) and `sling_sent` appended, with the
daemon as source and tagged with the agent and the canvas. That event moves
the marker, so a sling that fails sends the same ink next time. Selecting
old items again sends a packet with nothing new (`strokes` 0). Slings go one
at a time, so two packets never cover the same operation.

Slinger never starts an agent for a sling (D10): with no running process
the reply is `unavailable` with a hint to `session start` the workspace,
and nothing is stored. A canvas no open workspace has is `not_found`, a
workspace with no agent `invalid`, and the canvas, page and selection fail
as in `canvas_read`.

**Which sling a reply answers.** A `canvas_put` answers the agent's latest
sling on that canvas, until the next sling there: nothing closes a sling,
`agent_done` included, so an agent that adds a note after it reported done
still lands `reply_landed` against it. The other choice was to close a sling
at `agent_done`; then a late put would answer nothing, and the record would
lose which question it belonged to.

```json
{ "v": 1, "op": "sling_send", "canvas": "notes", "select": ["c42"], "prompt": "what is missing here?" }
{ "ok": true, "event": 14, "packet": "sha256:5e1f…", "from": 42, "to": 58, "strokes": 23, "agent": "scout", "session_id": "6f1c0d52-8a3e-4b7d-9c1e-2f4a5b6c7d8e" }
```

#### Agent canvas verbs

An agent answers on the canvas, not in chat (SPEC section 7). Each answer
is an item the agent makes, with its name and the time, as one undo step of
its own.

**`canvas_put`** adds text, a box or an arrow, placed beside the item
`beside` names (ids as for crops, above: `c<n>` on the user's ink is its
whole cluster) on that item's page. Text is black, wrapped at 40 characters
a line, in a frame that holds it; a box is `w` by `h` tenths. Either goes in
free space: 24 units right of the item, top-aligned; else below it, left of
it or above it; else further right, past whatever is in the way. It never
comes within 12 units of another item. An arrow joins `from` and `to`, two
items on that page, from the edge of one to the edge of the other, and
points at both. The reply gives the new item's id and its box. When a
`sling_sent` tagged with the agent and the canvas is in the record, the put
answers the latest such sling: the daemon appends `reply_landed` (`sling`,
`items`: the new item) with the agent as source and tags it with the agent
and the canvas. A canvas or item that is not there is `not_found`; a blank
agent, an id that is not one, blank text, a negative size, or an arrow to an
item on another page is `invalid`.

```json
{ "v": 1, "op": "canvas_put", "canvas": "idea", "agent": "scout", "beside": "c12", "text": "the date is wrong" }
{ "ok": true, "item": "i31", "page": "p1", "x": 4810, "y": 1200, "w": 4080, "h": 240, "seq": 58, "event": 14 }
```

**`canvas_edit`** changes an item the agent made, as one undo step: `text`
gives a text item new words (its frame grows or shrinks to hold them),
`box` gives a text or a box a new size `[w, h]`, and `move` puts the top-left
corner of the item's box at `[x, y]`. What it leaves out stays; an edit that
leaves the item as it was writes nothing. **`canvas_remove`** takes an item
the agent made off the canvas; an item already gone (or never there)
succeeds and writes nothing (SPEC B4). Ids are as for `canvas_put`. The
author rule (above, "Authors") is checked first: an agent naming the user's
ink, or another agent's item, gets `invalid` with a hint, whatever it asked.
A canvas that is not there, or an item to edit that is not there, is
`not_found`; a blank agent, an id that is not one, blank text, text for an
item that is not text, a size for a stroke or an arrow, or a size below zero
is `invalid`.

```json
{ "v": 1, "op": "canvas_edit", "canvas": "idea", "agent": "scout", "item": "i31", "text": "the date is right", "move": [4810, 1500] }
{ "ok": true, "item": "i31", "page": "p1", "x": 4810, "y": 1500, "w": 4080, "h": 240, "seq": [59] }
{ "v": 1, "op": "canvas_remove", "canvas": "idea", "agent": "scout", "item": "i31" }
{ "ok": true, "seq": [60] }
```

The agent canvas verbs are one variant of `Request` (`Request::Agent`),
written by `crates/control/src/agent.rs`.

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
| `unavailable` | The agent's process is not running (`session_send`, `sling_send`); made by the client when the daemon did not answer |
| `internal` | The daemon could not read or write its record, or the client could not read the reply |

`not_found` also covers a blob the record does not hold, a file to put
that is not there, a UI with no such name or version, and a canvas, page or
item that is not there, a directory or workspace that is not there or
not open, and an agent with no session.

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
   write the same line. `canvas_read` does the same in
   `crates/control/src/packet.rs`. A group of such verbs shares one
   variant: `#[serde(untagged, skip_serializing)]` over an enum of their
   own, tagged by `op`, as `crates/control/src/session.rs` and
   `crates/control/src/agent.rs` do (`sling_send` is a session verb). The app sends
   `workspace_list` and the pane verbs, each written with such a type of its
   own (`crates/control/src/pane.rs`), as are the showing verbs it sends
   (`crates/control/src/ui.rs`) and the session verbs (`session_list`,
   `sling_send`, as `SessionAsk`), so the app builds no `Request` at all;
   keep it that way.
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
