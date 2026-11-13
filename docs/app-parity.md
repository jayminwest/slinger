# What the CLI can do that the app cannot

Status: audited 2026-11-12 (slinger-ce9df8) against `docs/cli.md`,
`docs/service.md` and the app's command registry
(`crates/app/src/commands.rs`), menu bar (`crates/app/src/commands/menu.rs`),
sidebar, footers, entries and direct manipulation on the canvas.

The rule: a command a person would reach for belongs in the app too. A new
CLI command or control verb comes with its app entry (a command in
`Command::ALL`, which puts it in the palette, the key overlay and the menu
bar, or a tap target), or with a row here saying why not. "Adding things"
in `ARCHITECTURE.md` points here.

Sizes, as the app is built today:

- **arm**: a command in the registry and a call the app's client already
  makes or can make with no new input from the user.
- **ui**: needs something new on screen: a text entry, a picker, a form.
- **design**: needs a decision on what the user sees or does first.

A new verb's call costs the app's binary about 700 bytes, unless its reply
type is new to the app: decoding a new reply type can cost many KB (a reply
full of maps cost about 10 KB), so read the reply into a type the app
already decodes where you can. The binary grows in 16 KB steps against its
ceiling (slinger-7af57c), so measure the room left in `__TEXT` before and
after a change, not the file size.

## Not for the app

These serve agents, scripts or the app's own plumbing. The person does the
same thing by drawing, or never needs to.

| CLI | Why not |
|---|---|
| `record append` | The app and the daemon append what happens; a person has nothing to log by hand |
| `blob put`, `blob get` | Plumbing: the app moves PDF bytes with them already |
| `ui put` | An agent writes UIs |
| `canvas show`, `changes`, `read`, `crop`, `strokes`, `schema` | How an agent reads a canvas; the app draws it |
| `canvas apply`, `undo`, `redo` | The pen, the eraser, select and lasso, Undo and Redo |
| `canvas put`, `edit`, `remove` | Agent verbs; the person draws, writes, moves and deletes |
| `status set`, `file touched`, `session listen` | Agent verbs: the agent's harness runs them |
| `guide`, `skill install` | For agents and their harness; no daemon verb behind them |
| `feedback resolve` | Done: a tap on a note's "✓" in the feedback entry |
| `--json`, `--quiet`, `--socket` | CLI output contract; the app's socket is `[daemon] socket` in `config.example.toml` |

## Already in the app

| CLI | App |
|---|---|
| `workspace open DIR` | `open-workspace` (shift+cmd+O, File menu, palette, sidebar "+"), 36e8844 |
| `workspace list` | The sidebar; a tap on a workspace shows its panes |
| `workspace close` | `close-workspace` (opt+cmd+W, File menu, palette; a broken workspace's "Close workspace" line), slinger-c75719 |
| `workspace show` (`dir`) | `show-workspace-folder` (File menu, palette; no default key): the Finder shows the folder; the sidebar already has it from `workspace_list`, so no new verb, slinger-190b1b |
| `session list`, `session show` | The sidebar's mark and note per agent; the sling pill and its answer ("tap to read") |
| `profile list` | The sling's profile menu lists the agent's profiles |
| `sling send` (`--page`, `--select`, `--prompt`, `--profile`) | `sling`, `sling-now`, `dictate`, the send-to-agent button, the profile menu |
| `session start` | `start-agent` (opt+cmd+A, Tools menu, palette; the sidebar's "Start agent" line under a shown workspace's agent that does not run), slinger-35e7d5 |
| `session start --fresh` | `restart-agent` (opt+shift+cmd+A, Tools menu, palette), slinger-dd0783 |
| `session stop` | `stop-agent` (opt+cmd+., Tools menu, palette), slinger-5f4959 |
| `session start --profile` | Implied: a sling under a picked profile starts the process under it |
| `pane open`, `close`, `resize`, `list` | `split-right`, `split-down`, `close-pane`, `grow-pane`, `shrink-pane`, a divider dragged with the pen |
| `feedback add`, `feedback ls` | `leave-feedback` (shift+cmd+F), with the recent notes under the entry |
| `ui open` (a person's reply) | A UI pane or live item takes the reply (`ui_reply`) |
| `canvas create --ops` of a PDF | `open-pdf` (cmd+O) and a PDF dropped on the window |
| `canvas import` | `open-pdf` (cmd+O) with a `.tldraw`/`.tldr` file, or one dropped on the window: imported, then shown beside the focused pane (`pane_open`), slinger-99061d |
| `canvas put --color` | `pick-colour` and recolouring a selection |

## Gaps

What the person cannot do in the app, the verb that already serves it, and
a rough size. Known ones name their seed.

| The person cannot | Verb | Size | Seed |
|---|---|---|---|
| Make a workspace with no folder, from a typed name | `workspace_open` (`name`) | design | slinger-59bf85 |
| Send the agent a plain message, with nothing from the canvas | `session_send` | ui | new |
| Set or remove an agent's profiles | `profile_set`, `profile_remove` | design | slinger-fbd4d3 |
| Show a stored agent UI in a pane | `pane_open` (`ui`), `ui_list` | ui | new |
| Make a new empty canvas, or show another canvas in a pane | `canvas_create`, `canvas_list`, `pane_open` (`canvas`) | design | slinger-16a66e |
| Pick the size of a new text, or resize a text's characters | `canvas_put`/`canvas_edit` `size` (agent side); the app's own text tool | design | slinger-34a2ae |
| Read the thinking record (what was slung, answered, touched) | `record_list` | design | slinger-895bde |
| Browse a UI's versions, or save one to a file | `ui_list`, `ui_show`, `ui_export` | ui | new |
| See resolved feedback notes, or find the feedback file | `feedback_list` (`all`), `feedback_path` | arm | new |

Notes on the new rows:

- **The agent's lifecycle** (`start-agent`, `stop-agent`,
  `restart-agent`) acts on the workspace the window shows. Start and
  restart reuse `session_start`, which the sling's profile menu already
  calls, so start cost the binary about 750 bytes and no new verb;
  `session_stop`, new to the app, cost about 700.
- **A plain message** reuses the sling entry's one-line text field; the
  question is whether it is a mode of the sling entry or its own command.
- **A UI in a pane** needs a picker over `ui_list`; the pane verbs are
  already in the app.
