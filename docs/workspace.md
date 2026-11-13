# Workspace definition file

Status: decided 2026-11-10 (slinger-8c3c). Closes O6 in SPEC.md. The core
module `crates/core/src/workspace.rs` owns this format; no other module
knows it. Sources: SPEC.md section 10, D16, P5, B5.

## Name and place

The file is `slinger-workspace.toml`, in the workspace's primary root, so it
syncs with the directory. A workspace with no root keeps the same text in
Slinger's config instead.

The name is visible, not a dotfile: the user edits it in place (P5), and it
says what it is. TOML because it is plain text a person can edit, it has
comments, and its tables map one to one onto the fields below.

## Example

The stored fixture is `crates/core/fixtures/workspace/slinger-workspace.toml`.

```toml
version = 1
name = "slinger"
tag = "slinger"
canvas = "main"

[agent]
name = "slinger-dev"
durable = true

[[root]]
machine = "my-laptop"
path = "/Users/me/Projects/slinger"
primary = true

[[root]]
machine = "server"
path = "/srv/slinger-data"

[layout]
version = 1
```

## Fields

| Field | Required | Meaning |
|---|---|---|
| `version` | yes | The format version, 1 or more. This document is version 1. |
| `name` | yes | The workspace's name. Not blank. |
| `tag` | no | The workspace's tag on events in the record (section 9). Defaults to `name`. |
| `canvas` | no | The default canvas, where you talk to the agent. Defaults to `main`. |
| `[agent]` | no | The agent that works here. |
| `agent.name` | yes, in `[agent]` | The agent's name, as the server's session list knows it. No `#`. |
| `agent.durable` | no | `true` when a harness keeps the agent alive (D20). Defaults to `false`. |
| `agent.user_config` | no | `true` to load the owner's own Claude Code settings and instructions into the agent's session. Defaults to `false`. |
| `agent.dirs` | no | More directories on slingerd's machine the session may read, beyond the roots. Absolute paths. |
| `agent.tools` | no | More Claude Code permission rules the session may use without asking, beyond the `slinger` CLI: `"Edit"`, `"Bash(git *)"`, `"mcp__github"`. Each is a rule slinger knows (below). |
| `[[agent.profile]]` | no | One table per profile of the agent: limits a session can run under instead of the agent's. Zero or more. |
| `profile.name` | yes, in a profile | The profile's name: `a-z`, `0-9`, `-` and `_`, at most 64 long. |
| `profile.dir` | no | The session's working directory on slingerd's machine. Absolute. Defaults to the agent's home. |
| `profile.tools` | no | The Claude Code permission rules a session under the profile may use, in place of `agent.tools`. Each names a tool slinger knows. |
| `[[root]]` | no | One table per directory. Zero or more. |
| `root.machine` | yes, in a root | The machine the directory is on. |
| `root.path` | yes, in a root | An absolute path on that machine. A trailing `/` does not count. |
| `root.primary` | no | `true` for the primary root. Defaults to `false`. |
| `[layout]` | no | The pane layout. Absent means unset: the app picks one. |
| `layout.version` | yes, in `[layout]` | The layout's own format version. |

The server keeps the panes here and changes them with the pane verbs
(`pane open`, `pane close`; docs/service.md, "Panes"), which the app uses
too. The core module `crates/core/src/workspace/panes.rs` owns this text.
Version 1 is `version = 1` and one string, `panes`: the pane tree in prefix
order, words joined by one space. A pane is `canvas:<id>` or `ui:<name>`; a
split is `row` (side by side) or `column` (stacked), the first side's share
(0.1 to 0.9; a share past that is held in it), then its first side and its
second. At most 64 panes. Focus is not kept: it is the window's own.

```toml
[layout]
version = 1
panes = "row 0.6 canvas:main column 0.5 ui:board canvas:notes"
```

A layout that cannot be read (another version, or text that is not a tree)
is one pane on the workspace's canvas. The first change to it replaces it
with version 1. Other fields in a version 1 `[layout]` are kept.

## The agent's limits

A session runs within limits (slinger-a33c; docs/service.md, "Limits"). With
no limit fields it has every Claude Code tool and the MCP servers its
settings name (slinger-29bb9f), but reads only the workspace's roots on
slingerd's machine, runs without asking only the `slinger` CLI and
read-only commands, writes files (the `Edit` and `Write` rules,
slinger-05ad22), and does not load the owner's own agent instructions.
Nobody answers a permission prompt, so a file it writes is not reviewed
first, and a tool no rule allows (a web fetch, an MCP tool) is denied. The
three `agent` fields above widen it:

