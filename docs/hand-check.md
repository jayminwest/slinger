# Hand checks

Checks that need the owner, the bundled app (`scripts/bundle.sh && open target/slinger.app`) or real hardware. First check what a driven window can show (`slinger drive`; `slinger guide` says how). An agent that ships something only a person can check adds one line here, with the commit, under the right heading. It does not file a seed. The owner ticks a line or deletes it. Each line keeps the id of the seed it came from; `sd show <id>` gives its history.

## Switching and views (slinger-eaa98b)

- [ ] (bundle) `open target/slinger.app --env SLINGER_TRACE=switch --stderr <file>`, tap between two workspaces ten times, paste the lines in slinger-eaa98b: the baseline in the shown window (a driven window measured 818 to 1925 ms to drawn, of which under 5 ms was the daemon) (slinger-9fd400)
- [ ] (bundle) an agent's `slinger canvas put` appears with no half-second step, also while a pen stroke or a divider drag is under way; the trace's taken and heard come a few ms after layout and shown; the app idles at no more CPU than before. Driven window after slinger-1e161b: drawn mean 990 ms (945-1035), of which the watch's wait ~970 ms (step 3's); taken and heard 0-35 ms after layout and shown; a `canvas apply` showed in the scene 24-26 ms later (slinger-1e161b)
- [ ] (bundle) with SLINGER_TRACE=switch: tap between two workspaces ten times; 'drawn' is under about 50 ms warm; paste the ten lines in slinger-eaa98b beside the baseline. Driven window after slinger-2c5d60: drawn mean 18 ms (11-27), watch +0 ms, layout +0 ms, taken 4-12 ms (slinger-2c5d60)

## Pen-less use and text (plan pl-c5e2a3)

- [ ] (rest of 7ada2a) a day of notes in the BUNDLE with keyboard and trackpad only: how it feels; log gaps with leave-feedback; the real trackpad's middle/right drag and cmd+scroll (a driven window passed the rest; it found the key overlay's chips cut to '⌥⌘…' at 960 pt: Close workspace, Focus pane left/right, Remove page, Restart agent, and labels 'Select a word, a r…', 'Set where it wra…') (slinger-75514e)
- [ ] on real hardware: the pointer (51c0274, 669d5ca): a Wacom and a Sidecar pen still draw (events carry a tablet subtype), the footer/cursor before the first proximity event, space-drag and middle-button pan, the hand cursors, space key-up on losing focus; a plain 'space' binding in [keys] now loses to the hand (slinger-de037f)
- [ ] in the bundle: dictation at the caret of a canvas text (e36f600; a driven window passed caret place, shift-selection, word and row jumps, cut/copy/paste) (slinger-c068f0)
- [ ] in the bundle: ctrl+letters (emacs keys) reach a typed text before the system takes them (a driven window passed double/triple tap with pen and mouse, and ctrl+A/K/Y) (slinger-949514)

## PDF (plan pl-d2285b)

- [ ] sling over a PDF: the sling image's backdrop lines up with the ink (a driven window passed the words: ink over a cropped pdf item and a rotated page's crop gets the words under it) (slinger-9fd6f0)

## Launcher (plan pl-ac18df)

- [ ] `scripts/bundle.sh --install` (checked with scratch SLINGER_APP_DIR and HOME only): `ls -l ~/.local/bin/slinger ~/.local/bin/slingerd` are links into /Applications/slinger.app; `slinger --version` runs through the link; an agent session's hooks still find slinger (`slinger daemon info` names the bundled one); About slinger shows the version and the build number (slinger-0d265b)
- [ ] restart offer (slinger-866657): install a newer build with `scripts/bundle.sh --install` while slingerd runs with an agent working, quit and reopen the app: the notice names both builds and "ends 1 running agent"; Restart slingerd shows "Starting slingerd…" and brings the window back on the new daemon (`slinger daemon info` shows the new build and the exe inside the app); canvases and waiting slings are intact; ✕ hides the offer until the next different daemon (launcher, skew and offer tests passed; a driven window never manages a daemon, so it cannot show the offer)
- [ ] read README "Quickstart" and docs/service.md "Running it" against the install above: every command works as written and nothing tells you to start the daemon from a terminal (slinger-5f71e8)
- [ ] after `scripts/bundle.sh --install` the icon shows in the Dock, cmd+tab, Finder and Spotlight and reads at the smallest size (macOS may cache the old one: `touch /Applications/slinger.app`) (slinger-d8fb56)

- [ ] first launch (slinger-5e8b60): with no slingerd running, open /Applications/slinger.app: "Starting slingerd…" then the canvases, no terminal used; quit the app: slingerd and a working agent keep running; reopen: the window finds it; `slinger daemon info` names the exe inside the app and the log `slingerd.log` in the data directory, which holds its start line (scene and launcher tests passed; a driven window never starts a daemon by design) (slinger-5e8b60)
- [ ] with the bundled app open, `kill -9` its slingerd: "Starting slingerd…" and it is back within seconds, and ink drawn meanwhile is sent; `slinger daemon stop` instead: "slingerd is stopped" and the sidebar's Start slingerd brings it back (slinger-5e8b60)
- [ ] open two copies of the app at once with no daemon (`open -n` twice): one slingerd runs (`pgrep -l slingerd`), neither window shows "did not start" (slinger-5e8b60)
- [ ] a workspace under ~/Documents opened through the app-started daemon: which macOS prompt appears and for whom (slinger.app or slingerd); does the grant survive a reinstall (the bundle is signed ad hoc) (slinger-5e8b60)
- [ ] from a slingerd that the bundled app started, start an agent and sling once: Claude Code is found, its hooks report, its Bash tool finds git/cargo/sd, its MCP servers load (slinger-be3082)

