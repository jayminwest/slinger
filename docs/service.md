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
  machine's name, the host name up to its first dot.
- Blobs are keyed by their id (`sha256:<hex>`). Storing the same bytes again
  stores nothing new.
- The file's layout has a version. A daemon refuses a file laid out by a
  newer one (exit 4).

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
CLI does this for `blob put -` and for `blob get` without `--out`, in the
temporary directory, and removes the file after.

### Error codes

The core's codes pass through unchanged (`not_found`, `conflict`, `invalid`).
The protocol adds:

| Code | Meaning |
|---|---|
| `bad_request` | The line is not JSON, has no `v`, names no known `op`, or has wrong arguments |
| `too_large` | The request line is over the limit |
| `unsupported_version` | `v` is not a version this daemon speaks |
| `unavailable` | Made by the client, never sent: the daemon did not answer |
| `internal` | The daemon could not read or write its record, or the client could not read the reply |

`not_found` also covers a blob the record does not hold, and a file to put
that is not there.

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
   `crates/control/src/client.rs`.
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
