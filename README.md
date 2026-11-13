# slinger

One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.

## Why

Agent tools are chat windows: you type text, and text comes back. Slinger is one window where you think with a pen on a canvas and agents do the work. You point at anything in a pane, add a prompt and sling it to an agent that is already running. The agent answers with things (marks on the canvas, files, a UI it builds), not with chat. It adds to the terminal, multiplexer and editor you use now, and it is built for one person. The design is in `SPEC.md`.

## Quickstart

```bash
cargo install --locked --git https://github.com/jayminwest/slinger slinger
slinger --help
```

Start the daemon, then use the CLI. The protocol is in [docs/service.md](docs/service.md).

```bash
cargo run --release --bin slingerd &
cargo run --release --bin slinger -- canvas create main
```

Teach your agents to use it. In Claude Code, install the plugin:

```
/plugin marketplace add jayminwest/slinger
/plugin install slinger@slinger
```

For another harness, `slinger skill install --dir <its skills directory>` writes the same skill. The skill sends the agent to `slinger guide`, which the binary prints, so the guide always fits the installed commands.

Build and open the macOS app (it shows the daemon's answer):

```bash
scripts/bundle.sh && open target/slinger.app
```

Run the bundle, not the bare `slinger-app` binary (from `cargo run`, a shell or ssh), whenever the app reads your files: opening a workspace, a PDF or a drawing, or exporting a UI. macOS gives folder access (Desktop, Documents, Downloads) to the bundle, which says why it asks. A bare binary gets none of its own, so its open panel can show those folders empty or refuse them.

## Development

`cargo xtask check-all` runs every quality gate, the same way the pre-commit hook and CI do.
Agents start at [AGENTS.md](AGENTS.md). The design rules are in [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md).

This is a personal project. It does not take issues or outside pull requests. Fork it freely.

## Status

Early (`0.0.0`). See [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE)
