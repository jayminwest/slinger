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
| `record append <KIND> [PAYLOAD] [--source NAME] [--workspace NAME] [--agent NAME] [--canvas ID]` | The new event's number. `--json`: `event` |
| `record list [--kind KIND] [--machine NAME] [--workspace NAME] [--agent NAME] [--canvas ID] [--after NUMBER]` | One line per event: number, kind, tags (`name=value`) and payload, separated by tabs. `--json`: `events` |
| `blob put <FILE\|->` | The blob id (`sha256:<hex>`). `--json`: `id` |
| `ui put <NAME> <FILE\|-> [--source NAME]` | `<name>@<version>`: the version that holds the bytes. `--json`: `ui`, `new` |
| `ui list` | One line per UI, its latest version: `<name>@<version>`, hash, bytes, source and time, separated by tabs. `--json`: `uis` |
| `ui show <NAME>` | One line per version of that UI, oldest first, as `ui list`. `--json`: `uis` |
| `ui export <UI> [--out DIR]` | The path of the file written, `<DIR>/<name>.v<version>.html`. `--json`: `ui`, `path` |
| `ui open <UI> --workspace W [--as pane] [--data JSON\|-] [--beside N] [--below] [--agent NAME]`, `ui open <UI> --as item --canvas C --beside ITEM [--size W,H] [--data JSON\|-] [--agent NAME]` | `<open>\t<pane>`: the showing's id (`pick-slot#12`) and the new pane's number, or `<open>\t<item>` and the live item's id. `--json`: `open`, `ui`, `workspace`, `pane` or `item` (`canvas`, `item`, `page`, `x`, `y`, `w`, `h`, `seq`), `event` |
| `blob get <ID> [--out FILE]` | The blob's bytes, exactly, on stdout. With `--out`: nothing, and the file holds the bytes. `--json` (needs `--out`): `id`, `size` |
| `canvas create <ID>` | Nothing. `--json`: `canvas` |
| `canvas list` | One canvas id per line. `--json`: `canvases` (list of ids) |
| `canvas show <ID> [--page PAGE]` | One line per item on the page, in drawing order: id, kind, maker and the item as JSON, separated by tabs. `--json`: `canvas`, `next`, `page`, `pages`, `items`, `path` (the canvas's file) |
| `canvas apply <ID> <OPS\|-> [--agent NAME]` | The ids the operations made, one per line. `--json`: `seq`, `made` |
| `canvas schema` | The item kinds and operations `canvas apply` takes, one per line: section (`unit`, `item`, `kind` or `op`), name and fields, separated by tabs. Needs no daemon. `--json`: `units`, `item`, `kinds`, `ops` (each name to its fields) |
| `canvas undo <ID> [--agent NAME]`, `canvas redo <ID> [--agent NAME]` | Nothing. `--json`: `seq`, `made` |
| `canvas read <ID> [--page PAGE] [--after NUMBER] [--select IDS] [--out DIR]` | The packet's text (`docs/design/packet.md`), its header naming the directory that holds the images (`at=`). `--json`: `packet`, `dir`, `text`, `images` |
| `canvas import <FILE> [--name ID]` | The new canvas's id, then one count per line, tab separated: `pages`, each item kind, and `skipped` with each tldraw shape type. `--json`: `canvas`, `pages`, `items`, `skipped` |
| `workspace open [DIR] [--name NAME]` | The workspace's name. `--json`: `workspace`, `new` |
| `workspace list` | One line per open workspace, by name: name, canvas and directory, separated by tabs. `--json`: `workspaces` |
| `workspace show <WORKSPACE>` | One field per line, the name and the value separated by a tab: `name`, `tag`, `canvas`, `dir`, then `agent` (if any) and one `root` line per root, `machine:path`. `--json`: `workspace` |
| `workspace close <WORKSPACE>` | Nothing. `--json`: `dir` |
| `pane list <WORKSPACE>` | One line per pane, in order (left to right, top to bottom): number, kind (`canvas` or `ui`) and target, separated by tabs. `--json`: `workspace`, `layout`, `panes` |
| `pane open <WORKSPACE> --type <canvas\|ui> --target <ID> [--beside PANE] [--below]` | The new pane's number. `--json`: `workspace`, `layout`, `panes`, `pane`, `event` |
| `pane close <WORKSPACE> <PANE>` | Nothing. `--json`: `workspace`, `layout`, `panes`, `event` (absent when nothing changed) |
| `pane resize <WORKSPACE> <PANE> <RATIO>` | Nothing. `--json`: `workspace`, `layout`, `panes`, `event` (absent when nothing changed) |
| `canvas changes <ID> [--after NUMBER] [--wait MS]` | One line per operation numbered above `--after` (default 0: all), lowest first, as the canvas file's JSON line. `--json`: `next`, `ops` |
| `canvas crop <ID> (--item ITEM \| --box X,Y,W,H [--page PAGE]) [--out FILE]` | The path of each image written, one per line, tiles in reading order. `--json`: `images` (`path`, `x`, `y`, `width`, `height`) |
| `canvas strokes <ID> --item ITEM` | One stroke per line, as JSON with absolute `points` in tenths. `--json`: `strokes` |
| `session list` | One line per session, by agent: agent, status (`-` for none) and home, then the note if there is one, separated by tabs. `--json`: `sessions` |
| `session show <AGENT>` | One field per line, the name and the value separated by a tab: `agent`, `home`, `deliver`, `status`, then those it has of `note`, `process`, `pid`, `profile`, `pending`, `session_id` and `answer` (its line breaks as `\n`). `--json`: `session` |
| `session start <WORKSPACE> [--fresh] [--profile NAME]`, `session stop <WORKSPACE>` | Nothing. `--json`: `session` |
| `profile list <WORKSPACE>` | One line per profile, in its file's order: name, directory (`-` for the agent's home) and rules joined by `, ` (`-` for none), separated by tabs. `--json`: `workspace`, `profiles` |
| `profile set <WORKSPACE> <NAME> [--dir DIR] [--tool RULE]...`, `profile remove <WORKSPACE> <NAME>` | Nothing. `--json`: `workspace`, `profiles` |
| `session send <WORKSPACE> <TEXT\|->` | Nothing. `--json`: `session` |
| `sling send <ID> [--page PAGE] [--select IDS] [--prompt TEXT\|-] [--profile NAME]` | Nothing. `--json`: `event`, `packet`, `from`, `to`, `strokes`, `agent`, `session_id` |
| `status set <working\|blocked\|done> [--note TEXT] [--agent NAME]` | Nothing. `--json`: `agent`, `status`, `note`, `event` |
| `file touched (<PATH> \| --hook) [--agent NAME]` | Nothing. `--json`: `agent`, `path`, `hash`, `event` |
| `canvas put <ID> --beside ITEM (--text TEXT \| --box W,H \| --ink PTS \| --arrow FROM TO) [--color COLOR] [--at X,Y] [--agent NAME]` | The new item's id and its box `x,y,w,h` in tenths, separated by a tab. `--json`: `item`, `page`, `x`, `y`, `w`, `h`, `seq`, `event` |
| `canvas edit <ID> --item ITEM [--text TEXT] [--box W,H] [--move X,Y] [--agent NAME]` | The item's id and its box `x,y,w,h` in tenths now, separated by a tab. `--json`: `item`, `page`, `x`, `y`, `w`, `h`, `seq` |
| `canvas remove <ID> --item ITEM [--agent NAME]` | Nothing. `--json`: `seq` |
| `feedback add <TEXT>...` | The note's `ts`. `--json`: `path`, `entry` |
| `feedback ls [--all] [--limit N]` | One line per note, newest first: `ts`, text and, with `--all`, `resolved` and its note for a resolved one, separated by tabs. `--limit` defaults to 20. `--json`: `path`, `entries` |
| `feedback resolve <TS> [NOTE]...` | Nothing. `--json`: `path`, `resolves`, `ts`, `note` |
| `feedback path` | The feedback file's path. `--json`: `path` |

