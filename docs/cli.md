# CLI contract

`slinger` follows this contract so that people, scripts and agents can rely
on it. Breaking it is a breaking change.

## Global flags

| Flag | Effect |
|---|---|
| `-h, --help` | Help for the tool or a command |
| `-v, --version` | Bare semver (`0.1.0`). With `--json`: `{name, version, platform}` |
| `--json` | Exactly one JSON document on stdout, errors included |
| `-q, --quiet` | Silence progress and confirmations. Never silences errors or a command's main output |
| `--verbose` | Extra diagnostics on stderr |
| `--socket <PATH>` | The control socket of `slingerd`. Default: `$SLINGER_SOCKET`, else see `docs/service.md` |
No arguments prints help. Unknown commands and flags are usage errors with
a "did you mean" suggestion.

## Commands

| Command | Prints |
|---|---|
| `hello [NAME]` | The greeting |
| `record append <KIND> [PAYLOAD] [--source NAME] [--workspace NAME] [--agent NAME] [--canvas ID]` | The new event's number. `--json`: `event` |
| `record list [--kind KIND] [--machine NAME] [--workspace NAME] [--agent NAME] [--canvas ID] [--after NUMBER]` | One line per event: number, kind, tags (`name=value`) and payload, separated by tabs. `--json`: `events` |
| `blob put <FILE\|->` | The blob id (`sha256:<hex>`). `--json`: `id` |
| `ui put <NAME> <FILE\|-> [--source NAME]` | `<name>@<version>`: the version that holds the bytes. `--json`: `ui`, `new` |
| `ui list` | One line per UI, its latest version: `<name>@<version>`, hash, bytes, source and time, separated by tabs. `--json`: `uis` |
| `ui show <NAME>` | One line per version of that UI, oldest first, as `ui list`. `--json`: `uis` |
| `ui export <UI> [--out DIR]` | The path of the file written, `<DIR>/<name>.v<version>.html`. `--json`: `ui`, `path` |
| `blob get <ID> [--out FILE]` | The blob's bytes, exactly, on stdout. With `--out`: nothing, and the file holds the bytes. `--json` (needs `--out`): `id`, `size` |
| `canvas create <ID>` | Nothing. `--json`: `canvas` |
| `canvas list` | One canvas id per line. `--json`: `canvases` (list of ids) |
| `canvas show <ID> [--page PAGE]` | One line per item on the page, in drawing order: id, kind, maker and the item as JSON, separated by tabs. `--json`: `canvas`, `next`, `page`, `pages`, `items` |
| `canvas apply <ID> <OPS\|-> [--agent NAME]` | The ids the operations made, one per line. `--json`: `seq`, `made` |
| `canvas undo <ID> [--agent NAME]`, `canvas redo <ID> [--agent NAME]` | Nothing. `--json`: `seq`, `made` |
| `canvas read <ID> [--page PAGE] [--after NUMBER] [--select IDS] [--out DIR]` | The packet's text (`docs/design/packet.md`), its header naming the directory that holds the images (`at=`). `--json`: `packet`, `dir`, `text`, `images` |
| `canvas import <FILE> [--name ID]` | The new canvas's id, then one count per line, tab separated: `pages`, each item kind, and `skipped` with each tldraw shape type. `--json`: `canvas`, `pages`, `items`, `skipped` |
| `workspace open [DIR] [--name NAME]` | The workspace's name. `--json`: `workspace`, `new` |
| `workspace list` | One line per open workspace, by name: name, canvas and directory, separated by tabs. `--json`: `workspaces` |
| `workspace show <WORKSPACE>` | One field per line, the name and the value separated by a tab: `name`, `tag`, `canvas`, `dir`, then `agent` (if any) and one `root` line per root, `machine:path`. `--json`: `workspace` |
| `workspace close <WORKSPACE>` | Nothing. `--json`: `dir` |
| `canvas changes <ID> [--after NUMBER] [--wait MS]` | One line per operation numbered above `--after` (default 0: all), lowest first, as the canvas file's JSON line. `--json`: `next`, `ops` |
| `canvas crop <ID> (--item ITEM \| --box X,Y,W,H [--page PAGE]) [--out FILE]` | The path of each image written, one per line, tiles in reading order. `--json`: `images` (`path`, `x`, `y`, `width`, `height`) |
| `canvas strokes <ID> --item ITEM` | One stroke per line, as JSON with absolute `points` in tenths. `--json`: `strokes` |
| `session list` | One line per session, by agent: agent, status (`-` for none) and home, then the note if there is one, separated by tabs. `--json`: `sessions` |
| `session show <AGENT>` | One field per line, the name and the value separated by a tab: `agent`, `home`, `deliver`, `status`, then those it has of `note`, `process`, `pid`, `pending`, `session_id` and `answer` (its line breaks as `\n`). `--json`: `session` |
| `session start <WORKSPACE>`, `session stop <WORKSPACE>` | Nothing. `--json`: `session` |
| `session send <WORKSPACE> <TEXT\|->` | Nothing. `--json`: `session` |
| `status set <working\|blocked\|done> [--note TEXT] [--agent NAME]` | Nothing. `--json`: `agent`, `status`, `note`, `event` |
| `canvas put <ID> --beside ITEM (--text TEXT \| --box W,H \| --arrow FROM TO) [--agent NAME]` | The new item's id and its box `x,y,w,h` in tenths, separated by a tab. `--json`: `item`, `page`, `x`, `y`, `w`, `h`, `seq`, `event` |
| `canvas edit <ID> --item ITEM [--text TEXT] [--box W,H] [--move X,Y] [--agent NAME]` | The item's id and its box `x,y,w,h` in tenths now, separated by a tab. `--json`: `item`, `page`, `x`, `y`, `w`, `h`, `seq` |
| `canvas remove <ID> --item ITEM [--agent NAME]` | Nothing. `--json`: `seq` |

