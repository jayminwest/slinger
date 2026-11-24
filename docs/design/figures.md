# Figures: an agent draws with code

Status: design closed overnight 2026-11-24 (slinger-e8d6e3, plan pl-9f74c2,
parent slinger-cb0227), owner to confirm the picks in "Recommended" at the
end. Inputs: SPEC D6 and section 4, B4, B5, B6, the canvas file
(`docs/design/canvas-file.md`), the packet (`docs/design/packet.md`), folds
(`docs/design/fold.md`), live items (`docs/design/live-items.md`), the plan's
context, approach and alternatives (`sd plan show pl-9f74c2`) and its seeds.

The draft this page was to finish (the spike results and the owner's
answers) never reached the repository: the spike is gone and its numbers
are kept only where the plan and its seeds quote them. This page rebuilds
the design from those, and every later step of the plan reads this page
alone. A step that cannot do what it says reports that; it does not pick
another way.

## What it is for

Agents answer on the canvas with a paragraph of text. The reference is a
tldraw worksheet an agent made from one script: 64 placed shapes (plots,
dashed answer areas, dotted areas the agent later filled) that the user wrote
1,023 strokes on. Slinger cannot make it today: items have no fill, dash,
lock or plain line; placing 60 items by verb is slow and error-prone for an
agent; and nothing names "the answer area for 1a" so a later turn can find
it. The worksheet is one example: the same mechanism covers diagrams, plots,
tables, boards and marks drawn over the user's own work.

**A figure** is a set of ordinary canvas items that share a name (`fig`),
some with a **part** name, locked against the user's pen by default, made by
one agent from one **script** and its **data**. The script is JavaScript the
agent writes; the daemon runs it in an embedded, bounded QuickJS and places
what it returns. An update is a data patch (`canvas redraw`): the daemon runs
the script again and rewrites only what changed.

## Decided by the owner (2026-11-24, not reopened here)

Collected from the plan's approach and alternatives and the seeds' "Decided"
lines. They do not go in `docs/open-picks.md`.

1. **The engine:** QuickJS (quickjs-ng through rquickjs, MIT), in the
   daemon's process, in a new crate `crates/script` that only the daemon
   links. Pure: script, data and a read-only view in; shapes out. Rejected:
   the agent running its own script into `canvas apply` (no figure, no
   parts, no redraw), a declarative layout tree, an HTML live item (no pen
   on it), Boa (no memory limit, 175 crates, 3.7x slower), Starlark (failed
   the worksheet; no classes).
2. **The nouns and verbs:** figure and part; `canvas draw`, `canvas redraw`,
   `canvas figure`. A figure is its script plus its data. Membership is a
   field on each item, never a figure item that lists child ids.
3. **Locked by default, and the unlock:** a figure's items are locked
   against the user's pen. One command, "unlock figure", on the figure under
   the pointer: it selects the whole figure as one unit (move, duplicate,
   delete), and the figure is locked again when deselected. No lock/unlock
   toggle, no single-item unlock. A duplicate of a whole figure is a new
   figure with a new name.
4. **Script units:** whole canvas units, the units the packet prints. A
   script never sees tenths.
5. **The default figure colour:** the agent colour, which the daemon passes
   in. The agent colour is a theme config key (the app's
   `crates/app/src/config/theme.rs`). Colour defaults nest: `fig.color`, then
   part, group, stack, table and plot; the nearest wins.
6. **Fills are always under ink:** under every outline, stroke and text,
   above PDF pages. Nothing raises a fill.

Also decided: named theme colours beside hex (the names and the plot series
are picked below), and the fluency rules from the spike: `arrow` takes
`line`'s arguments or `(from, to)`; bad arguments fail at the call, by name;
an arrow's label sits beside the arrow, not on it; the engine warns on text
over a line and on overlapping texts.

## Owners

| Step | Seed | Owns |
|---|---|---|
| 1 | slinger-e8d6e3 | this page, SPEC D6 and section 4, the "Figures" open picks |
| 2 | slinger-c0c4c2 | core: `fill`, `dash`, `heads`, `closed`; file, ops, show, raster |
| 3 | slinger-450877 | app: draw them; the fill pass; the theme's `agent` key |
| 4 | slinger-097d16 | core: `locked`, `fig`, `part`; the address; `Canvas::figures` |
| 5 | slinger-dd5add | app: the gesture table; `unlock-figure`; paste and duplicate |
| 6 | slinger-b860be | core: the figure line in the packet, `over=`, `read --in` a part |
| 7 | slinger-3ce137 | `crates/script`: the engine, the prelude (Appendix A), limits |
| 8 | slinger-387b76 | core: place shapes as a figure; replace by match; `figure_drawn` |
| 9 | slinger-14a8ab | `canvas draw` through control, daemon, CLI; `--dry-run --png` |
| 10 | slinger-2ee5da | `canvas redraw`, `canvas figure` |
| 11 | slinger-9af735 | guide, both skills, the sling note, the e2e sling |
| 12 | slinger-b0b24a | the fluency round; records it under "Fluency" here |
| 13 | slinger-3ca75e | tldraw import keeps fill, dash, isLocked |
| 14 | slinger-9dcf50 | acceptance in a driven window |

## What an old reader does with new fields

The rules are `docs/design/fold.md`, "What an old reader does today", findings
1 to 8. For optional fields on known kinds they come to this:

- An old reader ignores the unknown fields (finding 3) and draws the item
  as it knows it: a filled box as an outline, a dashed line solid, a
  headless line as an arrow, a closed stroke open, a locked item as free.
- Its `set` keeps them (finding 6): replay lays a `set`'s fields over the
  item's own, so a field it did not name stays.
- Its `restore` and `page.restore` (undo of an erase or of a page
  removal) write whole items from memory without them (finding 5). After an
  erase and undo in an old build, a figure's item comes back plain: no fill,
  no dash, no lock, and no longer in its figure. **The style or the
  membership is lost, never the item.** A redraw puts a lost member back as
  a new item (it no longer matches) and the plain one stays as an agent's
  item: visible, removable by the user.
- No field changes the type of an existing one (finding 7), and no item's
  existence depends on another's field.