## Agent sessions, workspaces and the rest

- [ ] the file panels in the BUNDLED app after 01bd66f (scripts/bundle.sh && open target/slinger.app, never a bare binary or ssh): cmd+O reaches Downloads (allow the macOS prompt) and opens a PDF; shift+cmd+D / shift+cmd+O / cmd+V in Go-to-folder work in the panel; the app menu bar returns after; Open Workspace and Export UI show their message and button (rest of 03cad2) (slinger-4e7b98)
- [ ] start-agent, stop-agent, restart-agent in the real app: the Tools menu items; does opt+cmd+. clash with a system or tablet key (35e7d5, 5f4959, dd0783; a driven window passed keys, palette, sidebar line and flash) (slinger-0879be)
- [ ] canvas text after f66a97: are glyphs ~17% smaller right (system font at 0.83, an empirical number)? (a driven window passed paragraph breaks against 'canvas crop', the put box, multi-line caret and frame, descenders and accents) (slinger-53c684)
- [ ] run scripts/e2e-sling.sh after 29bb9f: a session under no profile now gets no --tools and no --strict-mcp-config; check MCP servers load in print mode, .mcp.json approval, that dontAsk lets Agent and TodoWrite run, and the token cost per sling against SPEC 14 (slinger-881ffd)
- [ ] (bundle) click an agent UI page or live item, then start a canvas text and press cmd+V / dictate: the paste lands (e41e9ee; a cause for 145162 shown in code only); after clicking the canvas an agent UI page no longer has the keyboard until clicked again; then redo slinger-29364d (slinger-54dc4a)
- [ ] restart the real slingerd on this build, then from a Claude Code session run slinger with no --socket: it finds the daemon by the 'socket' file in the data directory (ac82eb3); a daemon that dies uncleanly leaves the file pointing at a dead path until the next start (slinger-29a9a9)
- [ ] after 6debdd7 run scripts/e2e-sling.sh: bare Skill/WebSearch/WebFetch allow rules approve under dontAsk; unattended sessions with the owner's full user config, plugins and hooks loaded (slinger-7852d4)
- [ ] a [theme] change restyles an open agent UI pane and a live item without reload; Export agent UI… writes <name>.v<version>.html and shows it in Finder (1f80ad4, 2187a70, 58f1c8c; a driven window passed nameless broken workspaces) (slinger-faa11a)
- [ ] in the real app: cmd+O panel picks .tldraw/.tldr and imports beside the focused pane, 'Show workspace folder in Finder' (File menu, palette), the menu item 'Open PDF or tldraw…' (190b1b, 99061d; a driven window passed a dropped .tldraw) (slinger-726f8d)
- [ ] an agent UI (live item and UI pane) on a dark and a light theme: sits on the paper, no white flash before load, readable defaults (3169bc, 299e8f; a driven window passed the broken-workspace parts) (slinger-15dc2b)
- [ ] the pill's '~X.xK' follows a real session's context (a1d9b4 assumed Claude Code's stream fields: assistant message.usage input/cache_creation/cache_read/output tokens, parent_tool_use_id for subagents; never run against real Claude Code) (slinger-b3e718)
- [ ] in real Claude Code, 'slinger session listen --pid $PPID' names the session process (not a throwaway shell or a sandbox pid); kill a listening session mid-sling and see the sling go to the next listener (fdb9fe) (slinger-7c3149)
- [ ] drop a PDF on the real app window: the drag reaches the window; known gaps: a drop over an agent UI pane or the sling entry goes to that view, copy cursor shows for any file, several PDFs open only the first (fd53aa; a driven window passed the open beside the pane under the drop) (slinger-e8d373)
- [ ] open-workspace in the real app (shift+cmd+O, File menu, sidebar '+'): folder panel is modal, New Folder works, cancel does nothing, the '+' sits right; confirm the choices: folder-only, named after the folder, window turns to the new workspace (5fe947) (slinger-950528)
- [ ] (bundle) with the sling entry open, click an agent UI page: the entry keeps the keyboard with its caret at the end (e55ad8a); cmd+V with nowhere to paste flashes why, incl. a greyed cmd+V (95b8014 assumes a disabled key equivalent reaches the window as a keyDown; a driven window passed an empty clipboard) (slinger-a57413)
- [ ] 'Show feedback file in Finder' and 'Show all feedback…' (Help menu, palette; 1b5b0a3, 4866f28): names, placement, resolved rows read well (slinger-b4017e)
- [ ] (bundle) type in a canvas text, open leave-feedback: the mouse pointer over the entry (platform/canvas.rs sets its tool cursor over the field's I-beam) (rest of b298f6; a driven window passed: only the entry's caret, the text saved) (slinger-290586)
- [ ] quit the real app while it shows a non-first workspace, relaunch: that workspace is shown (fa2e551; saved in the app's user defaults via an unsafe NSUserDefaults call never run for real) (slinger-d00b38)
- [ ] FAILED in a driven window (slinger-585af9): a broken workspace's long hint is cut at the sidebar's foot and the sidebar does not scroll, so rows below it (even the shown workspace) cannot be seen; and 'line 1: extra `=`, expected' drops 'expected' (laid out as one 160 pt row, drawn wider). Red mark, error, field and the 200 pt wrap passed (a24272) (slinger-31b207)
- [ ] IME input (Japanese/Chinese composition, dead-key accents) in the sling entry (slinger-368f56)
- [ ] open the app once with no saved window frame (a copy of the binary under another name, or delete the 'NSWindow Frame slinger window' default) and confirm it opens 960 wide; 70d6d7 was closed as not reproducible (slinger-f7bb8d)