`record append` takes the payload as a JSON object (default `{}`); the
kinds and their payloads are in `docs/service.md`. A payload that is not
JSON, an unknown kind or a payload that does not fit it exits 5 (`invalid`)
with a hint. `--source` defaults to `slinger`. `record list` with no match
prints nothing and exits 0.

`blob put -` reads the bytes from stdin. Putting bytes already stored
succeeds and prints the same id. `blob get` of an id the record does not
hold exits 3 (`not_found`), and of a string that is not an id exits 5
(`invalid`). `blob get --json` without `--out` exits 2 (`usage`): stdout
cannot carry both the bytes and the envelope.

`ui put` stores one self-contained HTML file (`-` reads stdin) as the
next version of a UI; putting the bytes of the latest version again adds
none and prints that version. A name or file the rules in
`docs/design/live-items.md` refuse exits 5 (`invalid`). `ui export` takes
`<name>` (the latest version), `<name>@<version>` or a blob id, and writes
into the current directory unless `--out` names another, which it makes
if missing. An unknown UI exits 3 (`not_found`). `ui list` with no UIs
prints nothing and exits 0.

`canvas apply` takes one operation as a JSON object, or a list of them, as
one undo step (the operations are in `docs/service.md`, "Canvas
operations"); `-` reads them from stdin. Without `--agent` the user acts.
An agent changing an item it did not make, an id that is not one, or
operations that are not JSON exit 5 (`invalid`). A canvas that exists exits
4 (`conflict`) on create; one that does not exists exits 3 (`not_found`) on
the other commands. A damaged canvas file (`canvas_corrupt`) or one from a
newer slinger (`canvas_too_new`) exits 1.

`canvas import` makes a new canvas from a tldraw file (`.tldraw` or `.tldr`),
named by `--name` or after the file (lowercased, other characters made `-`).
A file that is not there exits 3 (`not_found`), one that is not a tldraw file
exits 5 (`invalid`), and a canvas of that id exits 4 (`conflict`).
`workspace open <DIR>` opens a directory (relative to the current one) as a
workspace, writing its `slinger-workspace.toml` and making its canvas when it
has none; `--name` names a new one. `workspace open --name <NAME>` with no
directory opens a workspace with no root. Opening an open workspace prints
it again. `<WORKSPACE>` is a name, or a directory (`.`, `..` or anything with
a `/`). A directory that is not there or a workspace that is not open exits
3 (`not_found`); a second workspace on one primary root, name or canvas exits
4 (`conflict`); `workspace open` with neither exits 5 (`invalid`). Closing
keeps the file, the canvas and the agent. The rules are in
`docs/workspace.md`; what the daemon does is in `docs/service.md`.

`canvas read` prints what an agent reads instead of a screenshot: images
of the ink drawn after operation `--after` (default: the canvas's last
sling), an index of what is new, and an outline of the rest of the page.
The images are in the directory the header's `at=` names: `--out`, or
slingerd's own. `--select c42,t12` keeps only those items in the outline.
A read changes nothing. The packet is stored as a blob (`packet` in
`--json`), and so is each image (`images`), for `blob get`. A canvas or
page that is not there exits 3 (`not_found`), an id that is not one exits
5 (`invalid`).

`canvas changes` prints nothing and exits 0 when nothing is newer. With
`--wait`, it first waits up to that many milliseconds (at most 60000) for a
change. A long backlog comes in parts: run it again with `--after` one below
the `next` of `--json`. An `--after` the canvas has not reached exits 5
(`invalid`).

`canvas crop` writes an image of one item or one box at the packet's scale
(`docs/service.md`, "Crops and strokes"). `--item` takes an id from `canvas
show` (`i12`) or from a packet (`c12` is the whole cluster). `--box` is in
tenths of a unit, like every canvas command, on the first page unless
`--page` names another. The image goes to `--out` (default `<ITEM>.png` or
`crop.png` in the current directory); a crop too large for one image goes
beside it in tiles, `<stem>-1.png` and on. Both or neither of `--item` and
`--box` exit 2 (`usage`); a box too large, over 16000 units a side, exits
5 (`invalid`). `canvas strokes` prints the raw points of a stroke or of every
stroke of a cluster; an item that is not ink exits 5. An item that is not
there exits 3 (`not_found`).

`status set` is an agent verb: it acts as the calling agent, named by
`--agent <NAME>` or else by the `SLINGER_AGENT` environment variable, so a
harness sets the variable once for its agent. With neither (or a blank name)
it exits 2 (`usage`) with a hint. Every agent verb added after it names the
agent the same way. `status set blocked` and `status set done` append
`agent_blocked` and `agent_done` to the record (the note is the reason or the
summary); `working` is shown by `session list` only. Another status exits 2.
`canvas put` is an agent verb too: the agent answers on the canvas with
text, a box (`W,H` in tenths of a unit) or an arrow between two items, placed
beside `--beside`, an id from `canvas show` (`i12`) or a packet (`c12` is the
whole cluster), where it covers nothing (`docs/service.md`, "Agent canvas
verbs"). After a sling to that agent on that canvas, the put also appends
`reply_landed` to the record. An item that is not there exits 3
(`not_found`); none or two of `--text`, `--box` and `--arrow`, or a `--box`
that is not two integers, exits 2 (`usage`); blank text exits 5 (`invalid`).
`canvas edit` and `canvas remove` are agent verbs for items the agent made:
`edit` gives a text new words, a text or a box a new size (`--box W,H`), or
moves the item's top-left corner to `--move X,Y`, all in tenths; `remove`
takes the item off the canvas, and an item already gone succeeds. Naming
the user's ink or another agent's item exits 5 (`invalid`) with a hint. An
edit with none of `--text`, `--box` and `--move`, or a pair that is not two
integers, exits 2 (`usage`); an item to edit that is not there exits 3
(`not_found`).
`session list` shows the agents of the open workspaces; with none it prints
nothing and exits 0. `session show` of an agent with no session exits 3
(`not_found`).

`session start` starts the workspace's agent: a Claude Code process in its
home that `slingerd` holds open, resuming the conversation it last had, even
after a restart. `session send` delivers one message to it (`-` reads it from
stdin) and returns once it is delivered; `session show` has the process's
state and its last answer. A send to an agent that was never started, or
whose process exited, exits 6 (`unavailable`) with a hint to start it.
`session stop` ends the process; stopping one that is not running succeeds.
A workspace that is not open exits 3, one with no agent exits 5. The process
gets `SLINGER_AGENT` and `SLINGER_SOCKET`, so its `slinger` calls act as its
agent on the daemon that started it (see "The process" in
`docs/service.md`).

## Output

- **stdout** carries the result and nothing else, so it pipes cleanly.
- **stderr** carries progress, diagnostics and human-readable errors.
- **Human errors:** `✗ message` and then an indented, dim hint that says
  what to do.
- **Color:** only when stdout is a TTY. `NO_COLOR` turns it off and
  `FORCE_COLOR` turns it on. `--json` never uses color.
- **Broken pipe** (`| head`) exits 0.

## JSON envelope

```json
{ "success": true,  "command": "hello", "greeting": "hello, ada" }
{ "success": false, "command": "hello", "error": "name is empty", "code": "invalid", "hint": "pass a non-empty name" }
```

The data fields sit at the top level next to `success` and `command`. On
failure, `code` is stable and `error` is for people. Branch on `code`,
never on `error`.

## Exit codes

| Code | Name | Meaning |
|---|---|---|
| 0 | ok | Success |
| 1 | error | Unexpected or internal failure |
| 2 | usage | Bad arguments, unknown command or flag |
| 3 | not_found | A named thing does not exist |
| 4 | conflict | The thing exists or is in the wrong state |
| 5 | invalid | Input was understood but rejected |
| 6 | unavailable | `slingerd` does not answer, or the agent's process is not running. The hint says how to start it |
Add a code only when a caller needs to tell the failure apart from the
others. Never renumber.

## Adding a command

1. Add the verb to the daemon: see "Adding a verb" in `docs/service.md`.
2. Add a variant to `Command` in `crates/cli/src/cli.rs`. Its doc comment is its help text.
3. Add a match arm that calls the client method and returns `Output { data, human }`.
   The CLI never calls the core.
4. Add tests to `crates/cli/src/tests.rs`. They run against a real daemon.
