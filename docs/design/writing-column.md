# The writing column: a canvas text you can live in

Status: picked overnight 2026-11-24 (slinger-820fe6, plan pl-b237f5, parent
slinger-f86716), owner to confirm the picks marked so. Built the same night:
"As built" and "Acceptance" at the end say what shipped and what is left. Inputs: D6, D18, B6,
the canvas file (`docs/design/canvas-file.md`), the packet
(`docs/design/packet.md`), pen-less input (`docs/design/pen-less.md`, pick
2A), folding (`docs/design/fold.md`, "Step in"), and the spike
slinger-dd9b7b (branch `spike/nvim-embed`, commit 892628c: a reference to
port from, never merged).

The later steps of the plan build on "Recommended" at the end. A step that
cannot do what it says reports that; it does not pick another way.

## The aim

The owner's own writing, on the canvas, where the ink, notes and slings
already are: several paragraphs of markdown, written and revised with the
owner's own nvim, without leaving slinger. Not a markdown editor that
competes with nvim, and not a place to read prose an agent wrote.

## Settled by the owner (2026-11-24, not reopened here)

- **Every canvas text is edited in nvim when nvim is there.** One way to
  type. A new text starts in insert mode; ctrl+return (`new-text`) still
  ends it and starts the next one below. With no nvim, or with it turned
  off, the scene editor (pen-less 2A) types as today.
- **The gutter is off**: no numbers, no sign column, no fold column. The
  grid is the text's 80 cells and nothing else.
- **A text is a column 80 cells wide that grows to 80 lines**; past that it
  shows a fade and "N more lines", and the rest is read by stepping in.
- **The agent rule is absolute**: an agent never changes the characters of
  a text the user wrote. It proposes beside the text; the owner makes the
  change. There is no per-sling grant.
- Upgrade the text item; add no new item kind (plan alternative 3).

## Where things stood (main, 2026-11-24, before the plan)

- A text item is `Shape::Text { text, frame, colour, size }`
  (`crates/core/src/canvas/item.rs`). A character's cell is `CELL` = 0.5 of
  the size wide and one size tall, in the core (`crates/core/src/render/mod.rs`)
  and the app (`crates/app/src/scene/glyphs.rs` `CELL`) since slinger-ce52ee
  (6a84371). The core cuts a text into rows greedily at spaces
  (`render/raster.rs` `wrap`); the app's `glyphs::layout` cuts the same rows
  and draws them in the system font at `FONT` = 0.83 of the size.
- A frame as wide as the longest line is a text that does not wrap; any
  other width is a column. A new typed text is 48 cells
  (`glyphs::NEW_COLUMN`), stored as 48.5 cells so it never reads as
  unwrapped (`glyphs::column`).
- The pure scene editor (`scene/editor.rs`, `scene/editor/{emacs,undo}.rs`)
  types every text. `canvases/editing.rs` owns it (`Editing`: ink, select,
  shapes, crop, outbox, kept, pen, unshown, handle).
- **The agent rule already holds for edits.** `Author::may_change`
  (`item.rs`) lets an agent change, move or remove only items it made, and
  every path goes through `Canvas::may_change` (`canvas/mod.rs`): a
  request's `set`, `canvas edit`, fold (`canvas/fold.rs`), restore. So
  "an agent cannot change a text the user wrote" is true today for texts
  the user made; the gaps are below (Question 5).
- The packet quotes a text whole, newlines as `\n` (`packet/line.rs`).
- Step in is a pane's view state for boxes (`canvases/step.rs`, `Trail`).

## What the spike showed (slinger-dd9b7b)

- The owner's real nvim (0.12, own config: render-markdown, spell,
  which-key) edited a canvas text in a driven window: key to paint 6.5 ms
  median, about 1,800 lines, no new dependency (a 267-line msgpack in
  `nvim/pack.rs`). nvim is 0.11 on this build machine; `nvim_win_text_height`
  needs 0.10, so 0.10 is the floor.