`profile` keeps the profiles of a workspace's agent in its file
(`docs/service.md`, "Profiles"): each a working directory and the Claude
Code permission rules a session under it may use, in place of the agent's
own. `profile set` replaces a profile of that name whole; `--tool` takes one
rule and repeats (`--tool Edit --tool 'Bash(git *)'`), and `--dir` is taken
from the current directory when relative. A name, a rule or a tool slinger
does not know exits 5 (`invalid`) with a hint listing the tools; a `--dir`
that is not there, or a profile `remove` does not find, exits 3
(`not_found`). `session start --profile NAME` starts the agent under that
profile, in its own conversation. `sling send --profile NAME` sends only if
the agent runs under that profile, and exits 4 (`conflict`) otherwise.

`feedback` keeps notes about slinger itself in the daemon's feedback
file (`docs/service.md`, "Feedback"), the same notes the app's
`leave-feedback` command saves. `feedback add` joins its words with spaces;
a blank note exits 5 (`invalid`). `feedback ls` lists the open notes;
`--all` adds the resolved ones. `feedback resolve` takes a note's `ts` as
`ls` prints it, and a note of how it was resolved (a tracker id, say): an
unknown `ts` exits 3 (`not_found`) and one resolved already exits 4
(`conflict`).

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

`ui open` is an agent verb (see `status set`): the calling agent shows one
version of a UI in a new pane of the workspace's window, with `--data` (JSON,
`-` reads stdin) as its context's data. Each reply the user makes in it is
recorded and reaches that agent's running session as one JSON line,
`{"v":1,"kind":"ui_reply","ui","version","hash","open","value"}`; a reply
made while the session is not running stays in the record. `--as item` puts
it on canvas `C` instead, as a live item beside `--beside ITEM` (`i12`,
`c12`), `--size` tenths of a unit (`3200,2400` by default), as one undo step:
the user moves, deletes and undoes it like any item. Data that is not JSON
or over 16384 bytes, a size not above 0, or a window that holds 64 panes,
exits 5 (`invalid`); a UI, workspace, `--beside` pane or item that is not
there exits 3 (`not_found`). A refused open records nothing.