So no `v` change: the fields join `size`, `live`, `button`, `pdf` and `fold`
in the canvas file's Versions list. Step 2 and step 4 each add an
old-reader test as fold's: replay a file with the new fields, and the same
file with every new key stripped, and every item of the first is in the
second at the same place.

## The model (B6: where the style and the membership live)

### A: optional fields on the item object (picked)

Seven optional keys on the item objects of the kinds that use them, written
after the kind's own fields, left out at their default so a file without
them reads and writes the same bytes:

| field | on | value | default (absent or `null`) |
|---|---|---|---|
| `fill` | `box`; `stroke` with `closed` | a colour, `#rrggbb` or `#rrggbbaa` | no fill |
| `dash` | `box`, `stroke`, `arrow` | `"dashed"` or `"dotted"` | solid |
| `heads` | `arrow` | `"none"`, `"end"` or `"both"` | `"end"` (the arrow of today) |
| `closed` | `stroke` | `true`: the last point joins the first | open |
| `locked` | any kind | `true` | free |
| `fig` | any kind | a figure name (below) | in no figure |
| `part` | any kind with `fig` | a part name (below) | in no part |

- Order on a line: the kind's fields (and `fold`), then `fill`, `dash`,
  `heads`, `closed`, `locked`, `fig`, `part`. `heads` is written only
  when not `end`; `closed` and `locked` only when `true`.
- Written by `add`, `set` (each as `null` when absent, as a pdf's `crop`, so a
  `set` clears them), `restore` and `page.restore`; shown by `canvas_show`.
- **Reading a file** never fails on them: a value of the wrong type, an
  unknown `dash` or `heads` word, a `fill` on a kind that cannot fill, or a
  `part` without `fig` reads as the default. A colour that does not parse
  reads as no fill (not black: a fill is never guessed).
- **Applying a request** is strict: each of those is `invalid` with a hint
  naming the field and what it takes, as `size` is today.
- **A fill is a tint.** It is drawn at its alpha, at most `0x66` (40%):
  a higher alpha draws as `0x66`. The rule is one constant in the core
  (`canvas::style::FILL_MAX_ALPHA`) that the app's fill pass reads too. So
  ink over a fill always reads, in either theme (see "Colour").
- **Locked is for the user's hands in the app only.** The core never reads
  it: the author rule (`Canvas::may_change`) is unchanged, an agent edits its
  own locked items, and the user's CLI verbs act on locked items as on any.
- **Lock and unlock as a write** is a `set` of `locked` (`true` or `null`),
  by the maker or the user, through `canvas apply`: no new operation kind and
  no new verb. The app never writes it (the unlock is a grab, below).

The core keeps them out of the full files. `crates/core/src/canvas/item.rs`
(457 lines) and `crates/core/src/canvas/file/json.rs` (447) are near the
500-line limit, so:

- **`crates/core/src/canvas/style.rs`** (new): `pub struct Style { fill: Option<Colour>, dash: Dash, heads: Heads, closed: bool }`,
  `enum Dash { Solid, Dashed, Dotted }`, `enum Heads { End, None, Both }`,
  `FILL_MAX_ALPHA`, which kinds take which field (`Style::fits(&Shape)`),
  and `impl Item { pub fn style(&self) -> &Style }`.
- **`crates/core/src/canvas/figure.rs`** (new): `pub struct Member { locked: bool, fig: Option<String>, part: Option<String> }`,
  the name rules, `Address` and its parser, `Canvas::figures()`,
  `Canvas::figure(name)`, `Canvas::part(&Address)`, and
  `impl Item { pub fn member(&self) -> &Member }`.
- **`crates/core/src/canvas/file/style.rs`** (new): the seven keys to and from
  JSON: `write(o: Obj, &Style, &Member) -> Obj`, `read(&Fields) -> (Style, Member)`
  (lenient, for replay), `check(&Fields, &Shape) -> Result<()>` (strict, for
  apply), and the `null` keys `fields` adds for a `set`. `json::shape`,
  `json::read_shape` and `json::fields` each call it in one line.
- `Item` gains two fields, `style: Style` and `member: Member` (both
  `Default`), so no `match` on `Shape` changes. They sit beside `shape`, not
  in it: a `set` that changes a shape keeps them unless it names them.

### B: new kinds, or a style table

A `fbox` / `line` / `polygon` kind for each styled shape, or one `style`
table in the header that items point into.

- New kinds: an old reader keeps them as unknown and **does not draw them**
  (finding 2): the worksheet's scaffold disappears in an older build, and
  every hit test, packet line and app path needs each new kind.
- A table: the header is written once and never rewritten (the file is an
  append log), so a table needs new operation kinds to grow, and an item's
  look depends on another line: finding 2's "no item depends on another".

**Pick: A.** Optional fields keep every item visible to an old reader and
taking them back is ignoring seven keys.

### A headless line: an arrow with `heads: none` (picked) or a two-point stroke

- **Arrow, `heads: none`.** Its ends bind to items as an arrow's do, so a
  connector follows what it joins; it draws at the box edge's width; it is
  an `arrow` line in the packet (with `from`/`to`). An old reader draws a head
  on it: a wrong look, nothing lost.
- **Two-point stroke.** An old reader draws it right; but it cannot bind,
  and the packet treats an agent's stroke as ink (one `ink` line with
  `by`), so a table's rule lines would read as ink.

**Pick: the arrow.** `line()` in the script API makes one. A stroke stays
what a pen (or `poly`) draws.

## Draw order

One order, in the core raster and in the app:

1. PDF pages (`pdf` items), in item order (as today: `paper_first`).
2. **Every fill**, in item order: a filled box's inside, a closed stroke's
   inside. A fill is clipped to nothing else.
3. Everything else in item order: outlines, strokes, arrows, text, live
   items, buttons.
4. App only, above all of the page: a folded box's peek panel and what it
   shows (with its own fills first, inside the panel), then selection and
   the caret.

