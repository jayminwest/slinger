# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/).

Entries are written for users: what changed for them, not how. Lead each
bullet with the area in bold.

## [Unreleased]

### Added

- **Canvas:** text can be any size. Drag a corner handle of a selection
  with text in it and the text grows or shrinks with it (from 4 to 400
  units). Agents give a text its size with `size` in `canvas apply`
  (`slinger canvas schema` shows it), and the pictures an agent reads draw
  each text at its own size. Canvases saved before read exactly as they did.

- **App:** rename a page and leave feedback with the pen alone. Their
  entries have buttons beside them: ✓ Rename or ✓ Save does what return
  does, ✕ what escape does, and Dictate has Flick write the text in, as
  in the sling's prompt (tap it again, ● Stop, when done). The dictate
  key works in these entries too.

- **Agents:** profiles per workspace. `slinger profile set <workspace> dev
  --dir ~/Projects/app --tool Edit --tool 'Bash(git *)'` gives the
  workspace's agent a named profile: a working directory and the Claude
  Code tools it may use there. `slinger session start <workspace> --profile
  dev` runs the agent under it, in a conversation of its own, and `slinger
  sling send --profile dev` sends only to an agent running under it. A
  start with no profile keeps the agent's own narrow limits, and a tool
  slinger does not know is refused rather than passed on. `profile list`
  and `profile remove` show and drop them; they live in
  `slinger-workspace.toml` under `[[agent.profile]]`.
- **Canvas:** `canvas apply` can add items and the arrows between them in
  one step: an arrow's `from` or `to` may be `$n`, the item at `made[n]`
  of the same request, made by an `add` before it.
- **Feedback:** the app's feedback entry (`leave-feedback`, ⌘⇧F) lists
  the five newest open notes under it, with when each was left, so you see
  what is already said before you add to it.
- **CLI:** `slinger session start <workspace> --fresh` starts the agent in a
  new conversation instead of resuming its last one, stopping a running
  agent first.
- **App:** copy, cut and paste on the canvas. With a selection, Edit >
  Copy (⌘C) puts its items on the clipboard (other apps get the text of
  its texts), Cut (⌘X) also removes them, and Edit > Paste (⌘V) adds them
  to the focused pane's page, centred in it and selected, on any page or
  canvas. Arrows pasted with the items at their ends stay joined to them.
  Undo takes a paste or a cut back in one step. Cut, Copy and
  Paste grey out in the Edit menu when they would do nothing, such as
  Copy with nothing selected or while typing a text. Plain text copied
  from another app pastes onto the page as a new text in the pen's
  colour.

- **CLI:** `slinger canvas put` takes `--color` (`#rrggbb`, so an agent can
  answer in yellow), `--at x,y` to put the item exactly where it wants
  instead of beside the item, and `--ink` to draw a pen stroke (a bracket,
  an underline, a check mark). The note in a first sling tells the agent.
- **CLI:** `slinger canvas schema` prints the item kinds (text, box,
  arrow, stroke, button, live) with their fields and the operations
  `canvas apply` takes, without a daemon; `canvas apply --help` shows the
  same table.
- **Packets:** the packet header shows about what it costs to read
  (`tokens=`), its images included, so you and the agent both see what a
  sling costs. It is a local estimate (a token per 4 characters, a token
  per 750 pixels of an image), not a tokenizer's count.

- **App:** strokes drawn while slingerd is down are no longer lost when
  the app quits. They are kept in `unsent-ink.json` next to slingerd's
  data (`~/Library/Application Support/slinger` by default) and sent the
  next time the app starts and reaches slingerd. Erases, moves and other
  edits made while it is down are still lost on quit.
- **App:** see where a canvas is saved. The window's title names the
  focused pane's canvas, with its file under it
  (`~/Library/Application Support/slinger/canvases/<id>.canvas` by
  default); cmd-click the title for the folders that hold it. There is
  nothing to save by hand, as slingerd writes every change as you make it:
  Save (`save`, ⌘S) says where it went, or why the last change has not
  reached the file yet. Show canvas file in Finder (`show-canvas-file`,
  ⌥⌘R) shows it there. Both are in the File menu and the palette.
- **CLI:** `canvas show --json` gives the canvas's file as `path`.

