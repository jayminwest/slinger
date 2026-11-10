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
| `agent.name` | yes, in `[agent]` | The agent's name, as the server's session list knows it. |
| `agent.durable` | no | `true` when a harness keeps the agent alive (D20). Defaults to `false`. |
| `[[root]]` | no | One table per directory. Zero or more. |
| `root.machine` | yes, in a root | The machine the directory is on. |
| `root.path` | yes, in a root | An absolute path on that machine. A trailing `/` does not count. |
| `root.primary` | no | `true` for the primary root. Defaults to `false`. |
| `[layout]` | no | The pane layout. Absent means unset: the app picks one. |
| `layout.version` | yes, in `[layout]` | The layout's own format version. |

Everything else in `[layout]` belongs to the app shell. The core stores it
and writes it back without reading it.

## Rules

For one workspace (a file that breaks one is rejected as `invalid`):

- At most one root is primary.
- A workspace with a durable agent has exactly one primary root. That
  directory is the agent's home.

Over the set of workspaces the server knows:

- One directory (the same machine and path) is the primary root of at most
  one workspace. A second one is a `conflict` that names the first. The same
  directory can be a plain root of any number of workspaces.

## Compatibility (B5)

- A reader keeps every field it does not know, at any level, and writes it
  back. An older Slinger never drops what a newer one wrote.
- A reader reads a file with a higher `version` than its own. Changes to this
  format only add optional fields, so no change breaks a reader.
- Writing keeps the version the file was read with. A new workspace gets the
  current version.
- Writing does not keep comments or the order of unknown fields. Slinger
  writes the file only when it makes or changes a workspace.