- **Core:** `render::draw` (`crates/core/src/render/mod.rs`) paints the
  backdrops, then `Raster::fill` for each item with a fill (a new method,
  in a new `crates/core/src/render/raster/fill.rs`: scan-line coverage of the
  polygon, blended at the fill's alpha capped by `FILL_MAX_ALPHA`), then
  `Raster::item` for each as now. In `render::crop` a `pdf` item is a
  grey frame, drawn in step 1 all the same. Existing golden images do not
  change: a canvas with no fill draws byte for byte as before.
- **App:** `paper_first` (`crates/app/src/scene/ink/shown.rs`) keeps the item
  order every hit test takes. The fill pass is a draw-time pass only: the
  platform paints, for the list it is given, each item's fill first, then
  each item as now. Hit tests do not change order, and a fill is not a hit
  (a press inside a filled box's edges is a press on what is under it, as
  for any box). The peek panel (`canvases/fold.rs::drawn` and the panel mark)
  is drawn after the page's pass, so a fill never covers it. A miniature
  draws its members' fills under its members' ink, inside the miniature.
- Dash: `dashed` is 8 units on, 6 off (the app's `DASH` for the panel,
  scaled for the line width); `dotted` is a round dot of the line width every
  6 units. A dash starts at a box's top left corner and at a line's first
  point. Arrow heads: the app's barbs (12 units, 30 degrees) at `to`, and at
  `from` too for `both`; the core raster's head (16 units) likewise. A
  closed stroke draws its last point joined to its first; the app's
  `smooth` closes the curve (wraps its neighbours) for it.

## Colour

### The names (picked)

A script, and only a script, takes a colour as hex (`#rrggbb`,
`#rrggbbaa`) or as one of these names. The canvas file keeps hex only.

| name | canvas value | what it is |
|---|---|---|
| `agent` | `#8a5cf5` | the agent colour; every shape's default |
| `ink` | `#000000` | the user's default ink (the app shows it light on dark, as any black) |
| `muted` | `#8c8c8c` | secondary text, tick labels |
| `accent` | `#4c9aff` | the theme's accent (`focus`) |
| `error` | `#e5484d` | wrong, missing |
| `series1` .. `series6` | `#2f80ed`, `#d55e00`, `#009e73`, `#cc79a7`, `#a87a00`, `#178a8a` | the plot series, in order |
| `black`, `white`, `red`, `green`, `blue`, `yellow`, `orange`, `purple`, `grey` | the CSS colours | the names the packet prints for ink (`packet::line::colour_name`), so a colour read can be given back |

- The theme names are the `--slinger-*` names a live UI already uses
  (`docs/design/live-items.md`) plus `agent`; `paper` is left out (a paper
  fill is a white block on the dark theme and hides nothing a fill may hide).
- The series are the Okabe-Ito hues, darkened where needed so each has a
  contrast of at least 3:1 on white and on the dark theme's `#1e1e1e`;
  `agent` has about 4:1 on both and is none of them. `plot` takes the
  series in order for its lines and dots, from `series1` in each plot.
- The table is one `const` in `crates/script` (the prelude resolves the name,
  so an unknown name fails at the call and lists the names). The `agent`
  value is `slinger_core::canvas::style::AGENT`; the daemon passes it in
  (`View.agent`), and a daemon test checks the script's table has the same
  value, since `crates/script` does not link the core.

### Stored on the item, or resolved to hex at run time (B6)

- **A: resolve at run time (picked).** The prelude turns a name into hex
  before the shape leaves the engine; the file holds hex as today. Old
  readers draw it right. A theme change does not reach a figure already
  drawn, but a figure keeps its script and data, so a redraw resolves again:
  theme-aware later is a redraw, not a file change.
- **B: store the name.** `color` keeps hex for old readers and a new optional
  `tone` names the colour; the app and the raster map the name through the
  theme. Theme-aware at once, but every reader and the packet must decide
  what a name means, and the field is on disk for good. (A name in `color`
  itself reads as black in every build: `json::parse_colour`.)

**Pick: A.** It is the cheaper to reverse: B can come later as an added key
that a redraw writes, while names once on disk stay there for good.

**The agent colour in the app.** The owner made the agent colour a theme
key. With A it is one draw-time rule beside `legible()`: the app's
`[theme] agent` key (in `crates/app/src/config/theme.rs`, default `#8a5cf5`,
listed in `config.example.toml`) is the colour the app draws for a stored
colour exactly `AGENT`, in ink and in fills. Nothing is written. A user who
picks another agent colour sees every agent's default-coloured figure in it.

### A fill in the dark theme (B6)

- **A: as stored, never through `legible()` (picked).** A fill is drawn at
  its colour and alpha (capped at 40%) over the background, in both themes.
  Being a tint, a light fill on dark paper is a soft wash and ink over it
  (which `legible()` may lighten) still reads.
- **B: flip a fill as ink is flipped.** `legible()` swaps a colour too
  close to the background for the theme's text colour: a pale yellow wash
  would turn into a light grey block on dark paper, and the meaning of
  "yellow means given" is lost.

**Pick: A.** The script API's `fill` is a tint by default: a fill given as
a name, as `#rrggbb` or as `true` (the shape's own colour) is stored at alpha
`0x33` (20%); `#rrggbbaa` is stored as given and drawn capped at `0x66`.

### Defaults that nest

Each shape's colour, and its `dash`, `fill` and text `size`, is the first of:
the shape's own option, then the innermost enclosing `group`, `stack`,
`table`, `plot` or `part` that sets it (they nest in the order the script
calls them; the nearest wins), then `fig.color` (colour only, starts as the
agent colour), then the built-in default (solid, no fill, size 24). A
plot's series colour is the plot's own default for its lines, not for its
axes, which take the plot's colour.

## Figure and part

### Names

- A **figure name** is 1 to 40 characters of `a-z`, `0-9` and `-`, starts with a
  letter, and is not shaped like an item id (a letter then only digits:
  `b4`, `q1`), so a name and an id never read alike. `worksheet`,
  `q1-plot`, `sprint-board`.
- A **part name** is 1 to 40 characters of `a-z`, `0-9` and `-`, any first
  character (`1a-answer`, `q1`). Several items share a part; that is what a
  part is for. An item in a figure may have no part.
- A figure's name is **unique on its canvas**, across pages: an address
  names a canvas, never a page. All of a figure's items are on one page
  (a draw places on one page; a move of items to another page is not a
  thing the canvas does).
- A figure **exists** while any item on the canvas has that `fig`. Its
  **maker** is the author of its items. Its frame is the joint frame of
  its visible items. Erasing every item ends it.
- Applying a request does not police names: `add` and `set` may write any
  valid `fig` (undo, import and paste need it). The clash rules below are
  the verbs' and the app's.

### The address

```
address = figure [ "/" part ]
```

- `worksheet` names the whole figure; `worksheet/1a-answer` names the items of
  that part. Either is taken wherever a canvas verb takes an item id:
  `put --beside`, `put --in` (when the part is one box), `edit` and `remove`
  (each item named), `crop --item` (all of them), `read --in`, `draw --over`
  and `draw --beside`.
- Resolution is one function, in the place every id already goes through:
  `Log::resolve` (`crates/core/src/canvas/file/look.rs`), which `Log::named`
  (`file/put.rs`) calls. Order: an id shape (`i12`, `c12`, `b4`) is an id; a
  token with `/` is a part; anything else is a figure. It returns the
  items in id order; `named` returns their joint frame and the page, and
  for an id the first item (the lowest id) stands for them.
- An address that names nothing is `not_found`, hint: "list the figures with
  'slinger canvas figure <canvas>'". A malformed one is `invalid` with the
  name rules.
- `Canvas::figures() -> Vec<Figure>` (name, maker, page, frame, parts with
  their frames, in reading order) is what `canvas figure` and the packet read.

### A clash

- `canvas draw --name N` when N exists on the canvas: by its maker, it is a
  redraw with the new script and data (replace by match, below); by anyone
  else, `conflict` (exit 4): "figure N is agent:other's", hint: "pick another
  --name". The user (no `--agent`) may draw over a figure of any maker, as
  the user may change any item.
- A whole figure the app copies, duplicates or pastes takes a free name: the
  name with its trailing `-<n>` taken off, then `-2`, `-3`, ... the first not
  on the canvas (`worksheet` gives `worksheet-2`). Its parts and its lock stay;
  its maker is the user (the user made the copy), so the agent cannot
  redraw the copy, only the original.

## Lock

### The unlock (designed from the owner's pick)

There is no stored toggle and no single-item unlock. The user grabs a
figure:

- **Command:** `unlock-figure`, title "Unlock figure", group Canvas (Edit menu,
  palette, key overlay), default key opt+cmd+U. It acts on the figure under
  the pointer in the focused pane: the topmost locked item whose bounds
  (a stroke's with its width, a box's or a closed stroke's fill area) hold
  the pointer, and its `fig`. A locked item with no `fig` is a figure of
  one. Nothing locked under the pointer: the command flashes "No figure
  under the pointer" and does nothing; `able::can` greys it in the menu
  when the facts say no locked item is under the pointer.
- **Context menu:** a secondary click on a locked item opens the canvas's
  menu with "Unlock figure" at its top, acting on that figure (the item
  that was under the click, carried as the subject).
- **The grab:** the selection becomes every visible item of the figure, held
  as one unit (`Select` gains `grabbed: Option<String>`, the figure name;
  pane state, never written). While grabbed: a drag inside the figure's
  frame moves all of it; the nudge keys move it; delete removes all of it
  (one step); duplicate (cmd+J), copy, cut and paste work on all of it and
  a copy is a new figure (above); the corner handles are not shown (no
  resize: a script decides sizes; a redraw is how a figure changes size).
- **Relock:** anything that ends that selection ends the grab: escape, a
  press outside the figure, select-all, a tool change to the pen, eraser or
  text, a page or canvas change. Nothing is written by the grab or its end.
- An agent's `set` of `locked` (above) is the only stored unlock, and the app
  shows it as any free item.

**B6 for the grab, where it lives:** (a) a flag on the selection (picked):
every gesture that acts on a selection acts on the figure with no new
path, and every hit test still skips locked items, so the figure under the
grab is moved by the selection's own drag; (b) a separate "figure grab"
mode with its own drag and keys: a second selection model to keep in step.

### The gesture table

Every gesture that picks items by place skips a locked item, unless the
figure is grabbed. **Skip** means the gesture behaves as if the item were
not there: what is under it, or empty paper. Ink drawn over a locked item
is written as on empty paper.

| gesture | where it picks today | locked item |
|---|---|---|
| pen stroke | none (writes) | drawn over as on empty paper |
| eraser | `Select::drag` (`scene/select.rs`) | skip: passes over |
| lasso | `takes()` (`scene/select.rs`) | skip |
| marquee (rectangle select) | `takes()`, `innermost` (`scene/select/pick.rs`) | skip |
| tap to select | `topmost` / `innermost` (`scene/select/pick.rs`) | skip: selects what is under, or clears |
| select-all (cmd+A) | `takes()` | skip |
| next / previous item (tab) | the page's items | skip |
| text tool tap, double tap to edit | `text_at` (`scene/shapes.rs`) | skip: a new text starts there |
| arrow tool, binding an end | `bind::target` (`scene/shapes/bind.rs`) | skip: the end stays free |
| fold mark press, fold command | `mark_under` (`canvases/fold.rs`), the selection | skip: a locked box neither folds nor unfolds by hand |
| step in (two taps on a miniature) | `mark_under` / `topmost` | skip |
| crop tool | `topmost` (`scene/crop.rs`) | skip: crops the free item under |
| find (cmd+F) | `Canvases::found` | finds and shows a locked text, does not select it |
| secondary click | `window/context.rs::subject_at` | the canvas's menu plus "Unlock figure" |
| unlock-figure | its own hit (above) | the one gesture that picks a locked item |
| drag, nudge, delete, duplicate, copy, cut with a grab | the selection | act on the whole figure |
| sling (the selection, a page, a box) | the packet | unchanged: a figure is in the packet (below) |

The skip is one predicate, `scene::canvas::free(&Item, grabbed: Option<&str>) -> bool`
(locked items need `locked` and `fig` on the app's `Item`), called by each
site in the table; step 5 adds a test per row that a locked item is
passed over and, for the selection rows, that a grabbed one is not.

### Paste and duplicate

- A paste or duplicate of items that are **not a whole figure** drops `fig`,
  `part` and `locked`: the copies are the user's free items. (With the grab
  being whole, this is a clip from elsewhere: an agent's `canvas show`
  objects, another canvas's partial clip.)
- A paste or duplicate of **a whole figure** (every object of the clip with
  a `fig` has the same one, and the clip holds every item the figure had
  when copied, which `Mirror::copied` records as a count in the clip) keeps
  `part` and `locked` and takes a free name. The rule is one function in
  `crates/app/src/mirror/clip.rs` beside `pasted`, which today keeps unknown
  keys.
- Styles (`fill`, `dash`, `heads`, `closed`) are kept in both cases: they
  are the look, not the membership.

## In the packet

- **A figure is one line**, `figure` kind, its id the figure's name, its box
  the figure's frame, then `by`, then `parts=` (the part names, comma-joined,
  in reading order of their frames), then `locked` when any item is
  (`locked=yes`), then the quoted text: the first line of the text of part
  `title` if there is one, else of its first text in reading order, cut at
  80 characters as in `page`:

  ```
  worksheet figure 40,40 900x1400 by=claude parts=title,name,q1,q1-plot,1a-answer,... locked=yes "Compound growth"
  ```

  Its items make no lines of their own. A figure an agent drew or redrew in
  `from..=to` goes in `made`, else in `page`. The line goes where the
  figure's lowest item id would go: when that item is hidden in a folded
  box, the figure is one of the lines the box's `folded=<n>` counts.
- **Ink over a part:** an `ink` line gains figure parts in `over`, after the
  `pdf` ids: each part whose frame holds the centre of the cluster's box,
  the smallest first, as `figure/part`: `over=worksheet/1a-answer`. A
  cluster over no part has none. `in` never names a box inside a figure
  (those make no line).
- **Arrow ends** bound to an item of a figure name its address (the part, or
  the figure when the item has none).
- **`canvas read <ID> --in worksheet/1a-answer`** is the packet of that part as if
  it were a page: one line per item of the part (with `part=`), and every
  other item, the user's ink above all, whose box's centre lies inside the
  part's frame. `--in worksheet` is the whole figure, item by item, each with
  `part=`. The header says `in=worksheet/1a-answer`.
- Packet version stays 1: a new kind and new fields (B5). Stored packet
  snapshots without figures read and print the same.

## The script API

The full reference is Appendix A: it goes into `crates/cli/src/guide.md` as
written there. In short: globals `box`, `line`, `arrow`, `poly`, `circle`,
`text`, `measure`, `part`, `group`, `stack`, `table`, `plot`, `warn` and
the object `fig` (`name`, `data`, `view`, `color`). Units are canvas
units, y down, origin the figure's own top left before placement. Each
call emits shapes at once; nothing is returned to place later.

### Limits and purity (step 7)

- 1 s of run time (an interrupt handler), 32 MB of memory, 512 KB of stack,
  a script of at most 1 MiB, data of at most 256 KiB as JSON, at most 5,000
  items. Each limit fails with its own message and the line:col where the
  script was.
- No `Date`, timers, `console`, `import`/`require`, `eval` of strings, or IO.
  `Math.random` is a seeded generator (seed 1 each run), so the same script
  and data give the same bytes out, which replace by match relies on.
- Errors keep the script's line and column: `script:12:7: box: w must be a number above 0, got "wide"`.
  A bad argument fails at the call, by the function's name. The line of
  a stopped run is where QuickJS last recorded a position: a loop whose body
  calls nothing (`while (true) n += 1`) can be named at the line before the
  loop; a loop that calls anything is named inside its body. Warnings
  (text over a line, texts overlapping, a shape far outside the others)
  come back beside the shapes, with their line, and never fail a draw.

### How shapes cross to the daemon (B6)

- **A: item objects as JSON (picked).** The engine returns
  `Drawn { items: Vec<serde_json::Value>, warnings: Vec<Warning> }`; each item
  is a `canvas apply` `add` item object (kind, color, the kind's fields, the
  seven new keys; no `page`, no `id`), positions and lengths in **tenths**,
  relative to the figure's origin. The prelude converts units to tenths as
  it emits, so the core's existing strict item reader (the one `apply`
  uses) checks every shape: one schema, one checker, and `crates/script`
  needs no core type. Cost: a JSON round trip of at most 5,000 objects.
- **B: typed shapes.** `crates/script` defines its own `Shape` enum and the
  daemon converts it to the core's. Two models of an item to keep in step,
  and a converter that is a second checker.

**Pick: A.** `crates/script`'s one function:
`slinger_script::run(script: &str, data: &serde_json::Value, view: &View) -> Result<Drawn, ScriptError>`,
with `View { name: String, target: Option<[f64; 4]>, agent: String, page: String }` (the
figure's name, which every shape carries as `fig`; the `--over`/`--in` item's frame, of which
the script sees the size; the agent colour as hex; the page name) and
`ScriptError { line: u32, col: u32, message: String }`. Text measure in the
prelude is the core's rule (a cell is `render::CELL` of the size wide, one
size tall); a daemon test pins it against `slinger_core::render::text_size`
(slinger-ce52ee: the app draws text proportional, so a script's measured
width is the core's, wider than the app's for most text; scripts leave room).

## The verbs

### `canvas draw` (step 9)

```
slinger canvas draw <CANVAS> <FILE> --name N (--beside ID | --over ID | --at X,Y | --in ID)
    [--data JSON|@FILE] [--dry-run] [--png PATH] [--agent NAME]
```

- The script crosses the socket as a path, as `ui put` does: the daemon
  reads the file, stores it as a blob (`Store::put_blob`), runs
  `crates/script`, calls the core, then appends `figure_drawn`. `--data` is
  inline JSON or `@file`; absent is `{}`. The data crosses as a file too
  (the request's `data` is a path; the CLI writes inline JSON to a scratch
  file), since a request line is at most 64 KiB (step 9's pick).
- **Placement** of the figure's frame (the joint frame of its shapes):
  `--beside ID` puts it where `Canvas::free_beside` finds room beside the
  item (or address); `--over ID` puts the figure's origin (0,0) on the
  item's top left and moves nothing (the script sees the item's size in
  `fig.view.target`, to draw over it); `--at X,Y` puts the origin there, in
  canvas units, on the first page or `<page>:X,Y`; `--in ID` puts the frame in a
  box's room (`Canvas::free_in`), as `put --in`.
- **Reply:** the figure's name, page, frame, part names, item count, `seq`,
  and the warnings. Text output prints the warnings only (one a line,
  `warning: script:12:7: ...`); success is otherwise silent. `--json` prints the
  reply.
- **`--dry-run`** writes nothing (no blob, no event, no op) and replies as a
  draw would, frame and parts placed as they would be; `--png PATH` writes
  the figure drawn alone on white by the core raster (a public
  `render::shapes(&[Item], id)` beside `render::items`, for items not yet on
  a canvas). `--png` without `--dry-run` writes the PNG of the figure as
  placed.
- One undo step. The reply counts as the agent's answer for a sling waiting
  on it (`reply_landed`, as `sling::landed`).
- New files: `crates/control/src/figure.rs`, `crates/daemon/src/server/figure.rs`,
  `crates/cli/src/draw.rs`; each verb one row in `crates/control/src/verb.rs`.

### `figure_drawn`

A record event, written by `draw` and `redraw` only (so not in `KINDS`, as
`ui_put`), tagged with the canvas and the agent (the maker):

```json
{"canvas":"main","figure":"worksheet","page":"p1","script":"<blob id>","data":{...},"origin":[400,400],"frame":[400,400,9000,14000]}
```

- `script`: the blob id of the script's bytes. `data`: the data in full (after
  a redraw's merge), inline. `origin`: where figure (0,0) was placed;
  `frame`: the figure's frame as placed; both in tenths, as the canvas file.
- The last `figure_drawn` of a (canvas, figure) is the figure's script and
  data. `canvas figure --script`/`--data` print it; `redraw` starts from it.
- `target` (since step 10, absent when none): the frame of the item drawn
  over or in, in tenths, so a redraw shows the script the same
  `fig.view.target` as the first draw.
- The payload type is `FigureDrawn` in `crates/core/src/record/body.rs`
  (`payload`, `kind`, `from_parts`); `crates/core/tests/record.rs` pins it.

### `canvas redraw` (step 10)

```
slinger canvas redraw <CANVAS> <NAME> --data JSON|@FILE [--script FILE] [--agent NAME]
```

- The data is a **JSON merge patch** (RFC 7396) on the figure's last data:
  objects merge key by key, `null` deletes a key, anything else (arrays
  too) replaces. `--script` swaps the script.
- Only the maker may redraw (the author rule's error, `not_yours`). A figure
  with no item left on the canvas is `not_found`, hint: draw it again.
- **Placement follows the user:** the new origin is the last `origin` plus
  how far the figure's frame's top left has moved since the last
  `figure_drawn` (the user moves a grabbed figure as one unit).
- Same merged data and same script as the last `figure_drawn`: nothing runs,
  nothing is written, no event. Otherwise the script runs and the result
  replaces the figure by match; `figure_drawn` is written only when at least
  one op was.

### Replace by match (step 8, B6)

- **A: match by (part, kind, order) (picked).** Old items are the canvas's
  items with `fig` = N made by the maker; new items are the script's
  output, placed. Key each item by its part (or none), its kind, and its
  ordinal among items of that part and kind (old: in id order; new: in emit
  order). For each key: both sides and equal (shape, style, member, after
  placement) → nothing; both sides and different → a `set` of the old item
  (same id, so arrows and notes bound to it hold); new only → `add`; old
  only → `remove`. All in one step, in that order: sets, adds, removes.
  The user's ink has no `fig` and is never touched. A script that reorders
  calls within one part and kind churns contents, never ids; across parts
  and kinds it churns nothing.
- **B: match by a key the script gives** (`key: 'row-3'` on each shape) or by
  content hash. Keys put the burden on every agent and a missing key falls
  back to A anyway; content hashes match nothing that moved, so a data
  change that shifts a column rewrites everything.

**Pick: A.** The core function: `Log::place_figure(&mut self, by, name, items, at: Placement, once) -> Result<Placed>`
in a new `crates/core/src/canvas/file/figure.rs` (log.rs is 484 lines), with
the match a pure function in `crates/core/src/canvas/figure/matching.rs`
tested on its own.

### `canvas figure` (step 10)

```
slinger canvas figure <CANVAS> [NAME] [--script | --data]
```

No name: one line per figure (`name page x,y wxh by=<maker> parts=<n>`).
A name: its parts, one a line (`part x,y wxh items=<n>`). `--script` prints the
script's bytes, `--data` the data as JSON. `--json` as the rest of the CLI.
As built (step 10): positions in canvas units, `by=` the agent's name (or
`user`) as the packet writes it; the wire gives the script as a blob id
(the CLI reads it with `blob_get`) and only for a figure asked by name.
`canvas redraw` needs `--data` or `--script`; the core half is
`Log::again` and `merge_patch` (`crates/core/src/canvas/file/redraw.rs`).

## Fluency

Step 12 (slinger-b0b24a) records its round here: the five tasks, the model,
what ran first time, and every change it made to the API or its errors.
The spike's own round gave the rules in "Decided by the owner".

## Files to split first (measured 2026-11-24, limit 500)

Plan risk 3. A step adds to none of these before it moves code out (or puts
its code in the new files this page names). Lines at the time of writing:

| step | near the limit |
|---|---|
| 2, 4 (core model) | `crates/core/src/canvas/item.rs` 457, `crates/core/src/canvas/file/json.rs` 447, `crates/core/src/canvas/file/mod.rs` 480, `crates/core/src/canvas/file/log.rs` 484; tests: `crates/core/tests/canvas_fold.rs` 496, `crates/core/tests/canvas_file.rs` 435 (new tests in a new `crates/core/tests/canvas_style.rs` and `canvas_figure.rs`) |
| 3 (app draw) | `crates/app/src/scene.rs` 497, `crates/app/src/scene/canvas.rs` 480, `crates/app/src/mirror.rs` 458, `crates/app/src/platform/canvas.rs` 454, `crates/app/src/platform/mod.rs` 454, `crates/app/src/scene/ink.rs` 453, `crates/app/src/mirror/tests.rs` 495 |
| 5 (app lock) | `crates/app/src/scene/shapes.rs` 478, `crates/app/src/scene/shapes/tests.rs` 492, `crates/app/src/scene/tools.rs` 472, `crates/app/src/scene/tools/tests.rs` 492, `crates/app/src/scene/select.rs` 440, `crates/app/src/window.rs` 482, `crates/app/src/tests.rs` 491, `crates/app/src/config.rs` 496 (the `agent` key goes in `config/theme.rs`), `crates/app/src/scene/slung.rs` 500 |
| 8 (place) | `crates/core/src/canvas/file/log.rs` 484 (the code goes in `file/figure.rs`) |
| 9, 10 (verbs) | `crates/daemon/src/tests.rs` 485, `crates/daemon/src/store.rs` 490, `crates/cli/src/tests.rs` 486 (tests in new files) |

## Recommended (picked overnight, owner to confirm)

Picked by the brief's rule: the alternative cheapest to reverse, and the one
that changes least of what the pen does today. Each line is also in
`docs/open-picks.md` under "Figures".

1. **The model:** seven optional fields on the item object (`fill`, `dash`,
   `heads`, `closed`, `locked`, `fig`, `part`), no new kind, no `v`
   change; new modules `canvas/style.rs`, `canvas/figure.rs`,
   `canvas/file/style.rs`; `Item` gains `style` and `member`.
2. **A headless line** is an arrow with `heads: none`.
3. **Draw order:** PDF pages, then every fill, then the rest, in both
   renderers; a fill is not a hit; a fill draws at most 40% alpha.
4. **Colour names:** `agent` `#8a5cf5`, `ink`, `muted`, `accent`, `error`,
   `series1`..`series6` and the nine packet names; resolved to hex at run
   time; the app draws a stored `AGENT` in its `[theme] agent`.
5. **A fill in the dark theme** is drawn as stored, never through
   `legible()`; the API stores a fill as a 20% tint unless given `#rrggbbaa`.
6. **Names and addresses:** figure names unique per canvas, kebab case, never
   id-shaped; `figure[/part]` wherever an id is taken; resolved in
   `Log::resolve`; a draw on another maker's name is `conflict`; a copied
   whole figure takes `name-2`, `name-3`, ...
7. **The unlock:** `unlock-figure` (opt+cmd+U, Canvas group, and the context
   menu) grabs the figure under the pointer as a flag on the selection; no
   resize while grabbed; any end of the selection relocks; nothing written.
   Every place-picking gesture skips a locked item (the table), through one
   predicate.
8. **Paste and duplicate:** a whole figure keeps parts and lock and takes a
   free name; anything else drops `fig`, `part`, `locked`; styles stay.
9. **The packet:** a figure is one `figure` line named by the figure; ink
   gets `over=figure/part`; `read --in` takes a part or a figure.
10. **Shapes cross as JSON item objects** in tenths, checked by the core's
    apply reader; `crates/script` links no core type.
11. **Replace by match** on (part, kind, ordinal); redraw merges data by
    JSON merge patch, follows a user's move, and writes nothing (no event
    either) for the same script and data.
