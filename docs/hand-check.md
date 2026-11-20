# Hand checks

Checks that need the owner, the bundled app (`scripts/bundle.sh && open target/slinger.app`) or real hardware. First check what a driven window can show (`slinger drive`; `slinger guide` says how). An agent that ships something only a person can check adds one line here, with the commit, under the right heading. It does not file a seed. The owner ticks a line or deletes it. Each line keeps the id of the seed it came from; `sd show <id>` gives its history.

## Switching and views (slinger-eaa98b)

- [ ] (bundle) `open target/slinger.app --env SLINGER_TRACE=switch --stderr <file>`, tap between two workspaces ten times, paste the lines in slinger-eaa98b: the baseline in the shown window (a driven window measured 818 to 1925 ms to drawn, of which under 5 ms was the daemon) (slinger-9fd400)

## Fold and boxes (plan pl-f130a2)

- [ ] in the bundle: fold, peek, step in, split to compare, nest, move and sling boxes, with the pen and with the keyboard (slinger-03095f)

## Pen-less use and text (plan pl-c5e2a3)

- [ ] (rest of 7ada2a) a day of notes in the BUNDLE with keyboard and trackpad only: cmd+/ shows 'Typing a text' and 'Pointer' sections (also in a narrow window; fixed chips may crowd); ctrl+return chains notes, caret keys, shift-select, double/triple tap, escape; a trackpad press with the pen tool selects, no stray ink; cmd+A, tab, arrow nudge, cmd+J, delete on items; cmd+F/cmd+G; size keys and footer 'Size N'; space-drag, middle/right drag, cmd+scroll; log gaps with leave-feedback (slinger-75514e)
- [ ] on real hardware: the pointer (51c0274, 669d5ca): a Wacom and a Sidecar pen still draw (events carry a tablet subtype), the footer/cursor before the first proximity event, space-drag and middle-button pan, the hand cursors, space key-up on losing focus; a plain 'space' binding in [keys] now loses to the hand (slinger-de037f)
- [ ] in the bundle: the caret in a canvas text (e36f600): caret x against the glyphs (kerning may put it a point off), shift-selection, word and row jumps, cut/copy/paste at the caret, dictation at the caret (slinger-c068f0)
- [ ] in the bundle: double/triple tap in a typed text with the pen and the trackpad (DOUBLE_TAP 0.4 s), and that ctrl+letters (emacs keys) reach the text before the system takes them (slinger-949514)

## PDF (plan pl-d2285b)

- [ ] sling over a PDF in the real app: backdrop and words line up with the ink in a live sling, on rotated pages and on cropped pdf items (only unit tests and one rendered image were checked) (slinger-9fd6f0)
- [ ] crop-pdf in the real app: cmd+8, drag on a PDF page in reader mode shows the dashed box; cmd+V on an ordinary canvas shows the cropped region at the right aspect; select moves and resizes it keeping aspect; a crop of a pasted snippet maps correctly (slinger-7105bd)

## Launcher (plan pl-ac18df)

- [ ] from a slingerd that the bundled app started (once slinger-5e8b60 lands; until then `env -i HOME=$HOME USER=$USER SHELL=$SHELL PATH=/usr/bin:/bin:/usr/sbin:/sbin slingerd` with no --claude), start an agent and sling once: Claude Code is found, its hooks report, its Bash tool finds git/cargo/sd, its MCP servers load (slinger-be3082)

## Agent sessions, workspaces and the rest