- Menlo's advance at `FONT` is 0.83 × 0.602 ≈ 0.50 of the size: the grid's
  cell is the core's `CELL`. One measure, no second one.
- It wrote into the owner's nvim directories (an `lsp.log` line, a `luac`
  cache file): `-n -i NONE` and a scratch buffer are not enough.
- Requests sent right after `nvim_ui_attach` run before the user's config,
  and a buffer attach sent too early hears nothing: every leave committed
  an empty text, with no error. The spike's fix (attach after a
  `slinger_ready` notification sent from `VimEnter`) works and is kept.
- nvim takes 75–107 ms to be ready. Keys typed in that time were not tested.
- Escape in normal mode ends the text, so leaving from insert is Esc-Esc,
  and a normal-mode `<Esc>` mapping in the owner's config never fires.

## Question 1: the editor

### 1A. `nvim --embed` as a UI client, its grid painted in the text's box (owner's pick)

The app starts `nvim --embed`, attaches with `ext_linegrid` and `rgb`,
seeds a scratch markdown buffer with the text, and paints the cell grid
nvim sends in the text's box, in Menlo at the text's size, one cell per
`CELL`. nvim owns wrap, cursor, modes, the `:` line, floats, spell and the
owner's plugins. The buffer's lines are mirrored from `nvim_buf_attach`
line events, so the text can end from anywhere with what the mirror holds.

- For: the whole of vim, the owner's config and plugins, at no cost per
  motion. Floats and virtual text show, because the grid is what nvim drew.
- For: no new dependency; the protocol half is pure and testable.
- Against: a child process per edit, two looks for one text (the grid while
  editing, the rendered view after: Question 4), and a cold start.
- Against: no macOS input methods (marked text), as with 2A today; dead
  keys work. Dictation is a paste (`nvim_paste`).

### 1B. nvim as a hidden buffer engine; slinger draws the text

nvim runs headless; slinger sends keys, reads the buffer and the cursor,
and draws them in its own proportional layout.

- For: one look for a text, edited or not.
- Against (plan alternative 2): slinger's wrap and nvim's line model
  disagree (`gj`/`gk`, visual-line motions), and floats, virtual text,
  completion menus and the `:` line do not show.

**Amends** pen-less 2A: the scene editor stays, as the fallback when nvim is
missing or off and for tests; spell check now comes from nvim; input methods
are still absent. **Amends** SPEC D6 (a text is edited in nvim; still one
text item) and D18 (prose typed in nvim is keyboard input, not terminal or
code entry: no shell, no terminal buffer, no LSP is set up by slinger).

## Question 2: the stored text and hard breaks

The owner's nvim has `textwidth=80`, so typing inserts real line breaks at
80. An agent's text has none.

### 2A. Keep the hard breaks; show the rows as typed

The stored text is exactly the buffer's lines joined by `\n`. Nothing joins
them: the rendered view (Question 4) draws each stored line as a row (a
line longer than the column wraps at spaces, the core's rule), the packet
quotes the text as stored, and the crop draws the core's rows. While nvim
edits, the session sets `wrap`, `linebreak` and `breakat=" "` on its window,
so a line longer than 80 cells (an agent's paragraph, a URL) wraps where the
core wraps it.

- For: the rows in nvim, in the view, in the crop and in the packet's text
  are the same rows. Entering a text does not move a word.
- For: nothing of the owner's config is overridden; `gq` and `formatoptions`
  do what the owner expects.
- For: the stored text is what the owner typed: reversible later by any
  rule that joins.
- Against: a text narrowed or widened later does not reflow its paragraphs
  (its hard breaks stay). Every text is 80 cells (Question 3), so this only
  bites a text resized on purpose.
- Against: an agent reads `\n` inside paragraphs. Markdown treats a single
  newline as a space, and so do models.

### 2B. Soft wrap only: one line a paragraph

The session sets `textwidth=0` for the buffer after the config loads; the
stored text has one line a paragraph and wraps in the column.

- For: a paragraph reflows when the column changes; the packet has no
  breaks inside paragraphs.
- Against: overrides the owner's `textwidth` (and plugins that set it per
  filetype run after us unless we fight them on every `FileType`).
- Against: nvim's soft wrap and the core's wrap must agree word for word on
  every paragraph, at every width; with hard breaks at 80 they agree by
  construction.

## Question 3: the column and the cap

### 3A. The cap is a core rule: a text's frame is at most 80 rows tall

The core's frame rule (canvas-file.md, the `text` row) gains one clause: a
text's height is its rows at its column, at most `MOST_ROWS` = 80 rows. The
whole text stays in the item and in the packet. Hit tests, `put --beside`
placement, the packet's box, the crop and the app all use the capped frame,
so nothing is placed under text that does not show.

- For: one rule, one place (`render::text_size` and `glyphs::layout` read
  the same constant); an agent's put never lands on hidden text.
- Against: a file field's meaning changes for texts over 80 rows (agents'
  long texts today shrink on screen). The bytes do not change.