12. **`figure_drawn`** holds the script as a blob id and the data inline
    (at most 256 KiB), with origin and frame in tenths.
13. **`Math.random`** is seeded, not removed.

## Appendix A: the script API (goes into the guide)

A drawing script is JavaScript (ES2023, as QuickJS runs it). It draws by
calling the functions below; each emits shapes at once. Units are canvas
units, the units a packet prints; y grows down; (0,0) is the figure's
origin, which `--at` or `--over` places (`--beside` and `--in` place the
figure's frame instead). Text size is the height of a character cell (24
unless set); a character is half that wide.

**Colours** are `"#rrggbb"`, `"#rrggbbaa"` or a name: `agent`, `ink`,
`muted`, `accent`, `error`, `series1` to `series6`, `black`, `white`,
`red`, `green`, `blue`, `yellow`, `orange`, `purple`, `grey`.

**Options** every shape takes in its last argument, an object; each falls
back to the nearest enclosing `part`/`group`/`stack`/`table`/`plot`
that sets it, then to `fig.color`:

| option | takes | default |
|---|---|---|
| `color` | a colour | `fig.color` (the agent colour) |
| `fill` | `true` (a tint of `color`) or a colour (a 20% tint; `#rrggbbaa` as given). `box`, `circle`, `poly` with `closed` | none |
| `dash` | `"dashed"` or `"dotted"`. `box`, `line`, `arrow`, `poly`, `circle` | solid |
| `size` | a text size, 4 to 400. `text`, and tables' and plots' labels | 24 |
| `locked` | `false` leaves the shape free for the user's pen | `true` |