- [ ] the file panels in the BUNDLED app after 01bd66f (scripts/bundle.sh && open target/slinger.app, never a bare binary or ssh): cmd+O reaches Downloads (allow the macOS prompt) and opens a PDF; shift+cmd+D / shift+cmd+O / cmd+V in Go-to-folder work in the panel; the app menu bar returns after; Open Workspace and Export UI show their message and button (rest of 03cad2) (slinger-4e7b98)
- [ ] start-agent (opt+cmd+A), stop-agent (opt+cmd+.), restart-agent (opt+shift+cmd+A) in the real app: Tools menu, palette, the sidebar's 'Start agent' line, the flash; does opt+cmd+. clash with a system or tablet key (35e7d5, 5f4959, dd0783) (slinger-0879be)
- [ ] canvas text after f66a97 in the real app: an agent's long paragraph breaks as 'canvas crop' does and ends at the 'canvas put' box; typed text size/spacing, caret and dashed frame on multi-line text; descenders and accents not clipped; glyphs are ~17% smaller (system font at 0.83, an empirical number) (slinger-53c684)
- [ ] run scripts/e2e-sling.sh after 29bb9f: a session under no profile now gets no --tools and no --strict-mcp-config; check MCP servers load in print mode, .mcp.json approval, that dontAsk lets Agent and TodoWrite run, and the token cost per sling against SPEC 14 (slinger-881ffd)
- [ ] the sling pill in the real app: ticks each second focused and unfocused, sits at the top right of its pane (confirm the place; one function scene/slung/out.rs place), an answered pill goes after 5 s or at the next stroke (7d5db9, b015f9, 9fe67e); and a sling sent mid-turn no longer leaves 'working' stuck (b84853) (slinger-ad02a1)
- [ ] (bundle) click an agent UI page or live item, then start a canvas text and press cmd+V / dictate: the paste lands (e41e9ee; a cause for 145162 shown in code only); after clicking the canvas an agent UI page no longer has the keyboard until clicked again; then redo slinger-29364d (slinger-54dc4a)
- [ ] restart the real slingerd on this build, then from a Claude Code session run slinger with no --socket: it finds the daemon by the 'socket' file in the data directory (ac82eb3); a daemon that dies uncleanly leaves the file pointing at a dead path until the next start (slinger-29a9a9)
- [ ] after 6debdd7 run scripts/e2e-sling.sh: bare Skill/WebSearch/WebFetch allow rules approve under dontAsk; unattended sessions with the owner's full user config, plugins and hooks loaded (slinger-7852d4)
- [ ] nameless broken workspaces (tap one of two: only it highlights, Close workspace closes it), [theme] change restyles an open agent UI pane and a live item without reload, Export agent UI… writes <name>.v<version>.html and shows it in Finder (1f80ad4, 2187a70, 58f1c8c) (slinger-faa11a)
- [ ] undo/redo across a daemon restart (4f0b375) — draw, undo, restart slingerd, redo; confirm the app ignores the new step/undoes/redoes fields on change-feed lines; canvases saved before have nothing to undo after a restart (slinger-3aeae2)
- [ ] in the real app: cmd+O panel picks .tldraw/.tldr and imports beside the focused pane, a dropped .tldraw does the same, 'Show workspace folder in Finder' (File menu, palette), the renamed 'Open PDF or tldraw…' item (190b1b, 99061d) (slinger-726f8d)
- [ ] an agent UI (live item and UI pane) on a dark and a light theme: sits on the paper, no white flash before load, readable defaults; a lone broken workspace shows its close line only after a tap; start/stop-agent with a broken first workspace act on the one the panes show (3169bc, 299e8f) (slinger-15dc2b)
- [ ] the pill's '~X.xK' follows a real session's context (a1d9b4 assumed Claude Code's stream fields: assistant message.usage input/cache_creation/cache_read/output tokens, parent_tool_use_id for subagents; never run against real Claude Code) (slinger-b3e718)
- [ ] in real Claude Code, 'slinger session listen --pid $PPID' names the session process (not a throwaway shell or a sandbox pid); kill a listening session mid-sling and see the sling go to the next listener (fdb9fe) (slinger-7c3149)
- [ ] drop a PDF on the real app window: the drag reaches the window, opens in the pane under the drop point; known gaps: a drop over an agent UI pane or the sling entry goes to that view, copy cursor shows for any file, several PDFs open only the first (fd53aa) (slinger-e8d373)
- [ ] open-workspace in the real app (shift+cmd+O, File menu, sidebar '+'): folder panel is modal, New Folder works, cancel does nothing, the '+' sits right; confirm the choices: folder-only, named after the folder, window turns to the new workspace (5fe947) (slinger-950528)
- [ ] (bundle) with the sling entry open, click an agent UI page: the entry keeps the keyboard with its caret at the end (e55ad8a); cmd+V with nothing usable or nowhere to paste flashes why instead of beeping, incl. a greyed cmd+V (95b8014 assumes a disabled key equivalent reaches the window as a keyDown) (slinger-a57413)
- [ ] 'Show feedback file in Finder' and 'Show all feedback…' (Help menu, palette; 1b5b0a3, 4866f28): names, placement, resolved rows read well (slinger-b4017e)
- [ ] (bundle) type in a canvas text, open leave-feedback: only the entry's blinking caret shows and the text is saved (2247395, cause inferred from code); if the odd cursor was something else, next candidate is the mouse pointer over an entry (platform/canvas.rs sets its tool cursor over the field's I-beam) (rest of b298f6) (slinger-290586)
- [ ] quit the real app while it shows a non-first workspace, relaunch: that workspace is shown (fa2e551; saved in the app's user defaults via an unsafe NSUserDefaults call never run for real) (slinger-d00b38)
- [ ] close-workspace in the real app (opt+cmd+W, File menu, palette) and the 'Close workspace' line under a shown broken workspace (c75719) (slinger-14fd9a)
- [ ] a broken workspace in the real sidebar shows the red mark, error, field and hint, wrapped to 200 pt; a long hint at the sidebar's foot (a24272) (slinger-31b207)
- [ ] IME input (Japanese/Chinese composition, dead-key accents) in the sling entry (slinger-368f56)
- [ ] look at a real 'canvas crop' of the new picture glyphs (stars, boxes, arrows, warning, katakana drawn as hiragana, the empty box for unknown characters) (ac948a) (slinger-95e85b)
- [ ] open the app once with no saved window frame (a copy of the binary under another name, or delete the 'NSWindow Frame slinger window' default) and confirm it opens 960 wide; 70d6d7 was closed as not reproducible (slinger-f7bb8d)
