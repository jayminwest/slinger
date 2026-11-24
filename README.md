# slinger

One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.

## Why

Agent tools are chat windows: you type text, and text comes back. Slinger is one window where you think with a pen on a canvas and agents do the work. You point at anything in a pane, add a prompt and sling it to an agent that is already running. The agent answers with things (marks on the canvas, files, a UI it builds), not with chat. It adds to the terminal, multiplexer and editor you use now, and it is built for one person. The design is in `SPEC.md`.

## Quickstart

On a Mac, build slinger and install it:

```bash
scripts/bundle.sh --install   # slinger.app to /Applications; links slinger and slingerd in ~/.local/bin to it
```

Then open slinger from Spotlight or the Dock. With no daemon running, the app starts the `slingerd` inside it; the window says "Starting slingerd…", then shows your canvases. slingerd keeps running after you quit the app, so agents keep working, and the app starts it again if it stops by itself. Its log is `slingerd.log` in the data directory; `slinger daemon info` names it. The protocol is in [docs/service.md](docs/service.md).

The CLI is `~/.local/bin/slinger`, a link into the app. Put `~/.local/bin` on your `PATH`, then:

```bash
slinger canvas create main
slinger --help
```

The bundle holds `slingerd` and `slinger` beside the app, so one install holds one copy of each: `--install` replaces whatever is at `~/.local/bin/slingerd` and `~/.local/bin/slinger` (a copy from an older install, or a link) with a link into `/Applications/slinger.app`.

To update, run `scripts/bundle.sh --install` again and reopen the app. The install stops and starts nothing, so the slingerd that runs keeps the old build. The app sees this and offers "Restart slingerd", naming both builds and the running agents a restart ends. Accept it when those agents can stop.

Teach your agents to use it. In Claude Code, install the plugin:

```
/plugin marketplace add jayminwest/slinger
/plugin install slinger@slinger
```

For another harness, `slinger skill install --dir <its skills directory>` writes the same skill. The skill sends the agent to `slinger guide`, which the binary prints, so the guide always fits the installed commands.

Run the bundle, not the bare `slinger-app` binary (from `cargo run`, a shell or ssh). Only the bundle starts a daemon: a bare binary waits for one. And whenever the app reads your files (opening a workspace, a PDF or a drawing, or exporting a UI), macOS gives folder access (Desktop, Documents, Downloads) to the bundle, which says why it asks. A bare binary gets none of its own, so its open panel can show those folders empty or refuse them.

On a host with no app (Linux, a server), install the CLI and the daemon with `cargo install --locked --git https://github.com/jayminwest/slinger slinger slingerd` and run `slingerd` under a service manager ([docs/service.md](docs/service.md), "Running it").

## Without a pen

The pen comes first, but a keyboard and a mouse or trackpad do the whole job. The keys below are the defaults; `cmd+/` lists every one. `[keys]` in [config.example.toml](config.example.toml) rebinds the commands; the keys inside a text and the pans are fixed.

- **Write**: ctrl+return starts a text at the pointer, or half a line under the one you are typing, so notes follow one another. With slinger's own editor (`[editor] nvim = "off"`, or no nvim found) the caret moves as in any Mac field: the arrows (alt by words, cmd to the row's ends), shift to select, delete and forward delete, cmd+A, copy, cut and paste, the emacs ctrl keys, a double tap for a word and a triple tap for a row; drag the right edge of the text you are typing to set where it wraps. Escape ends it.
- **Size**: cmd+shift+A picks the size a new text starts at; alt+shift+} and alt+shift+{ (shift with the bracket keys) step it. While a text is typed, or with texts selected, they resize those instead. The footer shows the size while the text tool is chosen.
- **Select and move**: a mouse press with the pen tool selects and drags items instead of drawing (`[pointer] press = "tool"` draws). With no text typed, cmd+A selects every item, tab and shift+tab step through them, the arrows nudge the selection, cmd+J duplicates it and delete removes it.
- **Find**: cmd+F finds typed text on any page of the canvas and selects it; cmd+G goes to the next.
- **Pan and zoom**: hold space and drag, drag with the middle or right button, or scroll with two fingers; pinch, or scroll with cmd held, to zoom. cmd+9 zooms to fit the page (inside a box, the box); a canvas you have not looked at yet opens that way when its ink is out of view.
- **Sling**: cmd+return slings the selection with a prompt, alt+return at once.

### The writing column

A canvas text is a place to write several paragraphs of markdown, with your own nvim:

- **nvim types it.** When nvim 0.10 or later is on the app's `PATH` or in the usual places (Homebrew, nix), every canvas text is edited in it, with your config and plugins, drawn in the text's place 80 cells wide with no line numbers or ruler. A new text starts in insert mode. Escape in normal mode (escape twice from insert), `:q` or a tap outside ends the text; ctrl+return ends it and starts the next. Every key without cmd is nvim's; cmd+Z and shift+cmd+Z are nvim's undo and redo, cmd+A selects the whole text, cmd+C and cmd+X copy and cut what visual mode selects, cmd+V and dictation paste at nvim's cursor, and a click or drag is nvim's mouse. nvim's state and cache go to slinger's own folder in the temporary directory, never yours. `[editor] nvim` in [config.example.toml](config.example.toml) picks the nvim, or turns it off.
- **The column.** A text is 80 characters wide and grows to 80 lines; a longer one fades and says "N more lines". Two quick taps on that row, or Step in (opt+cmd+I) with it selected, show it whole; escape steps back.
- **Markdown.** A text you are not typing in shows its markdown in the same cells (headings and emphasis bold or italic, code on a tint, checkboxes as ☐ and ☑, the marks dimmed), so no word moves when you open it. A double tap on a word opens the text with the cursor on that word.
- **Write focus.** opt+cmd+return zooms the pane so the column you are typing fills it; ending the text, or the same key, puts the view back.
- **Agents answer beside it.** An agent never changes the words of a text you wrote; it puts a short text (at most 12 rows) beside it.

## Development

`cargo xtask check-all` runs every quality gate, the same way the pre-commit hook and CI do.
Agents start at [AGENTS.md](AGENTS.md). The design rules are in [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md).

This is a personal project. It does not take issues or outside pull requests. Fork it freely.

## Status

Early (`0.0.0`). See [CHANGELOG.md](CHANGELOG.md).

## License

[MIT](LICENSE)