**Shapes.** Each returns `{x, y, w, h}`, its frame.

| call | draws |
|---|---|
| `box(x, y, w, h, opts?)` | a rectangle; `w`, `h` above 0 |
| `line(x1, y1, x2, y2, opts?)` | a straight line, no heads |
| `arrow(x1, y1, x2, y2, opts?)` or `arrow(from, to, opts?)` | an arrow; `from`, `to` are `[x, y]` or a frame a shape returned (the arrow meets its edge). `heads`: `"end"` (default), `"both"`, `"none"`; `label`: a text set beside the arrow's middle, never on it |
| `poly(points, opts?)` | a line through `[[x, y], ...]` (two or more); `closed: true` joins the ends; `width` (default 2) |
| `circle(cx, cy, r, opts?)` | a closed line of 48 points |
| `text(x, y, str, opts?)` | text from its top left; `w` wraps it at that width |
| `measure(str, opts?)` | draws nothing: `{w, h}` that `text` would take (`size`, `w`) |

**Structure.**

| call | does |
|---|---|
| `part(name, fn)`, `part(name, opts, fn)` | every shape `fn` draws is in part `name` (`a-z`, `0-9`, `-`, up to 40). Parts do not nest. A later turn finds it as `<figure>/<name>` |
| `group(opts, fn)` | defaults for the shapes `fn` draws |
| `stack(x, y, opts, fn)` | `fn(s)` places one thing after another: `s.text(str, opts?)`, `s.box(w, h, opts?)`, `s.gap(n)`; `opts.dir` `"down"` (default) or `"right"`, `opts.gap` (default 8). Returns its frame |
| `table(x, y, rows, opts?)` | a grid: `rows` an array of arrays of strings, one box per cell and one text per non-empty cell; `cols` (a width, or one per column; default fits the widest text), `rowH` (default 1.6 of the size), `header: true` tints the first row, `parts` (an array of arrays of part names, `null` for none). Returns its frame and `cell(r, c)` |
| `plot(x, y, w, h, opts)` | axes for `opts.x: [min, max]`, `opts.y: [min, max]`: two axis lines, a label per tick in `xticks`/`yticks` (size 16, `muted`), `xlabel`, `ylabel`. Returns `p` with `p.at(x, y)` (the canvas point of a value), `p.line(f or [[x, y], ...], opts?)` (one line; `steps` for a function, default 50), `p.dots([[x, y], ...], opts?)` (filled circles of radius 4). Each `line` or `dots` takes the next series colour unless given one |