`canvas apply` takes one operation as a JSON object, or a list of them, as
one undo step (the operations are in `docs/service.md`, "Canvas
operations"); `-` reads them from stdin. `canvas schema` (and `canvas apply
--help`) prints the item kinds and operations, for an agent that cannot read
the docs. An `add` of kind `button` places
the "send to agent" button, which needs no UI put first:
`slinger canvas apply notes '{"op":"add","item":{"page":"p1","kind":"button","x":0,"y":0,"w":1600,"h":400}}'`.
`canvas apply`, `canvas undo` and `canvas redo` act as the agent `--agent`
names, else `SLINGER_AGENT`'s, so an agent's process acts as its agent with
its own undo history; with neither, the user acts.
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

`pane` changes the panes the server keeps for a workspace, the ones the app
shows: the app follows them, so `slinger pane open` from a terminal shows a
new pane in an open window. A pane is named by its number, as `pane list`
prints it. `pane open` shows a canvas or an agent UI that exists, beside pane
`--beside` (to its right, or below it with `--below`), or beside the whole
window; the new pane gets half the space. `pane close` gives the pane's space
to the pane beside it. Closing a pane that is already gone, or the last pane,
changes nothing and succeeds. `pane resize` moves the divider after pane
`PANE` (on its right, or below it) so its split gives its first side
`RATIO`, held in 0.1 to 0.9; the app's pen does the same when it drags a
divider. Resizing the last pane (no divider comes after it), one that is
not there, or to the share it has changes nothing and succeeds. A workspace that is not open, a canvas or UI
that is not there, or no pane `--beside` exits 3 (`not_found`); a window that
holds 64 panes exits 5 (`invalid`) on `pane open`. Each change is recorded as
a `pane_moved` event.

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
`file touched` is an agent verb: it records that the agent wrote the file at
`PATH` (relative to the current directory) as a `file_touched` event with its
path and hash, never its contents. `--hook` reads a Claude Code `PostToolUse`
hook's payload on stdin instead and takes the file from it; a payload that
names no file records nothing and exits 0. The hooks `slingerd` gives each
agent's process run it after every file write (`docs/service.md`, "Hooks").
A file that is not there exits 3 (`not_found`); both or neither of `PATH`
and `--hook` exits 2.
`canvas put` is an agent verb too: the agent answers on the canvas with
text, a box (`W,H` in tenths of a unit), a pen stroke (`--ink`: x,y pairs in
tenths, the first where it starts, each next the step from the one before,
as `pts` in `canvas schema`) or an arrow between two items, placed beside
`--beside`, an id from `canvas show` (`i12`) or a packet (`c12` is the whole
cluster), where it covers nothing (`docs/service.md`, "Agent canvas verbs").
`--color` draws it in `#rrggbb` or `#rrggbbaa` (default black). `--at X,Y`
puts the top-left corner of its box there instead, in tenths, on the page of
`--beside`; an arrow takes no `--at`. After a sling to that agent on that
canvas, the put also appends `reply_landed` to the record. An item that is
not there exits 3 (`not_found`); none or two of `--text`, `--box`, `--ink`
and `--arrow`, a `--box` or `--at` that is not two integers, `--ink` that is
not pairs of integers, or `--at` with `--arrow`, exits 2 (`usage`); blank
text or a colour that is not one exits 5 (`invalid`).
`canvas edit` and `canvas remove` are agent verbs for items the agent made:
`edit` gives a text new words, a text or a box a new size (`--box W,H`), or
moves the item's top-left corner to `--move X,Y`, all in tenths (the ends
of the agent's arrows bound to it follow); `remove`
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
after a restart. If that conversation can no longer be resumed, the process
exits and `session show` has it `exited`; the next `session start` begins a
new conversation. `session start --fresh` begins a new one at once, ending
a running process first. `session send` delivers one message to it (`-` reads it from
stdin) and returns once it is delivered; `session show` has the process's
state and its last answer. A send to an agent that was never started, or
whose process exited, exits 6 (`unavailable`) with a hint to start it.
`session stop` ends the process; stopping one that is not running succeeds.
A workspace that is not open exits 3, one with no agent exits 5. The process
gets `SLINGER_AGENT` and `SLINGER_SOCKET`, so its `slinger` calls act as its
agent on the daemon that started it (see "The process" in
`docs/service.md`).

`sling send` sends what changed on a canvas page since its last sling, the
selection (`--select c42,t12`, ids as `canvas read` takes them) and the
prompt (`-` reads it from stdin; none is valid: the selection is the
question) to the agent of the canvas's open workspace, and returns once it
is delivered. The answer arrives on the canvas, as the agent's `canvas put`s,
and as its status. Each sling is in the record (`sling_sent`, with its packet
as a blob). With no running agent it exits 6 (`unavailable`) with a hint to
`session start` it, and sends and records nothing; slinger never starts one
for a sling. A canvas no open workspace has exits 3, a workspace with no
agent or an id that is not one exits 5.

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
{ "success": true,  "command": "canvas create", "canvas": "main" }
{ "success": false, "command": "canvas create", "error": "\"Main\" is not a canvas id", "code": "invalid", "hint": "use 1 to 64 of a-z, 0-9, '-', '_' and '.', not starting with '.'" }
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