```toml
[agent]
name = "scout"
user_config = true
dirs = ["/Users/me/reference"]
tools = ["Bash(git *)", "Bash(cargo *)", "WebFetch", "mcp__github"]
```

## Profiles

A profile (slinger-c8359a) is named limits the agent runs under when a
session start names it (docs/service.md, "Profiles"): it runs in the
profile's `dir`, and its `tools` take the place of `agent.tools` and of the
default `Edit` and `Write`, and it has only the built-in tools that read,
run commands and its `tools` name: a profile is how you opt into a
narrower session. The `slinger` CLI, `agent.dirs` and
`agent.user_config` stay. A start with no profile runs within the agent's
own limits, so a profile never changes a session unless it is asked for by
name. A profile with no `tools` (`look` below) cannot write: use one for a
session that only reads. `slinger profile set` writes them;
the user may edit them here too.

```toml
[agent]
name = "scout"

[[agent.profile]]
name = "dev"
dir = "/Users/me/Projects/app"
tools = ["Edit", "Write", "Bash(git *)", "Bash(cargo *)"]

[[agent.profile]]
name = "look"
```

A rule is a tool, alone or with a pattern in parentheses, and the tool is
one of `Bash`, `Edit`, `Glob`, `Grep`, `MultiEdit`, `NotebookEdit`, `Read`,
`WebFetch`, `WebSearch` and `Write`. A file with a rule slinger cannot read
is rejected whole, rather than run with a rule it does not understand.

## Rules

For one workspace (a file that breaks one is rejected as `invalid`):

- At most one root is primary.
- The agent's name has no `#` (it parts the agent from a profile in the
  key of a session's conversation). Each of its `dirs` is an absolute path,
  and each of its `tools` is a rule slinger knows: a tool of `Bash`, `Edit`,
  `Glob`, `Grep`, `MultiEdit`, `NotebookEdit`, `Read`, `WebFetch`,
  `WebSearch` and `Write`, alone or with a pattern in parentheses
  (`Bash(git *)`, `Bash(git:*)`, `Edit(src/**)`), or an MCP tool
  (`mcp__server`, `mcp__server__tool`, `mcp__server__*`). A rule it cannot
  read fails the file with a hint that lists the accepted forms (`Tool`,
  `Tool(pattern)`, `mcp__<server>`, `mcp__<server>__<tool>`,
  `mcp__<server>__*`) and the tools, and names the tool meant when only its
  case is wrong (`bash` for `Bash`); fix or drop it here.
- Each profile has a name of its own, its `dir` (when set) is an absolute
  path, and each of its `tools` is a rule slinger knows.
- A workspace with a durable agent has exactly one primary root. That
  directory is the agent's home.

A broken rule names the field that broke it (`agent.tools`, `root.path`);
text that is not TOML names its line. An open workspace whose file breaks a
rule stays open but is not served: it is listed with its error, and every
verb on it fails with that error, never with a default in its place. The
other workspaces serve as before. Fix the file and the next request reads
it.

Over the set of workspaces the server knows:

- One directory (the same machine and path) is the primary root of at most
  one workspace. A second one is a `conflict` that names the first. The same
  directory can be a plain root of any number of workspaces.
- One name, and one default canvas, belong to at most one workspace. A
  second one is a `conflict`, so `show` and `close` can name a workspace and
  a canvas belongs to one workspace.

## Making one

Slinger writes a new file when it opens a directory that has none, or a
workspace with no root (slinger-c5d7). The name is the one given, or else the
directory's last part. The canvas is the name made into an id (`slug`):
`a-z`, `0-9`, `-` and `_`, with every other run of characters one `-`, at most
64 long, and `workspace` when nothing is left. So `My Notes` opens on
`my-notes`, and two workspaces get two canvases. A canvas made for a workspace
starts with the "send to agent" button at its origin (SPEC section 8); one that
exists already is used as it is. An opened directory is the
primary root, on the machine that opened it. A workspace with no root keeps
its file in `workspaces/<slug>/` in slingerd's data directory.

## The tag on events

An event whose writer set no workspace tag takes the tag of the workspace
whose default canvas is the event's canvas, else of the one whose agent is
the event's agent. When no open workspace matches, or more than one does,
the event has no workspace tag.

## Compatibility (B5)

- A reader keeps every field it does not know, at any level, and writes it
  back. An older Slinger never drops what a newer one wrote.
- A reader reads a file with a higher `version` than its own. Changes to this
  format only add optional fields, so no change breaks a reader.
- Writing keeps the version the file was read with. A new workspace gets the
  current version.
- Writing does not keep comments or the order of unknown fields. Slinger
  writes the file only when it makes or changes a workspace.