**Context.** `fig.name` (the figure's name), `fig.data` (the data, `{}` when
none), `fig.view.target` (`{x: 0, y: 0, w, h}` of the `--over` or `--in`
item, else `null`), `fig.view.page` (the page's name), `fig.color` (assign
to change every later default). `warn(message)` adds a warning.

**Not there:** `Date`, timers, `console`, `import`, `fetch`, files. `Math.random`
is seeded: the same script and data draw the same figure.

**Errors** name the call and the line: `script:12:7: box: w must be a number above 0, got "wide"`.
Limits: 1 s, 32 MB, 5,000 shapes, a 1 MiB script, 256 KiB of data.

## Appendix B: the reference worksheet

The acceptance script (steps 7, 9, 14). With the data below it draws **56
items in 15 parts** (33 texts, 17 boxes, 4 arrows of which 3 are lines,
2 strokes: the plot's curve and the closed `check` circle). The redraw with the second data patch fills
`actual-top-5` and `feedback` by name: 62 items, 6 texts added and the
`check` circle filled (one `set`); every other item is unchanged and the
user's strokes are untouched. The counts were checked against a stand-in of
this API in plain JavaScript; step 7 checks them in QuickJS.

```js
const d = fig.data;
const pct = Math.round(d.rate * 100);
const value = (t) => d.start * (1 + d.rate) ** t;

part('title', () => {
  text(0, 0, d.title, { size: 40 });
  text(0, 56, 'Work in the dashed boxes. The dotted ones are mine.', { color: 'muted' });
});
part('name', () => {
  text(620, 8, 'Name', { color: 'muted' });
  line(690, 32, 900, 32);
});

// 1. Read a graph.
part('q1', () => text(0, 120, `1. ${d.start} is saved at ${pct}% a year, added once a year.`));
part('q1-plot', () => {
  const p = plot(60, 190, 480, 280, {
    x: [0, d.years], y: [0, 250],
    xticks: [0, 2, 4, 6, 8, 10], yticks: [0, 100, 200],
    xlabel: 'years', ylabel: 'balance',
  });
  p.line(value, { steps: 40 });
});
text(600, 190, '1a. About how much after 5 years?', { size: 20, w: 300 });
part('1a-answer', () => box(600, 250, 300, 90, { dash: 'dashed' }));
text(600, 360, '1b. When has it doubled?', { size: 20, w: 300 });
part('1b-answer', () => box(600, 400, 300, 90, { dash: 'dashed' }));

// 2. Fill the table.
part('q2', () => text(0, 540, '2. Fill in the balance.'));
part('q2-table', () => table(0, 580, [
  ['year', ...d.table.map(String)],
  ['balance', value(d.table[0]).toFixed(0), '', '', ''],
], { cols: 150, header: true }));
part('2-working', () => box(0, 680, 900, 120, { dash: 'dashed' }));

// 3. Predict, then compare.
part('q3', () => text(0, 840, `3. Rank these by balance after ${d.years} years.`));
part('3-accounts', () => stack(0, 880, { gap: 4 }, (s) => {
  for (const a of d.accounts) s.text(`${a.name}: ${a.start} at ${a.rate}%`, { size: 20 });
}));
part('3-predicted', () => box(0, 1010, 430, 200, { dash: 'dashed' }));
arrow(436, 1110, 464, 1110);
text(470, 1010, 'Actual top 5', { color: 'muted', size: 20 });
part('actual-top-5', () => {
  box(470, 1040, 430, 170, { dash: 'dotted' });
  if (d.actual) stack(486, 1052, {}, (s) => d.actual.forEach((n, i) => s.text(`${i + 1}. ${n}`, { size: 20 })));
});

// Feedback, written on a later turn.
part('feedback', () => {
  box(0, 1250, 900, 140, { dash: 'dotted' });
  if (d.feedback) text(16, 1266, d.feedback, { w: 868 });
});
part('check', () => circle(880, 1230, 8, { fill: d.feedback ? 'series3' : undefined }));
```

Data for `canvas draw main worksheet.js --name worksheet --at 40,40 --data @worksheet.json`:

```json
{
  "title": "Compound growth", "start": 100, "rate": 0.08, "years": 10,
  "table": [0, 2, 4, 6, 8],
  "accounts": [
    {"name": "A", "start": 100, "rate": 8}, {"name": "B", "start": 150, "rate": 4},
    {"name": "C", "start": 80, "rate": 10}, {"name": "D", "start": 120, "rate": 6},
    {"name": "E", "start": 60, "rate": 12}
  ],
  "actual": null, "feedback": null
}
```

The redraw: `canvas redraw main worksheet --data '{"actual":["C","A","E","D","B"],"feedback":"1a and 2 are right. In 1b, read where the curve crosses 200: about 9 years."}'`.
