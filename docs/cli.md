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
| `--socket <PATH>` | The control socket of `slingerd`. Default: see `docs/service.md` |
No arguments prints help. Unknown commands and flags are usage errors with
a "did you mean" suggestion.

## Commands

| Command | Prints |
|---|---|
| `hello [NAME]` | The greeting |
| `record append <KIND> [PAYLOAD] [--source NAME] [--workspace NAME] [--agent NAME] [--canvas ID]` | The new event's number. `--json`: `event` |
| `record list [--kind KIND] [--machine NAME] [--workspace NAME] [--agent NAME] [--canvas ID] [--after NUMBER]` | One line per event: number, kind, tags (`name=value`) and payload, separated by tabs. `--json`: `events` |
| `blob put <FILE\|->` | The blob id (`sha256:<hex>`). `--json`: `id` |
| `blob get <ID> [--out FILE]` | The blob's bytes, exactly, on stdout. With `--out`: nothing, and the file holds the bytes. `--json` (needs `--out`): `id`, `size` |

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
| 6 | unavailable | `slingerd` does not answer. The hint says how to start it |
Add a code only when a caller needs to tell the failure apart from the
others. Never renumber.

## Adding a command

1. Add the verb to the daemon: see "Adding a verb" in `docs/service.md`.
2. Add a variant to `Command` in `crates/cli/src/cli.rs`. Its doc comment is its help text.
3. Add a match arm that calls the client method and returns `Output { data, human }`.
   The CLI never calls the core.
4. Add tests to `crates/cli/src/tests.rs`. They run against a real daemon.
