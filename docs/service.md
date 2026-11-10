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

### Error codes

The core's codes pass through unchanged (`not_found`, `conflict`, `invalid`).
The protocol adds:

| Code | Meaning |
|---|---|
| `bad_request` | The line is not JSON, has no `v`, names no known `op`, or has wrong arguments |
| `too_large` | The request line is over the limit |
| `unsupported_version` | `v` is not a version this daemon speaks |
| `unavailable` | Made by the client, never sent: the daemon did not answer |
| `internal` | Made by the client, never sent: it could not read the reply |

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