- **App:** a dotted grid on the canvas, like grid paper. Set `[canvas]
  background = "dots"` in the config to start with it, or run Dotted grid
  (`toggle-grid`, ⌘', also in the Canvas menu and the palette) to show or
  hide it; the toggle lasts until the app quits. `[canvas] grid` sets the
  distance between dots in canvas points: the default, 28.35, is 1 cm at
  100% on a screen of 72 points per inch (most Mac screens show more, so
  the dots look closer; `config.example.toml` says how to match your
  paper). The dots pan and zoom with the ink, thin out when you zoom far
  out, and are for you only: they are not items, and an agent never sees
  them.
- **App:** pen-sized buttons in each pane's footer, so the pen does most
  things without the keyboard. Tap the swatch, the width bars or the tool
  name to pick a colour, a width or a tool; undo and redo; ‹ and › to turn
  the page; − and + to zoom, and the zoom level to go back to 100%; split
  right, split down and close the pane. Tap the page name for a page menu:
  it lists the canvas's pages (tap one to show it) and offers New page,
  Rename page…, Place send button and Remove page. Each button acts on its
  own pane. A button lights under the pen and runs when you lift on it, so
  a press that slides off runs nothing. A narrow pane keeps the tool,
  colour, page and commands, and drops the rest.
- **App:** a Delete button beside the selection, after ➤ Sling and +
  prompt, removes the selected items with the pen (one undo puts them
  back). A tap on empty canvas still selects nothing.
- **App:** slinging to an agent takes one tap. Select with the lasso and
  tap ➤ Sling beside the selection, or tap ➤ Sling in the footer (it slings
  the selection, else what is new on the page), or press ⌥↩ (`sling-now`,
  also in the Tools menu and the palette). Nothing to type: a sling with no
  words sends `[sling] prompt` from the config, by default "Read what I
  wrote and do what it asks." Tap + prompt beside the selection (or ⌘↩) to
  type words first. A workspace with no agent says "No agent in this
  workspace" and sends nothing.
- **App:** a sling shows what the agent is doing, in a pill under where you
  slung: "● scout working · 4s", "■ scout blocked: <its question>", then
  "✓ scout answered · 2 new items" (tap to bring them into view), "✓ scout
  answered in text · tap to read" (tap to read the answer in a panel),
  "✓ scout is done, with nothing on the canvas", or "✗ scout stopped before
  it answered" with how to start it. The pill goes 30 s after the agent
  ends. The footer names the workspace's agent and its status.
- **App:** `[sling] prompt` in the config: the words a sling with nothing
  typed sends.
- **App:** a button to hide the sidebar at its foot, and one to show it in
  the bottom-left footer while it is hidden, at the same spot. Tap a
  workspace in the sidebar to show its panes.
- **App:** the window opens where you left it, at the size you left it,
  and the sidebar opens or stays hidden as you last left it. A `[window]
  sidebar` setting in the config still wins.

- **App:** `[theme] done` sets the colour of a done agent's mark in the
  sidebar (default `"#4cc36b"`, the green it always had).

### Changed

- **App:** a page menu with more pages than fit shows ▲ or ▼ at the end of
  its top or bottom page row while more pages are above or below.
- **App:** a footer button's tip now sits on a small panel with rounded
  corners and a hairline edge, like the menus.
- **App:** the selection's corner handles resize it, with the lasso or
  the select tool: drag one and the selection scales about the opposite
  corner. Strokes, boxes and arrows stretch; texts and agent UIs keep their
  size and move with the rest; arrows pointing at the items follow. Hold
  shift to keep the selection's shape. The pointer on a handle turns into
  a diagonal resize cursor. One undo puts it back.
- **App:** the "<agent> is done" bar goes on its own: 5 s after it shows,
  or at the pen's next press. A blocked agent's bar stays until you tap its
  ✕ (or ⌘.). The bar sits above the footers, so it hides no button, and a
  tap on it shows the agent's workspace.
- **App:** the sidebar says "not running" for an agent whose session is
  stopped, and "working" as soon as a sling is waiting for it, so a sling
  to a stopped agent no longer looks like nothing happened.
- **App:** the pen's ring over a canvas is as wide as the line it will
  draw: it grows with a wider pen and with the pane's zoom, and never gets
  smaller than it was.
- **App:** the footer's glyph buttons (undo, redo, the page arrows, zoom,
  split, close, the swatch, the width) say what they do: hover over one, or
  hold the pen on it, and a tip above the footer names its command and key.
- **App:** the colour, shape, tool and width pop-ups' cells are 28 pt
  high, the same as every other button the pen taps.
- **App:** the page menu scrolls. With more pages than fit, scroll or drag
  the pen on it to reach the rest; a drag picks no page.

### Removed

- **CLI and daemon:** the `hello` command and the `hello` verb, a
  placeholder from the first build. To check that the daemon answers, run
  `slinger canvas list`. `--verbose` now names the command and the socket.

### Fixed

- **App:** after a sling, the agent's answer no longer flashes "answered
  in text" when its items are still on their way to the canvas: an answer
  with nothing on the canvas yet waits a second for them.
- **App:** the sidebar shows what an agent's hooks say for an agent you
  run yourself, outside slingerd, instead of "not running". An agent
  slingerd started and that has stopped still says "not running".
- **App:** while a sling is out, only Delete stands beside the selection:
  ➤ Sling and + prompt come back once the agent answers.
- **App:** the sling's pill has round corners, like the flash, and is as
  wide as its words: a pill with symbols or wide letters no longer cuts
  them off or leaves a gap.
