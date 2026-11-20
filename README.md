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
cargo run --release --bin slingerd &   # the record goes in the default data directory
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
scripts/bundle.sh --install   # the app to /Applications, slingerd and slinger to ~/.local/bin
```

Run the bundle, not the bare `slinger-app` binary (from `cargo run`, a shell or ssh), whenever the app reads your files: opening a workspace, a PDF or a drawing, or exporting a UI. macOS gives folder access (Desktop, Documents, Downloads) to the bundle, which says why it asks. A bare binary gets none of its own, so its open panel can show those folders empty or refuse them.

The app does not start the daemon: run `slingerd` first, or at login from a service manager ([docs/service.md](docs/service.md), "Running it as a system service"). The daemon refuses a data directory in a temporary directory unless it is given `--scratch`, because the system empties those.

## Without a pen

The pen comes first, but a keyboard and a mouse or trackpad do the whole job. The keys below are the defaults; `cmd+/` lists every one. `[keys]` in [config.example.toml](config.example.toml) rebinds the commands; the keys inside a text and the pans are fixed.

- **Write**: ctrl+return starts a text at the pointer, or half a line under the one you are typing, so notes follow one another. Inside a text the caret moves as in any Mac field: the arrows (alt by words, cmd to the row's ends), shift to select, delete and forward delete, cmd+A, copy, cut and paste, the emacs ctrl keys, a double tap for a word and a triple tap for a row. Escape ends it.
- **Size**: cmd+shift+A picks the size a new text starts at; alt+shift+} and alt+shift+{ (shift with the bracket keys) step it. While a text is typed, or with texts selected, they resize those instead. The footer shows the size while the text tool is chosen.
- **Select and move**: a mouse press with the pen tool selects and drags items instead of drawing (`[pointer] press = "tool"` draws). With no text typed, cmd+A selects every item, tab and shift+tab step through them, the arrows nudge the selection, cmd+J duplicates it and delete removes it.
- **Find**: cmd+F finds typed text on any page of the canvas and selects it; cmd+G goes to the next.
- **Pan and zoom**: hold space and drag, drag with the middle or right button, or scroll with two fingers; pinch, or scroll with cmd held, to zoom.
- **Sling**: cmd+return slings the selection with a prompt, alt+return at once.

## Development

`cargo xtask check-all` runs every quality gate, the same way the pre-commit hook and CI do.
Agents start at [AGENTS.md](AGENTS.md). The design rules are in [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md).

This is a personal project. It does not take issues or outside pull requests. Fork it freely.

## Status

Early (`0.0.0`). See [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE)