### 3B. The cap is the app's drawing only

The frame stays the full height; the app clips to 80 rows and fades.

- For: no core change.
- Against: the canvas has an invisible tall item: a put beside lands far
  below, a lasso catches empty space, and the crop disagrees with the app.

Shared by both: the column is 80 cells (`COLUMNS` = 80), stored as 80.5
cells at the text's size (`glyphs::column`, so it never reads as unwrapped);
a new text is made that wide (in place of `NEW_COLUMN` = 48). A text edited
in nvim whose characters changed is written with an 80-cell column; one
edited with no change keeps its frame. A row is one of the core's rows, so
"80 lines" counts rows on screen, not newlines.

What a capped text shows: rows 1 to 79, the last four fading to the paper,
and in row 80, dimmed, "N more lines" (N = rows past 79). Step in on it
(`step-in`, or two quick taps on that row with any tool but the pen and the
eraser) shows the text alone in the pane, all its rows, from its top; escape
or `step-out` goes back, as for a box. While nvim edits a capped text its
grid is 80 rows and nvim scrolls (the spike's `MOST_ROWS`).

## Question 4: the rendered view

### 4A. Markdown drawn on the same 80-cell grid

A text not being edited is drawn row by row in Menlo cells, exactly where
nvim drew them, with a small markdown subset styled in place: `#` headings
bold (the `#` marks dimmed), `*em*` italic and `**strong**` bold (marks
dimmed), `- `/`* `/`1. ` list marks dimmed, `> ` quotes with a bar in the
left cell, `code spans` on a tint, `- [ ]` / `- [x]` checkboxes as ☐ / ☑ in
the same cells. No mark is removed and none changes its cell count, so a
word is in the same cell edited or not. A fenced code block is drawn plain
on a tint. Everything else is drawn as typed.

- For: no jump on entering a text; one measure (the core's rows, the
  core's `CELL`); the grid painter (`platform/cells.rs`) draws both.
- For: render-markdown.nvim shows the owner the same look while editing.
- Against: it is a monospace page, not a typeset one; marks stay visible.

### 4B. Typeset markdown in the system font

Paragraphs joined and reflowed in the proportional font, headings larger,
marks hidden.

- For: reads like a document.
- Against: a second measure (the ce52ee problem again), every text jumps
  when entered, hidden marks shift every press-to-cell mapping, and the
  crop the agent sees can no longer match.

Shared by both: a press on a word of a text not being edited, with the
select tool or a mouse press with the pen tool (pen-less 1B), opens it in
nvim in normal mode with the cursor on that cell; the text tool's tap on a
text opens it at its end in insert mode, as today. **The agent picture**
(the crop) draws the core's rows, plain, as today: the agent reads the
markdown source in the packet.

## Question 5: the agent rule

What an agent may do to a user's text, where a reply goes, how long it may be.

### 5A. The core holds it: authorship, placement and length

- **Change**: as today, `Author::may_change`; nothing new to refuse for a
  text the user made. One hole closes: when the user changes the
  characters of an agent's text, the text becomes the user's (the item's
  author is the user from that `set` on, read from the log, no new field),
  so the agent may no longer change what the user wrote into it.
- **Where**: a reply is a new text beside the one it answers (`canvas put
  --beside`, which already keeps clear of items and matches the user's
  size and colour). Nothing in the core stops an explicit `--at` over a
  column; the guide says not to.
- **How long**: an agent's text is at most 12 rows at its column; a put or
  edit past that fails with a stable code (`too_long`) and the hint "split
  it: one short put beside each subject". The user's texts have no limit
  but the cap.

- For: holds without trusting the guide; the error teaches.
- Against: a long code snippet from an agent must go in several texts, or
  a live item (an agent UI) which has its own scroll.

### 5B. The guide and the skills only

The core changes nothing; `guide.md`, the slinger skill and the listen skill
say: never edit a user's text, reply beside it, a few lines at most.

- For: no code; trivially reversed.
- Against: "absolute" rests on the model following prose; the user's edit
  of an agent's text stays the agent's to overwrite.

## Question 6: the process

### 6A. The app owns it, one nvim per edit (the spike)

`crates/app/src/nvim.rs` and `nvim/{pack,grid,keys}.rs` hold the pure
protocol (messages to write, what the ones read mean); `backend/nvim.rs` the
child process and its reader thread; `canvases/editing/vim.rs` the glue. An
nvim starts when a text opens and is killed by its own pid when it ends.

- For: a crash loses one text's session, never another's state; nothing is
  shared between texts that could leak (registers aside: the system
  clipboard carries yanks).
- For: the daemon stays free of UI; keys never cross the socket.
- Against: 75–107 ms cold start each time; keys typed then must be held.

### 6B. One long-lived nvim, a buffer per text

Started once with the app, a buffer and window per edited text.

- For: no start delay after the first; registers and undo history live on.
- Against: one crash or one hung plugin takes every edit with it; state
  (marks, jumps, plugin caches) piles up for the app's lifetime; switching
  buffers fires the owner's autocommands in ways a single scratch buffer
  never does.

(A third, the daemon owning nvim, is out: the daemon has no screen, and
every key and every flush would cross the socket.)

## Recommended (picked overnight, owner to confirm)

**1A, 2A, 3A, 4A, 5A, 6A.** Each is the one with one measure and one owner.
2A and 6A are the cheapest to reverse (no stored text is rewritten; a
spare nvim kept warm can be added to 6A later). 4A shares the grid painter
1A needs. 3A and 5A change core rules; each is a constant and a clause.

### The process (step 2, slinger-c6e898)

- **Which nvim**: config `[editor] nvim = "auto" | "off" | "<path>"` (new
  `crates/app/src/config/editor.rs`; config.rs is 496/500 lines, so its
  section struct goes in that file). `auto` looks on the app's `PATH`, then
  `/opt/homebrew/bin`, `/usr/local/bin`, `~/.nix-profile/bin`,
  `/etc/profiles/per-user/$USER/bin`, `/run/current-system/sw/bin` (a
  Finder-launched bundle has launchd's `PATH`). Not found, or older than
  0.10: the scene editor, and one flash per app run: "nvim not found:
  typing with slinger's editor ([editor] nvim)".
- **Nothing of the owner's is written**: `nvim --embed -n -i NONE` with
  `XDG_STATE_HOME` and `XDG_CACHE_HOME` set to `$TMPDIR/slinger-nvim/{state,cache}`
  (config and data stay the owner's, so plugins load), `cwd` that temp
  directory, a `nofile` buffer with no swap and no undo file. Tests run
  `--clean` with all four `XDG_*_HOME` in a temp dir and assert nothing
  under them but state and cache was written; a test that finds no nvim on
  `PATH` skips with a line on stderr, never fails.
- **Ready before anything**: `nvim_ui_attach` and the seeding Lua go first;
  the buffer attach only on `slinger_ready`. Keys, pastes and presses that
  come before ready are queued in `Session` and sent after it, in order.
- **No lost text**: the session's text is **confirmed** only after the
  first line event (`send_buffer` = true) gives the whole buffer. Every way
  of ending (escape, `:q`/`ZZ`/`:wq`, a press outside, ctrl+return, the app
  quitting, nvim exiting or crashing, nvim never becoming ready within 2 s)
  writes the confirmed text, or, with none confirmed, leaves the old text
  untouched and flashes why. An empty confirmed buffer on a text that had
  characters is written (the owner deleted them), and a new text left empty
  is dropped, as today.
- `SLINGER_NVIM`, `SLINGER_NVIM_CLIP` and `SLINGER_NVIM_TRACE` stay for
  tests and drives: `clean` forces `--clean`, `CLIP` replaces the system
  clipboard with a file, `TRACE` writes timings.

### The grid (step 3, slinger-be5d24)

- The grid is `COLUMNS` = 80 cells wide plus one row for the `:` line, and
  as tall as the text's rows (measured after each flush with
  `nvim_win_text_height`), 1 to 80. The session sets, after the config
  loads: `number`, `relativenumber` off, `signcolumn=no`,
  `foldcolumn=0`, `statuscolumn=""`, `laststatus=0`, `showtabline=0`,
  `ruler` off, `cmdheight=1`, `wrap`, `linebreak`, `breakat=" "`. A gutter
  a plugin draws anyway is measured (`textoff`) and the grid widened by it,
  so the text stays 80 cells (the spike's rule).
- Painted in the text's frame at its size: backgrounds, runs of one
  highlight in Menlo (bold and italic faces), underlines and undercurls,
  the cursor in its mode's shape. The `:` line and floats paint inside the
  grid's box; a float wider than the grid is clipped to it.
- The scene item is `Shape::Cells` (the spike's), drawn by
  `platform/cells.rs`; `drive scene` prints the grid's rows so a driven
  test reads what nvim shows.

### The keys (step 4, slinger-b8e3d3)

| Key or act | While nvim edits a text |
|---|---|
| any key without cmd | nvim's (`nvim_input`, in vim notation; ctrl chords too) |
| escape in normal mode (after nvim took every key sent and confirmed its buffer: a barrier, at most 250 ms; an nvim still starting gets its 2 s to be ready too) | ends the text |
| `:q`, `:wq`, `ZZ` (nvim exits) | ends the text with the confirmed buffer |
| ctrl+return (`new-text`) | ends the text and starts the next below, in insert mode |
| a press outside the grid | ends the text (after the barrier) and does what it does today |
| a press or drag inside the grid | nvim's mouse (`nvim_input_mouse`) at that cell |
| cmd+z / shift+cmd+z | nvim's `undo` / `redo`; the canvas's once the text ends |
| cmd+v, dictation (Flick pastes) | `nvim_paste` at the cursor, one undo step |
| cmd+c / cmd+x in visual mode | `"+y` / `"+d` (nvim's clipboard is the system's, or the `CLIP` file in tests) |
| cmd+a | `ggVG` |
| any other cmd chord | the app's keymap, as now |

The key overlay's "Typing a text" rows (`scene/overlay/fixed.rs`) say
"nvim's keys" and list the rows above when nvim is on.

### The column (step 5, slinger-93b7c8)

- Core: `MOST_ROWS` = 80 next to `CELL` in `render/mod.rs`; `text_size` and
  the frame rule cap the height; canvas-file.md's `text` row says so. Tests:
  a 100-row text's frame is 80 rows tall, its packet line has the whole
  text, a put beside it lands beside the 80 rows.
- App: `glyphs::NEW_COLUMN` becomes 80; `glyphs::layout` caps rows the same
  way and gives the hidden count; the fade and "N more lines" row; step in
  on a text (`Trail` learns a text id; `canvases/step.rs`).
- Landed (core half, slinger-93b7c8): `render::MOST_ROWS`; `text_size` and
  `text_frame` cap the height (a frame written taller too), the crop draws
  80 rows, `text_rows` still counts every row (`AGENT_ROWS` reads it).
  Tests: `crates/core/tests/canvas_column.rs`.
- Found, not done: the packet's `page` section cuts every text to its
  first 80 characters (packet.md), so a user's long text is not whole in
  the packet unless it is in `new` or `made`, which a user's text never
  is. Quoting the user's texts whole in `page` takes the whole-board packet
  in `crates/core/tests/packet_build.rs` from under 55 700 to 64 971
  tokens: a ratchet, so it waits for the owner.
- Landed (app half, slinger-93b7c8): `glyphs::NEW_COLUMN` = 80;
  `glyphs::MOST_ROWS`, `rows_frame` capped and `whole_frame` not,
  `hidden`, `more` (the note's row); `scene/canvas/capped.rs` draws rows
  1 to 76, rows 77 to 79 fading, the note in row 80; `Mirror::step` takes
  a text as the last step (`Inside::text`), and the pane shows it alone in
  its whole frame; `step-in` takes a selected capped text when no box is
  selected (`Facts.steps`); two quick taps on the note's row step in
  (`Follow::miniature_under`). Slinger's own editor draws every row while
  it types (`Editing::whole`). Its context menu offers Step in and Step in
  beside (`Kind::LongText`); the commands read "Step in", "Step in beside"
  and "Step out" (slinger-69f995).
- Landed with it: nvim 0.11 with `wrap`, `linebreak`, `breakat=" "` at
  80 columns moves a word that ends exactly at cell 80 to the next row when
  a space follows it (the space must fit too); at a line's end it stays.
  Measured against nvim on 7,000 random lines: a row breaks after the last
  space in its cells that a non-space follows, never in the line's leading
  spaces, the space ending its row; with none, it is cut at the cells. The
  core's `raster::wrap` (rows keep that space) and the app's
  `glyphs::layout` follow it.

### Write focus (step 6, slinger-9321cf)

Command `write-focus`, "Focus on text", default cmd+alt+return (free on
main; cmd+shift+return is `dictate`). On the text being edited (or the
selected text) it zooms and pans the pane so the 80-cell column fills the
pane's width less a margin of 4 cells each side, its top a cell below the
pane's top; the view the pane had comes back when the text ends or on the
same chord again. Alternative not taken: a pane of its own for the text
(cuts the writing off from the ink around it).
Landed (slinger-9321cf): `crates/app/src/canvases/focus.rs` (`writing`, the
pure view; `Focus` keeps the view it replaced and the count of texts ended,
so any text ending gives it back); the top margin is one row (one size).
The context menu of a text and of the text being typed offer it
(slinger-668fd9); checked under nvim in a driven window: the chord and the
pick type nothing into nvim, and the view comes back when the text ends.

### The rendered view (step 7, slinger-45d66c)

A pure `scene/markdown.rs`: one stored line in, a list of (cell range,
style) out, for the subset of 4A; tested without AppKit. The painter is
`platform/cells.rs` with the styles mapped to the theme's ink.

- Landed (slinger-45d66c): `scene/canvas/markdown.rs` (a line's looks,
  fences carried line to line) and `scene/canvas/written.rs` (the rows'
  runs, `Written`), under `scene/canvas` because `scene.rs` is full; a
  canvas text's `Mark::Text` carries them as `written`, painted by
  `platform::cells::write`. Every canvas text's measure is its cells
  (`glyphs::Laid`, `rows_frame`), so slinger's own editor draws in the
  same cells too. A double tap on a word edits at its cell
  (`Laid::cell_at`; nvim's seed takes the cursor's line and byte).

### The agent rule (step 8, slinger-99a379)

- Core: a test that an agent's `set`, edit, move and remove of a user's
  text fail with the existing not-yours error (it holds today: the test
  holds it); the user's character edit of an agent's text makes it the
  user's; `AGENT_ROWS` = 12 and `too_long` for an agent's put or edit.
- Guide and skills: `crates/cli/src/guide.md`, `plugin/skills/slinger` and
  `plugin/skills/slinger-listen` say an agent never edits the user's text,
  answers beside it, at most 12 rows each.

## Risks the steps carry

- A plugin that writes on start (lazy.nvim updating `lazy-lock.json` in the
  config directory, a plugin with its own data under `XDG_DATA_HOME`) can
  still write: slinger redirects state and cache only, or the owner's
  plugins would not load. The acceptance test holds config, state and
  cache under `--clean`; the owner's real config is a hand check.
- An Esc mapping in normal mode in the owner's config never fires (escape
  is slinger's there). If the owner wants it, the alternative is to end a
  text only with a press outside, ctrl+return or `:q`, and let escape be
  nvim's.
- Rows can differ between nvim's `linebreak` and the core's `wrap` in one
  case: a word ending exactly at cell 80 followed by a space. Step 5 tests
  it and moves the core to nvim's rule if they differ.
- A pen tap still opens a text, now in nvim: the pen's strokes and tools
  are unchanged (acceptance 8), but the editor behind the tap is.
- Binary size: the spike is about 1,800 lines; `platform/cells.rs` adds
  NSFont and attributed-string code. Measure against the `slinger-app`
  ceiling in step 3 before the painter lands; no budget moves.

## As built (2026-11-24, steps 2 to 8 and the adopted fixes)

What shipped, where it differs from the picks above or adds to them:

- **Every canvas text is drawn in Menlo cells**, the user's and the
  agents', edited or not, and slinger's own editor types in the same
  cells: one measure (`glyphs::Laid`, the core's `CELL`) for the caret,
  a press and the frame. The system font at 0.83 is gone from the canvas
  (the "N more lines" note alone keeps it).
- **Hard breaks are kept** (2A): the stored text is the buffer's lines; the
  rendered view draws each stored line from the first cell and wraps only
  a line longer than the column. Nothing joins them, though the seed of
  step 7 said to.
- **A double press edits at a word.** The design said a press; a single
  press with the select tool still selects, so a double tap (select tool,
  lasso or a mouse with the pen tool) opens nvim in normal mode with the
  cursor on that cell (`Laid::cell_at`; nvim's seed takes the line and
  byte). The text tool's tap opens at the end in insert mode, as before.
- **Write focus** is `write-focus` (⌥⌘↩, "Focus on text"), also in the
  right-click menu of a text and of a text being typed (slinger-668fd9);
  checked under nvim in a driven window: neither types into nvim.
- **Step in on a long text**: a double tap on the "N more lines" row, or
  `step-in` with the text selected (only when no box is selected), or its
  right-click menu (Step in, Step in beside, slinger-69f995). The commands
  read "Step in", "Step in beside", "Step out".
- **A size change keeps the cells** (slinger-5285e4): the size keys and a
  handle resize keep a text's 80 cells a row, so a new text stays 80 cells
  at any size.
- **The barrier goes in the key stream** (slinger-8854d9): escape and a
  press outside wait for `<Cmd>call rpcnotify(0, 'slinger_barrier', n,
  mode(1))<CR>` sent through `nvim_input` in turn with the keys, so the
  word comes only after nvim took every key before it, and it carries the
  mode those keys left (a request, as the first build's `nvim_eval` was,
  can be answered before nvim takes keys still queued, such as a lone
  escape held for `ttimeoutlen`). Undo and redo go the same way (`<Cmd>silent! undo<CR>`).
- **Wrap** follows nvim's `linebreak` in the core (`raster::wrap`), the
  app (`glyphs::layout`) and the script prelude's measure (step 5).
- Not built: a warm spare nvim (slinger-4ecf05, only if the start is felt);
  macOS dictation that arrives as typed text still goes as a paste
  (slinger-bbbce8); the written runs are not cached per text (slinger-625ecd);
  the packet's page section still cuts a user's text (slinger-2ba749).

## Acceptance (plan pl-b237f5), checked 2026-11-24

Tests named by function; "driven" is a driven window on a scratch daemon
with `SLINGER_NVIM=clean` (nvim 0.11): four paragraphs of markdown typed,
ended with escape twice, drawn as markdown, opened again by a double tap
in normal mode on the tapped character, write focus in and out, 105 lines
made with `yy99p` (grid 80x81, then a capped text saying "26 more lines").

| # | Acceptance | State |
|---|---|---|
| 1 | Write and revise multi-paragraph markdown with your own nvim config, without leaving slinger | needs the owner (docs/hand-check.md, the writing-column script); driven with `--clean` |
| 2 | A text not edited shows rendered markdown; a press on a word edits there | met: `scene::canvas::markdown` tests, `a_double_tap_on_a_word_opens_nvim_in_normal_mode_with_its_cursor_on_that_cell`, driven (a double press, see As built); the look in light and dark needs the owner |
| 3 | 80 cells wide, never taller than 80 lines, read by stepping in | met: `a_text_is_at_most_most_rows_tall_however_its_frame_was_written`, `the_crop_of_a_long_text_draws_its_most_rows_only`, `a_text_past_most_rows_is_framed_at_most_rows_and_says_what_is_hidden`, `a_long_text_says_how_many_more_lines_and_steps_in_by_two_taps_or_step_in`, `a_size_step_keeps_a_new_text_s_80_cells_and_its_rows`, driven |
| 3 | ... and its whole text is in the packet | **not met**: the item keeps it (`the_whole_text_is_kept_in_the_item_and_the_packet_boxes_its_most_rows`), but the page section cuts a user's text to 80 characters; quoting it whole moves the token ratchet, an owner decision (slinger-2ba749) |
| 4 | Editing writes nothing under your nvim config, state or cache | met for `--clean` with all four homes redirected: `an_nvim_edits_the_text_and_writes_nothing_but_its_state_and_cache`; your real config needs the owner (script steps 1 and 11) |
| 5 | No way of ending loses the text | met: `keys_typed_before_nvim_is_ready_land_in_order_and_escape_twice_ends_the_text`, `quitting_nvim_ends_the_text_with_its_buffer`, `a_crash_ends_the_text_with_the_buffer_last_confirmed`, `a_text_left_before_its_buffer_is_confirmed_keeps_what_it_had`, `an_nvim_not_ready_in_time_leaves_the_text_as_it_was`, `an_nvim_that_cannot_start_leaves_the_text_to_the_apps_editor_and_says_why`, `a_tap_outside_ends_the_text_and_is_used_up_a_tap_inside_is_not`, `a_text_being_typed_when_the_app_quits_is_written` (slinger's editor); the app quitting while nvim edits needs the owner (script step 10) |
| 6 | With nvim missing or off, the scene editor works and every test passes | met: `slinger_nvim_turns_it_on_or_off_and_the_config_picks_which`, `the_editor_nvim_is_auto_off_or_a_path`, the one above that cannot start; unit tests and a driven window start nvim only with `SLINGER_NVIM` set; `cargo xtask check-all` green |
| 7 | An agent cannot change a user's text; the guide and both skills say where a reply goes | met: `an_agent_cannot_change_a_text_the_user_wrote_by_any_verb`, `the_users_edit_of_an_agents_words_makes_the_text_the_users`, `an_agents_text_is_at_most_twelve_rows`; `crates/cli/src/guide.md`, `plugin/skills/slinger/SKILL.md`, `plugin/skills/slinger-listen/SKILL.md` |
| 8 | With a pen, nothing changes | met by every pen and tool test, unchanged and passing; a pen tap on a text now opens it in nvim (a risk above); a real pen needs the owner (script step 5) |
| 9 | `cargo xtask check-all` green, no budget loosened | met: no budget moved by the plan |