- **App:** ⌘Z and ⇧⌘Z undo and redo your typing in the sling's prompt
  (and a note of feedback or a page's new name) instead of doing nothing.
- **App:** the pointer turns into the resize cursor within reach of the
  divider between two panes, and keeps it while you drag the divider, so
  you can see where a press resizes the panes.
- **App:** an arrow dragged to another page with the items at its ends
  stays joined to them there: move a box on the new page and the arrow
  follows, as it did on the old one.
- **Sessions:** an agent whose last conversation Claude Code no longer has
  (`--resume` fails) is no longer stuck. Its process shows as `exited`,
  and the next `slinger session start` begins a new conversation.
- **CLI:** when an agent moves its item with `slinger canvas edit --move`,
  its arrows bound to the item follow, as they do when you move it in the
  app; they no longer point at where the item was.

- **App:** a colour, shape, tool or width pop-up opened from a key before
  the pen or pointer has been in the window opens at the middle of the
  focused pane, wherever the window was sized, not at a fixed spot.
- **App:** the select tool can pick a box drawn inside another box. A box
  dragged inside a box takes the ink it touches, not the outer box; a tap,
  or a drag over nothing, inside nested boxes selects the innermost one.
  A drag across a box's outline still takes it.
- **App:** menu keys work while the sling's prompt (or a note of feedback)
  is open: ⌘Q quits, ⌘K opens the palette and so on, instead of doing
  nothing. Keys a text field uses itself (⌘X, ⌘C, ⌘V, ⌘A, the arrows
  and delete with ⌘ or ⌃) still edit the prompt.
- **App:** the key overlay (⌘/) fits a narrow window. It drops to two
  columns, then one, instead of squeezing three until the titles and keys
  overlap; the rows scroll as before.
- **CLI:** `canvas apply`, `canvas undo` and `canvas redo` act as the agent
  named by `SLINGER_AGENT` when there is no `--agent`, as `canvas put` does.
  What an agent applies no longer lands in your undo history as yours.
- **Feedback:** notes left in the same second can be resolved one at a
  time. Each note's `ts` is now to the millisecond and unique in the file;
  notes already saved to the second still list and resolve as before.
- **Agents:** a drawing reaches the agent as one image when its strokes
  touch, instead of one image per long stroke (an airplane arrived as
  fuselage, wings and belly). An underline joins the words it touches. A
  frame or arrow that touches nothing still stays apart.
- **Agents:** a handwritten line with wide gaps between its words reaches
  the agent as one image, instead of one image per few words.
- **App:** ink is smoother. The pen's small wobbles no longer show as bumps
  along a line's edges, most of all at the bold and marker widths; a line
  still starts and ends where the pen did, and fine writing keeps its shape.
- **App:** the eraser, the lasso and the select tool take a stroke, a text,
  a box or an arrow as soon as the daemon has written it, instead of only
  once the canvas's feed has echoed it back. A stroke no longer draws twice
  for that moment either.
- **Daemon:** a second `slingerd` started on another socket but the same
  data directory now stops with exit 4 and `✗ data directory <path> is in
  use by another slingerd`, instead of writing to the same record as the
  first.
- **Daemon:** renaming your computer no longer splits its record in two.
  The record keeps the machine name it was created with and tags every
  new event with it; a record made by an older `slingerd` keeps the name
  on its first event.
- **App:** the sling's pill (`● scout working · 4s` and what follows it)
  and the ➤ Sling and + prompt buttons centre their words top to bottom,
  instead of sitting them at the top of their fill.
- **Agents:** the picture an agent sees of your canvas draws em and en
  dashes, curly quotes, an ellipsis, bullets, arrows, check marks, box
  drawing and blocks as themselves, instead of as `?`. A character the
  picture still has no glyph for (most emoji, Chinese) is drawn as `?`.
- **Feedback:** each note names the commit `slingerd` was built from
  (with `-dirty` for uncommitted changes) instead of always `dev`.
- **App:** choosing a menu bar item while the command palette is open now
  closes the palette before the command runs, instead of leaving it over
  the window. Choosing Show commands only closes it.
- **App:** choosing a menu bar item while the key overlay or the page menu
  is open now closes it before the command runs, as the palette does.
  Choosing Show keys only closes the key overlay.
- **App:** the key overlay, the command palette and the page menu now draw
  over an open sling prompt (and the feedback and rename fields), dimming
  it with the rest of the window, instead of leaving the prompt on top.
- **App:** a tap on a command palette row runs it when you lift, as on
  the key overlay and the page menu, so a press that slides off runs
  nothing. The row lights while you hold it.
- **App:** a press on the canvas closes an open sling prompt or feedback
  field, as it already closed a page's new name. A tap on a button on the
  canvas still sends the sling prompt.
- **App:** return on a just-opened command palette no longer quits. The
  palette now opens on the command you last ran from it, then the tools,
  with Quit at the bottom. Typed, the best match comes first: a title that
  starts with what you typed, then a word that does, then looser matches,
  then matches on the group name only ("sp" puts Split right above Pick a
  shape).
- **App:** a failed tap on the "send to agent" button shows why in the
  pill, instead of opening an entry the pen cannot close.
- **Daemon:** an agent can answer when `slingerd` was started with a
  relative `--socket`. The agent runs in its own home, where that path
  named no socket, so slings arrived but no reply ever landed.

- **App:** dragging a selection between two panes works as it looks. The
  items stay under the pen the whole way, in whichever pane it is over,
  read through that pane's own pan and zoom, and land where you lift.
  Before, they vanished as the pen crossed into the other pane. Over a
  pane on another page they show there as you drag; the move is one undo
  step, and the items stay selected on their new page. Over a footer, the
  sidebar or an agent pane they go back home, and a lift there moves
  nothing.
- **App:** a quick second press on a text you just selected drags it with
  the select tool or the lasso, instead of opening it for editing. A
  double tap that lifts in place still edits the text.
- **App:** a press inside the selection near a pane divider drags the
  selection instead of the divider.
- **App:** a pen or mouse press on a pane focuses it, so with two panes
  open the zoom, pan, page, split and close commands act on the pane you
  last touched. The first stroke in the other pane still draws, and a
  press on its footer or on an agent UI pane focuses it too.

- **App:** the feedback, sling and rename entries take the keyboard each
  time they open, and get it back after you click elsewhere or switch apps,
  so what you type lands in them. Their caret now shows on the dark box.
  Opening feedback while the sling is open shows feedback's own entry.

- **App:** zoom works with a mouse: hold cmd (or ctrl) and turn the wheel,
  or scroll with two fingers, to zoom about the pointer (away from you
  zooms in). Every zoom, by key, pinch or scroll, flashes its level at the
  top of the pane ("Zoom 150%"), so a zoom on an empty page shows too.
  New `zoom-reset` (Canvas > Zoom to 100%) goes back to 100%. The zoom
  keys are now cmd+=, cmd+- and cmd+0, as in other Mac apps and as a
  tablet's zoom wheel sends them; growing and shrinking a pane moved to
  alt+= and alt+-. A zoom key with an agent UI pane focused says there is no canvas
  to zoom there.

- **App:** the eraser takes a text only where its letters are. Before, a
  text counted as about twice as wide as it looked, so erasing an arrow or
  a stroke just right of a text erased the text too. The lasso, the
  selection box and the dashed frame around a text being typed now fit the
  letters as well. This holds for texts on canvases made before.

- **App:** a pane's footer no longer draws its page and key hints on top of
  the tool name: each part sits at its own place (left, middle, right).

- **App:** paste (cmd+V or Edit > Paste) now works in a canvas text being
  typed, in a page's new name and in the command palette's search, so
  Flick dictation lands there too. Each line of the pasted text starts a
  line in a canvas text. A command key such as cmd+C no longer types its
  letter into a canvas text.

- **App:** page commands confirm themselves at the top of the pane: next and
  previous flash the page and where it is ("Page 2 · 2/3"), and new page,
  rename and remove say what they did ("New page: Page 3 · 3/3", "Renamed:
  plan", "Removed: ideas", "The last page stays"). Reload config flashes
  too, even when the files did not change.

- **Canvas:** text added with `canvas apply` and no size now gets a box that
  fits its words, so `canvas put --beside` it no longer lands on top of it.
  Text already on a canvas with no box gets one when the canvas is read. A
  `set` that changes the words and gives no `w` or `h` fits the box to the
  new words.

- **App:** the eraser now removes boxes, text and arrows it passes over, not
  only pen lines. A box goes when the eraser crosses its outline, so erasing
  the ink inside a box keeps the box. One eraser swipe is still one undo.

- **App:** after you switch to another app and back, the pointer over a
  canvas shows the tool's cursor again at once, and the pane's footer and
  the sidebar show the arrow.

### Added

- **App:** a select tool (cmd+7, "Select" in the Tools menu, the pick-tool
  pop-up and the command palette) does what the lasso does without drawing a
  loop. Press an item to select it, or drag from empty canvas to draw a box
  that selects everything it touches. Shift+press adds an item to the
  selection or takes it out; shift+drag adds what the box touches. Drag a
  selected item, or anywhere inside the selection's box, to move the
  selection. A box is picked by its outline: press inside an empty box, or
  draw a box inside it, to select it. Delete, sling and escape work as they
  do on a lasso's selection. The pointer is the normal arrow.

- **App:** the key overlay (⌘/) works with the pen: tap a row to run its
  command, tap ✕ or outside the panel to close it, and drag or scroll when
  the rows do not fit. The row under the pen lights up.

- **App:** tap a key hint in a pane's footer to open the key overlay or the
  command palette.

- **App:** a quieter look: the sidebar has its own fill and a hairline
  edge, panes are split by hairlines and the focus border is 1 pt. The
  sling, feedback and rename entries, the key overlay and the command
  palette have rounded panels, key chips and a filled selected row. Renaming
  a page now opens an entry like the sling's in its pane, not a line in the
  footer.

- **Feedback:** leave a note about slinger without leaving it. In the app,
  `leave-feedback` (⌘⇧F, or Help → Leave feedback…) opens an entry in the
  focused pane: return saves the note with the workspace, canvas, page,
  tool and selection count, and "Feedback saved" flashes. From a shell,
  `slinger feedback add`, `ls`, `resolve` and `path`. Notes go to
  `feedback.jsonl` in the checkout `slingerd` was built from, else in its
  data directory; `slingerd --feedback <PATH>` picks another file.

- **App:** a tablet pen's eraser end erases, whatever tool is chosen. The
  footer and the cursor show the eraser while it is in use, and the tool
  comes back when you turn the pen to its tip.

- **App:** drag with the right mouse button to pan the canvas under the
  pointer, with any tool.

- **App:** pen widths. `pick-width` (cmd+shift+w) opens a pop-up at the pen
  that shows each width as a line of that width in the pen's colour; pick
  one with the pen, a digit, or the arrows and return, as in the other
  pop-ups. `width-up` (alt+]) and `width-down` (alt+[) step to the next or
  previous width without the pop-up. New pen strokes take the width; boxes
  and arrows keep their one line width. The footer's swatch is a line as
  thick as the pen (up to a square), and a new width flashes (`Width 8
  ⇧⌘W`). The widths come from `[pen] widths` in the config file (one to
  nine, in points; default `"2 4 8 16"`: fine, medium, bold and marker);
  the first is the width at start.

- **App:** a menu bar. slinger, File, Edit, Tools, Canvas, Pane, Window and
  Help list every command with its key from `[keys]`, and Tools checks the
  pen's tool. A rebound key shows after the config reloads. Edit's cut, copy,
  paste and select all work in the sling prompt.

- **App:** the sling prompt is a labelled field. Above it, "Sling to" and
  the agent of the canvas's workspace ("Sling" when it has none); beneath
  it, the keys that send, cancel and dictate.

- **App:** each pane has a footer. On a canvas pane it shows the pen's
  colour, its tool and that tool's key, and `· typing` while a text is open
  or `· N selected` with a selection; in the middle, the page and where it
  is among the pages (`ideas · 2/3`); on the right, the keys that show the
  keys and the commands. An agent UI pane's footer shows the UI's name. A
  narrow pane leaves out the right part, then the middle. The page's name
  is no longer a line at the bottom of the canvas.

- **App:** a new tool says so. Pick a tool by its key or from a pop-up and
  its name and key (`Eraser ⌘2`) show at the top of the focused pane for a
  moment. Over a canvas the pointer shows the tool: a ring in the pen's
  colour, a wider ring for the eraser, cross hairs for the lasso, box and
  arrow, and an I-beam for text.

- **App:** the window resizes. Drag its edge or corner, or zoom it, and the
  sidebar and the panes follow. It opens at 960x600 and goes down to 480x320.

- **App:** two new commands, `show-keys` (`cmd+/`) and `show-commands`
  (`cmd+k`). Rebind them in `[keys]` as any other command.
- **App:** the key overlay. `show-keys` (`cmd+/`) dims the window and lists
  every command by group (tools, pop-ups, sling, canvas, pages, panes,
  window), each with its key from your config, or `no key`. Escape, the
  same key again, or a tap outside the panel closes it.
- **App:** the command palette. `show-commands` (`cmd+k`) dims the window
  and opens a search over every command: type part of a command's name or
  group (`split` finds the splits), move with up and down, and press return
  or tap a row to run it, as its key would. Each row shows the command's
  group and its key. Escape, the same key again, or a tap outside closes it.

- **App:** the sidebar has a `WORKSPACES` heading and a row per workspace:
  its name, bold on a lighter row for the workspace the window shows; under
  it the agent's mark with `<agent> · <status>` (or `no agent`); and, when
  the agent is blocked, its note. The marks differ in shape as well as
  colour: round for working, round and green for done, square and red for
  blocked. The button's mark matches.
- **App:** the focus border shows only when there are two panes or more.
- **App:** a canvas pane's view starts with a margin, so an item at the
  canvas's origin no longer touches the pane's edge.
- **App:** a selection shows as a tinted box with a handle at each corner
  and its number of items above it. A text being typed shows a caret after
  its last character, a dashed frame around it and, under it, how to end it.

- **App:** the "send to agent" button. The `place-button` command
  (`cmd+shift+b`) puts one in the middle of the focused canvas pane. Tap it
  to sling what is new on the page since the last sling to the agent of the
  canvas's workspace, with no prompt. To add a prompt, press `sling` with
  nothing selected: the prompt opens beside the button; type, then tap the
  button or press return. Its mark shows the agent's status, as the sidebar
  does: working, blocked or done. Select, move, delete and undo it like any
  item. `slinger canvas apply <C> '{"op":"add","item":{"page":"p1","kind":"button","x":0,"y":0,"w":1600,"h":400}}'`
  places one from a terminal. A packet lists it as a `button` line; older
  versions of slinger skip it when they read the canvas.

- **Agent UIs:** live items. `slinger ui open <UI> --as item --canvas <C>
  --beside <ITEM> [--size W,H] [--data JSON]` puts an agent's UI on a canvas,
  beside an item, instead of in a pane. It prints the showing's id and the
  item's id. In the app the UI moves and scales with the canvas as you pan
  and zoom, and it is cut off at its pane's edge. Click into it to use it;
  lasso it to select it, then drag it or delete it like any item, and undo
  either. Until its page has loaded, the item shows its box and the UI's
  name. A packet lists it as a `live` line, never an image. Older versions
  of slinger skip a live item when they read the canvas.
- **App:** `slinger-app --preview <UI> --as item` shows a UI as a live item
  would show it.
- **App:** sling from the window. With items selected on the focused
  canvas, the `sling` command (`cmd+return`) opens a one-line prompt at the
  selection. Return sends the selection and the prompt (an empty one too) to
  the agent of the canvas's workspace; escape cancels. The sling waits until
  the ink you drew before it has reached the daemon, so the agent sees what
  you saw. The answer arrives on the canvas, and the agent's status shows
  working, then done. If the sling fails (for example, the agent is not
  running), the prompt stays open with the error and what to do; return
  tries again.
- **App:** speak the sling's prompt. The `dictate` command
  (`cmd+shift+return`) opens the prompt at the selection and starts Flick's
  dictation; press it again to stop, and Flick types what you said into the
  prompt. Escape or return cancels a dictation still going. If Flick is not
  running, or its dictation is not set up, the prompt says so once and you
  can type instead. The prompt now takes cmd+X, C, V and A.
- **Agent UIs:** `slinger ui open <UI> --workspace <W> [--data JSON]` lets an
  agent show a UI in a new pane, with data, and get the user's answer back.
  It prints the showing's id (`pick-slot#12`) and the pane. Each
  `slinger.reply()` from that pane is recorded and reaches the agent's
  running session as one `ui_reply` JSON line, in order. If the agent is not
  running, the reply stays in the record and the pane says so. Data and each
  reply are at most 16 KiB, and a showing sends at most 100 replies.
- **App:** an agent UI pane shows the UI. `slinger pane open <workspace>
  --type ui --target <name>` shows the latest version of that UI in a
  sandbox: no network, no files, no way to navigate away or open a window.
  The UI reads what it was given with `slinger.context()`; a UI opened by
  name has no agent, so `slinger.reply()` answers `unavailable` there. A UI that cannot show
  says its name and why, and the rest of the window keeps working.
  `slinger-app --preview <UI> [--data JSON]` shows one UI alone, with that
  data, and prints each reply as a `ui_reply` JSON line.

- **Canvas:** text, boxes and arrows. With the text tool, tap to place a
  text and type it; `escape` or a tap outside ends it. Tap a text with the
  text tool, or double tap it with any tool but the pen, to edit it; empty it
  to remove it. With the box tool, drag from corner to corner; with the arrow
  tool, drag from start to end. An arrow end that lands on an item stays on
  it: move the item and the arrow follows. Each text, box, arrow and edit is
  one step. Pick a tool from the tool and shape pop-ups or with the new
  `tool-pen`, `tool-eraser`, `tool-lasso`, `tool-text`, `tool-box` and
  `tool-arrow` keys (`cmd+1` to `cmd+6`).
- **Canvas:** pages, undo and redo in the app. Each pane shows its own page,
  named quietly at its foot. `cmd+]` and `cmd+[` turn the focused pane to
  the next or previous page, `cmd+n` adds a page, `cmd+shift+r` renames
  the page (type the name, then `return`), and `cmd+shift+delete` removes
  it; a page with items asks first, and the last page stays. `cmd+z` undoes
  your last step on the canvas and `cmd+shift+z` redoes it. Undo is yours
  alone: it never takes back what an agent put. New keys: `previous-page`,
  `new-page`, `rename-page`, `remove-page`, `undo`, `redo`.
- **App:** resize panes and move things between them with the pen. Drag
  the divider between two panes (it is easy to catch: 8 points either
  side) to resize them; the workspace keeps the new sizes, and so do the
  `grow-pane` and `shrink-pane` keys now. Drag a selection into a pane on
  another canvas and its items move there, where the pen let go: one undo
  step on each canvas. Let go outside any canvas pane and nothing moves.
  `slinger pane resize <workspace> <pane> <ratio>` moves the divider after
  a pane from a terminal.
- **Canvas:** erase, select and move. With the eraser, every stroke the pen
  passes over goes at once; the whole drag is one step. With the lasso, draw
  a loop: the items wholly inside it are selected, with a box around them.
  Drag inside the box to move them, press `delete` to remove them, or
  `escape` to select nothing (the `delete-selection` and `clear-selection`
  keys). Each drag, and each delete, is one step. Anything on the canvas can
  be erased or moved, an agent's items too.
- **Canvas:** draw with the pen. With the pen tool, a stroke on a canvas pane
  shows under the pen at once, at a constant width in the pen's colour, and
  is saved to the canvas when the pen lifts (one stroke, one undo step). If
  the daemon cannot be reached, your strokes stay on screen and are saved, in
  the order you drew them, once it is back. A palm or finger on a tablet
  display does not draw. The app writes each stroke's pen-to-screen delay to
  stderr as an `ink:` line.
- **App:** a sidebar lists the open workspaces and each agent's status with
  one mark: working, blocked or done. `toggle-sidebar` (cmd+b) opens or
  collapses it, and `[window] sidebar = "open"` or `"closed"` sets how the app
  starts. When an agent becomes blocked or done, one small notice at the
  bottom of the window names it, with its note; nothing else interrupts.
  `dismiss-notice` (cmd+.) hides it, and it goes on its own once the agent's
  status moves on.
- **Panes:** the server keeps each workspace's panes, so you, the app and
  agents share them. `slinger pane list <workspace>` numbers them,
  `slinger pane open <workspace> --type canvas|ui --target <id> [--beside
  <pane>] [--below]` opens one, and `slinger pane close <workspace> <pane>`
  closes one (one already gone is fine). The app shows the panes of
  `[workspace] name` in its config, else the first open workspace; its
  splits and closes go to the server, and a pane opened from a terminal shows
  in the window within a second. The layout survives a restart. Each change
  is recorded as `pane_moved`. Growing and shrinking a pane stays in the
  window for now.

- **App:** a canvas pane draws its canvas from the server and follows it:
  pen lines, text, boxes and arrows in their colours, and a change made
  anywhere (`slinger canvas put`, another client) shows within about half a
  second. Scroll or alt+arrows pan, pinch or alt+= and alt+- zoom, and
  `next-page` (cmd+]) steps through the pages. Ink too dark to see on the
  theme's background draws in the theme's text colour.
- **App:** pop-ups at the pen. `pick-colour` (cmd+shift+c), `pick-shape`
  (cmd+shift+s) and `pick-tool` (cmd+shift+t) open a small pop-up where the
  pen or pointer last was, kept inside the window. Pick with the pen, with a
  digit, or with the arrow keys and return; escape or the same command again
  closes it. The colours come from `[pen] colours` in the config file (one to
  nine, `"#rrggbb"`, split by spaces or commas). The canvas that uses the
  chosen tool and colour comes later.
- **Sessions:** an agent reports without being asked. Each Claude Code
  process slingerd starts gets hooks of its own (your Claude Code settings
  are untouched): its status is `working` while a turn runs, `done` when it
  ends and `blocked` while it waits for your permission or input, and every
  file it writes is recorded by path and hash. `slingerd --slinger <path>`
  names the CLI the hooks run; its folder also comes first on the agent's
  `PATH`, so the agent's own `slinger` commands run the same CLI.
- **CLI:** `slinger file touched <path>` records that the calling agent wrote
  a file: its path and hash, never its contents. `--hook` reads a Claude Code
  hook's payload on stdin instead.

- **Daemon:** a session runs within stated limits. By default the agent
  reads only its workspace's roots, runs only the `slinger` CLI and read-only
  commands, and does not load your own Claude Code settings and
  instructions; anything else it would ask for is denied. The workspace's
  `[agent]` widens it with `dirs`, `tools` (Claude Code permission rules,
  such as `"Edit"` or `"Bash(git *)"`) and `user_config = true`.
- **CLI:** sling. `slinger sling send <canvas> --select <ids> --prompt
  <text|->` sends the ink you drew since the canvas's last sling, your
  selection and your prompt to the agent of the canvas's workspace, and
  returns as soon as it is delivered; the answer lands on the canvas. Every
  sling goes into the same conversation, and selecting old ink again sends
  no ink twice. Each sling is in the record with its packet. If the agent is
  not running, it exits 6 and tells you to `session start` it: slinger never
  starts an agent for you.
- **App:** the window holds panes. Split the focused pane right
  (`cmd+d`) or down (`cmd+shift+d`), close it (`cmd+w`; its space goes to
  the pane beside it), move focus to the next pane (`ctrl+tab`) or the one
  beside it (`cmd+alt+arrow`), and grow or shrink it (`cmd+=`, `cmd+-`).
  One border, in the theme's new `focus` colour, marks the focused pane;
  there is no other chrome: the daemon's greeting is gone, and a problem
  with the daemon or the config file shows in the focused pane. Each pane
  shows its kind and name for now. The keys are in `[keys]`, and
  `config.example.toml` lists them.

- **CLI:** agents answer on the canvas. `slinger canvas put <canvas> --beside
  <id> --text <text>` puts text beside an item, where it covers nothing:
  to the right first, else below, left or above. `--box w,h` puts a box and
  `--arrow <from> <to>` an arrow between two items instead. The item carries
  the agent's name (`--agent` or `SLINGER_AGENT`) and the time, and the
  command prints its id and box. After a sling to that agent, the put also
  records that the reply landed.
- **CLI:** agents change or take back what they put. `slinger canvas edit
  <canvas> --item <id>` gives a text new words (`--text`), a text or a box a
  new size (`--box w,h`) or a new place (`--move x,y`), and `canvas remove
  <canvas> --item <id>` removes it; removing it again succeeds. An agent can
  change only items it made, never your ink.
- **CLI:** run a workspace's agent. `slinger session start <workspace>`
  starts Claude Code in the agent's home and keeps it running; `session send
  <workspace> <text|->` delivers one message to it, and every message goes to
  the same conversation. `session show` has the process's state, its session
  id and its last answer. After slingerd restarts, `session start` resumes
  the same conversation. If the process dies, the session shows `exited` and
  a send exits 6 until you start it again; `session stop` ends it, and so
  does stopping slingerd. The agent's own `slinger` calls find it through
  `SLINGER_AGENT` and the new `SLINGER_SOCKET`, which `--socket` overrides.
  `slingerd --claude <path>` runs another Claude Code program.

- **Sling:** what an agent receives for one sling: your prompt, the page's
  packet with each image under its line, the items you selected, and a short
  note on how to answer on the canvas instead of in chat. The first sling of
  a session gets the whole note, later ones a single line. An empty prompt
  with a selection is a valid sling.
- **CLI:** sessions and agent status. `slinger session list` shows each agent
  of an open workspace with its status, home directory and note, and
  `session show <agent>` prints one. An agent reports what it is doing with
  `slinger status set <working|blocked|done> [--note <text>]`; `blocked` and
  `done` also go into the record, tagged with the agent and its workspace, and
  survive a restart. The agent's name comes from `--agent` or the
  `SLINGER_AGENT` environment variable; with neither, the command exits 2.
- **CLI:** workspaces. `slinger workspace open <dir>` opens a directory as a
  workspace: it writes `slinger-workspace.toml` in the directory if there is
  none, makes the workspace's canvas, and prints its name; opening it again
  returns the same workspace. `workspace open --name <name>` opens one with no
  directory, kept in slingerd's data directory. `workspace list`,
  `workspace show <name|dir>` and `workspace close <name|dir>` list, print and
  close them; closing keeps the file and the canvas. Open workspaces survive a
  restart, and an event on a workspace's canvas or from its agent is tagged
  with the workspace.
- **CLI:** `slinger canvas read <id>` prints a page's packet: what an agent
  reads instead of a screenshot. New ink since the last sling is images, the
  rest of the page an outline. `--after <n>` reads from another point,
  `--page` picks the page, `--select c42,t12` narrows the outline and
  `--out <dir>` says where the images go. Every packet and image is stored
  in the record, so `blob get` fetches them later.
- **CLI:** `slinger canvas crop <id> --item <item>` (or `--box x,y,w,h`
  with `--page`) writes an image of one item, one ink cluster or one box at
  the packet's scale, in tiles when it is large, and prints each file's
  path. `slinger canvas strokes <id> --item <item>` prints the raw points of
  a stroke or of every stroke of a cluster.
- **CLI:** agent UIs are stored in the record, named and versioned.
  `slinger ui put <name> <file|->` stores an HTML file as the UI's next
  version and prints `<name>@<version>`; the same bytes again add no version.
  `slinger ui list` shows each UI's latest version, `slinger ui show <name>`
  every version with its hash, size, author and time, and
  `slinger ui export <name>[@<version>] [--out <dir>]` writes one version
  back as `<name>.v<version>.html`.
- **CLI:** `slinger canvas import <file.tldraw> [--name <id>]` makes a new
  canvas from a tldraw board, with its pages, drawings, text, rectangles and
  arrows, and prints its id and what it holds. Shapes slinger has no item for
  (groups, lines, ellipses) are counted as skipped.
- **CLI:** canvases. `slinger canvas create <id>` makes one, `canvas list`
  lists them, `canvas show <id> [--page]` prints a page's items, `canvas apply
  <id> <ops|->` adds, moves, edits and removes items and pages, and `canvas
  undo` / `canvas redo` step back and forth. `--agent <name>` acts as an agent,
  which may change only what it made. slingerd keeps each canvas in a file in
  its data directory and every change is on disk before the command returns.
- **CLI:** `slinger canvas changes <id> --after <number>` prints the canvas's
  operations after that number, one JSON line each. `--wait <ms>` waits for
  the next change when there is none yet, so a client can follow a canvas
  without polling it.
- **CLI:** `slinger blob put <file|->` stores a file's bytes (or stdin's) in
  this machine's record and prints the blob id. `slinger blob get <id>`
  writes the bytes to stdout, or to a file with `--out`. Storing the same
  bytes twice gives the same id; an unknown id exits with `not_found`.
- **CLI:** `slinger record append <kind> [payload]` adds an event to this
  machine's record and prints its number. `slinger record list` prints the
  events, filtered by `--kind`, `--machine`, `--workspace`, `--agent`,
  `--canvas` and `--after <number>`. A payload that does not fit its kind
  is refused with a hint.
- **App:** a plain-text config file, `~/.config/slinger/config.toml` (or
  `$SLINGER_CONFIG`), sets the theme's colours and the daemon's socket. A
  `config.local.toml` next to it holds one machine's values. Edits show in
  the running app within a second; a file with an error keeps the last good
  config and the window shows the file, the line and what to fix.
  `config.example.toml` lists every key.
- **App:** every action is a key command, bound to a key chord in the
  config's `[keys]` table (`quit = "cmd+q"`). `quit` (⌘Q) and
  `reload-config` (⌘R, read the config files now) are the first commands;
  `""` leaves a command without a key. An unknown command, a chord that does
  not parse, or two commands on one chord is a config error with its line. A
  tablet button works by sending a key chord from the tablet's own software.
  The app no longer has a menu bar: quit with the `quit` key or by closing
  the window.

- **Record:** events (sling sent, reply landed, file touched, pane moved,
  agent blocked, agent done) and blob ids have a versioned JSON format. A
  reader keeps events of kinds and fields it does not know, so records from
  newer versions stay readable.
- **App:** the groundwork for the canvas pane: an endless canvas you can pan and zoom (within limits) about any point, drawing only what is on screen and repainting only what changed. Not yet shown in the window.
- **Workspaces:** a workspace is defined by a `slinger-workspace.toml` file in its primary root: a name, its directories, an optional agent, its record tag and its default canvas. See `docs/workspace.md`.
- **Canvas:** a canvas file format: a versioned, append-only log of every change.
  A crash in the middle of a save loses only that unsaved change, and canvases
  written by newer versions stay readable.
- **Canvas:** the canvas model: pages of strokes, text, boxes and arrows, each with its author and time. An agent can change only the items it made, never your ink, and removing something already gone is not an error.
- **Canvas:** tldraw boards can be read into a canvas: pages, pen strokes,
  text, rectangles (with their labels) and arrows (still pointing at what
  they were attached to), in their colours and drawing order. Shapes with no
  Slinger counterpart are skipped and counted. There is no export to tldraw.
- **Daemon:** `slingerd` keeps this machine's record (events and blobs) in
  one SQLite file in its data directory. The directory defaults to
  `$XDG_DATA_HOME/slinger` (on macOS, `~/Library/Application Support/slinger`)
  and `--data-dir` sets it.
- **Canvas:** the canvas knows what is new, changed or removed since the last sling, so a sling sends only that. Something added and then erased since is not new, and nothing new is a valid answer.
- **Canvas:** geometry queries on a page: the item under a point, the items inside a lasso (even one that crosses itself), the strokes an eraser touches, the items a box touches, and the box around a selection. Fast enough for pen input on a page of 7500 strokes.
- **Packet:** the text an agent will read from a canvas page instead of a screenshot: one line per item with its id, box, kind and what it points at, the new ink first, then what agents made, then an outline of the rest. What an agent made is always text, never an image.
- **Packet:** images of your ink at a fixed scale, so handwriting reads the same however the window is zoomed. An image too big to send in one piece is cut into tiles that overlap, so no word is lost at a tile's edge.
- **Packet:** a whole packet for a page: what is new since the last sling (your ink with its images, and what agents made), then an outline of the rest, or only of what you selected. It carries a format version, and nothing new is still a valid packet. The whole fixture board costs under half of what its screenshots did.
- **Packet:** your new ink is grouped into notes by distance, colour and time, so two notes of different colours side by side stay apart, and a frame, arrow or divider you drew never glues notes together.
- **Canvas:** undo and redo. Your undo takes back only your own changes, never an agent's, and an agent's undo never touches yours. A lasso move or one erase gesture is one undo step.

### Fixed

- **App:** a canvas pane no longer leaves a stale copy of its items over the
  sidebar or the pane beside it. Text, boxes or buttons that a pan or a
  sidebar toggle put part past the pane's edge drew past it and stayed
  there; now each pane draws only inside its own frame.
- **Canvas:** the first box, arrow, stroke, erase or lasso after a text you
  were typing is no longer lost. A press outside the text with any tool but
  the text tool ends the text and starts that tool's gesture at once; with
  the text tool, a tap outside still only ends the text.
