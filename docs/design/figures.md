# Figures: an agent draws with code

Status: draft 2026-11-24, owner to confirm. Nothing here is built. Inputs:
D6, D11, D13, D19, K4, B1, B6, B9, the canvas file
(`docs/design/canvas-file.md`), fold (`docs/design/fold.md`), live items
(`docs/design/live-items.md`) and the engine spike below.

> **Note, written overnight 2026-11-24 (not the owner's text).** Steps 2 to
> 13 of plan pl-9f74c2 were built that night from a page rebuilt from the
> plan and its seeds, because this draft had not reached local `main`.
> Every section from here to "Build order", and Appendices A and B, is the
> owner's text, unchanged: it is the authority for decisions, the spike and
> intent. Where the build differs, "Differences between this design and
> what was built on 2026-11-24" says so, one row each. "As built" keeps what
> the rebuilt page added that this draft does not cover (old readers, file
> keys, draw order, names and addresses, the gesture table, paste, packet
> detail, the fluency round, the files to split, the overnight picks), and
> Appendices C and D are the script API and the reference worksheet as
> shipped.

## The gap

An agent answers a sling with one short text beside the ink. The reference
for "better" is a worksheet a tutor agent made in tldraw on 2026-11-03
(`~/sync/learning/canvas/lessons.tldraw`, page `mech-interp 106 logits`): 64
shapes from one script. It had headings, answer areas sized for handwriting
(dashed: yours; dotted: the agent fills it in), a bar chart whose heights came
from the numbers, and feedback that landed in the area kept for it. The user
wrote 1,023 strokes on it.

The worksheet is one example, not the scope. The scope is: an agent makes
any structured, computed picture on the canvas (a chart, a diagram, a table,
a timeline, marks on the user's own ink), the user draws on it, and the agent
changes it later without redrawing it by hand.

Slinger today cannot do this for three reasons:

1. No fill, no dash, no lock and no plain line (D6).
2. `canvas apply` is a log of changes in page coordinates, not a thing to
   draw with. Every agent would write its own drawing library over it.
3. What an agent draws is loose items. Nothing says "these 64 are one
   worksheet" or "this box is answer (a)".

## Proposed, in short

1. **A drawing API that slinger owns.** An agent writes one JavaScript file
   against it. Slinger runs the file in a sandbox and makes the items.
2. **A figure.** What one script draws is one named thing with named parts.
   It has its own coordinates; slinger places it.
3. **A figure is its script and its data.** To change a figure, change its
   data; slinger runs the script again and changes only what differs.
4. **Four new item fields:** `fill`, `dash`, `locked` and a line with no
   head. These amend D6.

## Where the code runs (B6: the alternatives)

**A. Slinger runs the script (picked).** One API, versioned with the binary,
tested here. The run is a pure function: script, data and a read-only view
of the canvas in; a list of shapes out. The same core operations as
`canvas apply` put them on the page, so there is still one way to change a
canvas.

**B. The agent runs its own script and pipes operations to `canvas apply`.**
Works today with no new code. Rejected: slinger would own no drawing API,
each agent's helpers would differ, text could not be measured before it is
placed, and the result would be loose items (reason 3 above).

**C. A layout tree (rows and columns) that the core resolves.** Rejected by
the owner: it only stacks. Axes, vectors and diagrams need free coordinates.
What it was good for (stack, grid, table) is in A as helpers.

**D. HTML in a live item.** Exists (O1). It is the right tool for a control
(a picker, a slider). It is the wrong one here: the user cannot write on it
with the pen, the agent cannot change one part of it, and its content is not
in the packet.

## The engine spike (B9: the code is outside the repository)

One harness for each engine ran the same prelude and the same scripts: the
worksheet (56 items) and a harder page (a plot of three curves with ticks, a
4-layer network of 10 circles and 23 arrows, a table: 78 items). Release
build with this workspace's profile (fat LTO, strip), rustc 1.99, Apple
silicon.

| | QuickJS (rquickjs 0.14, quickjs-ng 0.16.2) | Boa 0.22 | Starlark 0.14 |
|---|---|---|---|
| Language | JavaScript | JavaScript | a Python subset |
| Code | C, one vendored library | Rust | Rust |
| Crates added | 8 | 175 | 182 |
| Clean build | 11 s | 91 s | 42 s |
| Binary over an empty one | 1.07 MB | 7.68 MB | 4.68 MB |
| Worksheet, one run (new engine each run, mean of 200) | 0.60 ms | 2.22 ms | did not finish |
| `while (true) {}` | stopped at the 1 s deadline | stopped at a loop count | stopped at a tick count |
| Fill memory forever | "out of memory" at the 32 MB limit, 31 ms | **no limit: ran until killed** | stopped at the heap limit |
| Endless recursion | error | error | error |
| A thrown error names line and column | yes, with the call stack | yes | yes, with the source line |
| A syntax error names line and column | yes | yes | yes (the line after) |

(`slingerd` is 4.4 MB today.)

- QuickJS and Boa gave byte-identical output for all three scripts,
  including a script of modern syntax (classes, private fields, generators,
  spread, optional chaining, BigInt, named groups, `toSorted`).
- Neither has `fetch`, `require`, `process`, `setTimeout` or `console` unless
  the host adds it. Both have `Date` and `Math.random`; the host removes them.
- Starlark failed the worksheet three times on what it lacks: no `math`
  module (no `exp`), no `sum`, no `"%.3f"`. It has no classes and no
  closures over a changing value. An agent writes Python well and Starlark
  badly.
- Boa 0.22's limits are loop count, recursion depth and stack size. It has
  no memory limit, so one script can take the daemon down.

**Pick: QuickJS.** It is the only one with a time limit, a memory limit and
a stack limit that all held, it is the smallest and the fastest, and agents
already write its language.

**Its cost:** C code. The core stays pure (`core-is-pure`), so the engine
goes in a new crate, `crates/script`, that only the daemon links. Nothing
else names the engine (a layer rule), so it can be replaced. A build needs a
C compiler; `rusqlite` (bundled SQLite) already needs one. The crates and
QuickJS are MIT, which `deny.toml` allows. Built only on
`aarch64-apple-darwin` in the spike; the other three targets in `deny.toml`
are not checked.

### Can a small model use the API first time?

Three fresh agents on the smallest model (claude-haiku-5-5) each got only
the reference in the appendix and one task the reference has no example of:
a timeline from data, a vector-addition diagram with a table and an answer
area, and a kanban board from data. They could not run anything.

| Task | Script | First run |
|---|---|---|
| Timeline (6 events, alternating sides) | 51 lines, 31 items | ran, right |
| Kanban (3 columns, cards fit their text) | 42 lines, 18 items | ran, right |
| Vector addition | 57 lines, 59 items | failed: it called `fig.arrow(x1, y1, x2, y2)` as `fig.line` takes, and `arrow` took two points |

What the failure changed in the API, not in the script:

- Calls that draw the same kind of thing take the same arguments. `arrow`
  now takes four numbers as `line` does, or two points or handles. The
  script then ran unchanged and was right.
- A bad argument fails at the call, by name: `fig.line: a point must be
  [x, y], got ["ten",5]`, with the script's line. The first failure
  surfaced at the end of the run, far from its cause.
- A label on an arrow sat on the arrow. Label placement must clear the line,
  and "text over a line" is a warning. A dry run would have shown it.

## The model

### New item fields (amend D6)

| Field | On | Values |
|---|---|---|
| `fill` | box, closed stroke | a colour; missing: none |
| `dash` | box, stroke, arrow | `dashed`, `dotted`; missing: solid |
| `heads` | arrow | `end` (default), `both`, `none` (a plain line) |
| `closed` | stroke | the last point joins the first |
| `locked` | any | the pen, the eraser, the lasso and a tap pass over it |

- **Fills draw under everything but PDF pages.** A fill never hides ink or
  text, whatever was made first. No z-order to manage.
- **A circle is a closed stroke.** No new kind for circles, ellipses or
  polygons.
- **Locked is for the user's hands, not for agents.** The author rule still
  decides what an agent may change. The user unlocks a figure with a command.
- All five are optional fields on known kinds, so an older reader keeps the
  items (fold.md, "What an old reader does today", rules 3 and 6; rule 5's
  loss applies as it does to `fold`).

### Figure and part

Two more optional fields on any item: `fig` (the figure's name, unique on
a canvas) and `part` (a name the script chose).

- A figure is the set of items with one `fig`. No item's existence depends
  on another item, and none leaves the page's item list.
- A part is the items of a figure with one `part` name. A part is an
  address: `softmax/p1a` works wherever an item id does (`--beside`, `--over`,
  `canvas crop --item`, an arrow's end).
- The figure's script (a blob, D19) and data are in the record, in a
  `figure_drawn` event: `{canvas, figure, script, data, origin}`. The canvas
  file holds only items.
- Not a frame: the user does not make, name or enter a figure (the owner
  rejected named frames in pl-f130a2). A figure can sit in a box and fold
  with it.

Alternative: a `figure` item that lists its children's ids, as a folded
box does. Rejected: the children's meaning would depend on another item, and
the list must be kept right on every add, remove and undo. Tags need no
upkeep.

### In the packet

- The outline has one line for a figure, not one per item:
  `f3 figure softmax 0,0 1700x3800 parts=15 by=teacher`.
- Ink over a part says so, as ink over a PDF page does:
  `c12 ink … over=softmax/p1a`. The agent learns which answer area the
  user wrote in without looking at coordinates.
- `canvas read --in softmax/p1a` gives only what lies in that part.

## The script API (v1)

Full reference with an example: the spike's `API.md` becomes a section of
`slinger guide`. The rules that shape it:

1. **It is the API agents already know.** Call shapes follow Canvas 2D and
   p5: `fig.text(str, x, y)`, `fig.box(x, y, w, h)`, `fig.line(x1, y1, x2, y2)`,
   `fig.circle(cx, cy, r)`, `fig.path(points)`, `fig.arrow(from, to)`. Options
   are one object, last: `{color, fill, dash, width, name, locked}`.
2. **Figure coordinates only.** (0, 0) is the figure's top left, in the
   units a packet prints. The script never sees the page.
3. **Build, measure, place.** Every call returns a handle with exact bounds
   (`x y w h right bottom cx cy`) and moves (`moveTo`, `below`, `rightOf`,
   `above`, `leftOf`, `centerIn`). Text is a fixed cell, so `measure` is
   exact (it calls `render::text_frame`). Nothing is on the canvas until the
   script ends.
4. **Free coordinates first, helpers second.** `stack`, `grid`, `table`,
   `plot` (data coordinates, bars, curves, vectors) and `slot` are written
   in JavaScript over the primitives, in a prelude that ships in the binary.
   A helper is never the only way to do a thing. New helpers need no engine
   change.
5. **Pure.** Inputs: the script, `data` (JSON from `--data`), `anchor`
   (the size and text of the item it goes beside or over) and, with
   `--over` or `--at`, the bounds of the items around it in figure
   coordinates. No clock, no random numbers without a seed, no IO. The same
   inputs give the same figure.
6. **Bounded.** 1 s, 32 MB, a 512 KB stack, a script of at most 1 MiB (as a
   UI), at most 5,000 items. Over a limit is one error, `limit`, that says
   which.
7. **One shape of call.** Calls that draw the same kind of thing take the
   same arguments (`arrow` as `line`). A bad argument fails at the call and
   names it.
8. **Errors point at the line.** `script`: the message, line and column,
   the source line, the call stack. Warnings do not fail the run: text
   taller than its box, a word wider than its frame, two named parts that
   overlap.

## The verbs

```bash
slinger canvas draw <CANVAS> <FILE> --name softmax --beside c12 [--data JSON|@FILE]
slinger canvas draw <CANVAS> <FILE> --name marks --over c12     # the figure's (0,0) is c12's top left
slinger canvas draw <CANVAS> <FILE> --name plan --at 0,0 | --in b4
slinger canvas draw <CANVAS> <FILE> --dry-run --png out.png     # draws nothing; writes the picture and the warnings
slinger canvas redraw <CANVAS> softmax --data '{"p1a":{"actual":[…]}}'   # merge into its data, run its script again
slinger canvas figure <CANVAS> softmax [--script] [--data]      # its parts and boxes; its script; its data
```

- `draw` with a name the canvas has replaces that figure in place, as one
  undo step. Items are matched by part and order: one that did not change
  keeps its id and is not touched, so a second run of the same script and
  data changes nothing and the next packet shows nothing new.
- A redraw never touches the user's ink or another agent's items.
- `--beside` places the whole figure by `free_beside` on its bounds.
- `--dry-run --png` lets the agent look at its own picture before the user
  sees it (the core's renderer, no app).
- Removal and moves use the verbs that exist, with `softmax` or
  `softmax/p1a` as the id.

## What this allows beyond the worksheet

- **Change by data.** "Fill in the actual top 5" is
  `canvas redraw … --data`, not a new script. A worked example shown one
  step at a time is `{"step": 3}`. A chart follows a result as it changes.
- **Marks on the user's own work** (`--over`): circle the wrong term,
  an arrow from one of their clusters to another, a bracket in a PDF's
  margin, a light fill behind a sentence. The tutor could only put notes in
  free space.
- **The agent checks itself.** A dry run gives the picture and the warnings.
- **Parts as places.** The packet says which part new ink is over, and a
  `canvas put --beside softmax/p1b` lands feedback at that answer.
- **Scripts worth keeping.** A script is a blob with a hash. Later, with no
  redesign: named, versioned scripts as UIs have (`timeline@2`), run with
  new data.
- **Later, with no redesign: a figure that follows the user.** A live item's
  reply (a slider), or a text the user edits, patches a figure's data and
  slinger redraws it with no agent turn. The run is pure and bounded, so it
  is safe to run on a user's action.
- **The tldraw importer** can keep fill and dash, so the old lessons board
  comes in as it looked.

## Not in this design

Images, rotation, rich text (bold, sub- and superscript, maths), curves
other than polylines, snapping. Each is a separate decision.

## Decided by the owner (2026-11-24)

1. **Engine.** QuickJS, in process, in its own crate that only the daemon
   links.
2. **Names.** figure, part, `canvas draw`, `canvas redraw`,
   `canvas figure`.
3. **Lock.** What a script draws is locked by default. The user moves a
   figure by grabbing it as one: an "unlock figure" command (a key and a menu
   row) on the figure under the pointer selects the whole figure as one unit,
   to move, duplicate or delete. It is locked again when deselected. There is
   no unlocking of single items and no stored lock toggle, so a scaffold
   never falls apart.
4. **Units.** A script uses whole units, as the packet prints them. The other
   verbs keep tenths.
5. **Colour.** A figure with no colour named takes one agent colour (a config
   key in the theme). Colour is also part of what an agent says: any call,
   group or part can take its own colour, so one figure can hold several.
   See "Colour" below.
6. **Fills.** Always under all ink and text, above a PDF page. Nothing raises
   one.

### Colour

- The default is the agent colour, so the user tells a scaffold from their
  own ink at a glance.
- Every draw call takes `color` and `fill` (as before). A group, a stack,
  a table, a plot and `fig.part(name, {color}, () => { ... })` set a
  default for what they hold; `fig.color = ...` sets it for the rest of the
  script. The nearest one wins.
- Decided: a small set of named colours beside hex (for example `'accent'`,
  `'muted'`, `'good'`, `'bad'`, `'warn'`, and a short series for plots)
  that the theme maps, so a figure reads in any theme and agents share one
  grammar. Hex stays for anything else. The design step (slinger-e8d6e3)
  picks the exact names and, designed twice, whether a name is stored on the
  item or resolved to hex when the script runs.

## Build order

1. Core, file, render, app: `fill`, `dash`, `heads`, `closed`.
2. Core and app: `locked`, and the unlock command.
3. Core: `fig` and `part`, part ids, the packet lines, `read --in`.
4. `crates/script`: the engine, the prelude, the limits, the errors, the
   layer rule. Tests run real scripts.
5. `canvas draw` through control, daemon and CLI: placement, replace by
   match, `--dry-run --png`, the `figure_drawn` event.
6. `canvas redraw` and `canvas figure`.
7. `guide.md` and both skills: when to draw a figure and when to put a
   text. `scripts/e2e-sling.sh` gains a sling that must come back as a
   figure.
8. The tldraw importer keeps fill and dash.

Acceptance: one script makes the 106 worksheet; `canvas redraw` fills
"actual top 5" by name; pen strokes over it are untouched; a second run of
the same script changes nothing.

## Differences between this design and what was built on 2026-11-24

Written overnight 2026-11-24 (not the owner's text), comparing the sections
above with the code on `main`, not only with the rebuilt page. Ordered by
weight. Size: **small** (a few lines, no contract), **medium** (real code,
additive), **contract** (changes what agents write or read: the script API,
the packet, an error code). What matches, and needs no row: the engine and
its crate and layer rule; the limits (1 s, 32 MB, 512 KB stack, 1 MiB
script, 5,000 items); the five style fields plus `fig` and `part` and
their values; a circle as a closed stroke; fills under all ink and text,
above PDF pages; locked by default and for the user's hands only, with the
author rule unchanged; the unlock command (opt+cmd+U, the Edit menu and the
context menu) that grabs the figure under the pointer whole and relocks on
deselection, with no single-item unlock in the app; part addresses wherever
an id is taken (`--beside`, `--over`, `--in`, `crop --item`, an arrow's
end in `put`, edit and remove); `over=figure/part` on ink; `read --in` a
part; `canvas draw`, `redraw` (a merge into the data) and `figure`;
`--dry-run --png`; replace in place by part and order, one undo step, a
rerun that changes nothing; `figure_drawn` with the script as a blob (it
also carries `page`, `frame` and an optional `target`); a seeded
`Math.random` and no clock or IO; the agent colour as a theme key; the
tldraw importer keeping fill and dash (and lock, closed, heads).

| # | The owner's decision | What was built | Where | How hard |
|---|---|---|---|---|
| 5 | Errors: a `script` error with the message, line, column, the source line and the call stack; over a limit is one error, `limit`, that says which | Every script failure and every limit is `invalid` with `script:L:C: message` and a hint; no source line or stack (lane c adds the error line tonight) | `crates/daemon/src/server/figure.rs` (`run`), `crates/script/src/engine.rs`, docs/service.md, docs/cli.md | contract |
| 6 | Packet: `f3 figure softmax 0,0 1700x3800 parts=15 by=teacher` (an id of its own, the name, a part count) | `softmax figure 0,0 1700x3800 by=teacher parts=p1a,p1b,... locked=yes "first line"` (the name is the id, the part names listed, a quote) | `crates/core/src/packet/figure.rs`, docs/design/packet.md | contract |
| 9 | Rule 5, inputs: `anchor` (the size and text of the item it goes beside or over) and, with `--over` or `--at`, the bounds of the items around it in figure coordinates | `fig.view.target` (`{x: 0, y: 0, w, h}` of the `--over` or `--in` item, else `null`) and `fig.view.page`; no text, nothing for `--beside`, no neighbours | `crates/script/src/lib.rs` (`View`), `crates/daemon/src/canvases/figure.rs`, `server/figure.rs` | medium |
| 13 | Decision 4: a script uses whole units; the other verbs keep tenths | `canvas draw --at` takes canvas units (`40,40`, `PAGE:X,Y`) while `canvas put --at` takes tenths (the wire is tenths for both) | `crates/cli/src/draw.rs`, guide.md | small |
| 15 | `fill`: a colour ("use a light or translucent colour") | A name or `#rrggbb` is stored at 20% alpha (`33`), `true` is a tint of the shape's colour, `#rrggbbaa` as given; every renderer caps a fill at 40% (`FILL_MAX_ALPHA`) | `prelude/base.js` (`tint`), `crates/core/src/canvas/style.rs`, the app's fill pass | small |
| 17 | Decision 3: no stored lock toggle | The app has none, but `locked` is an ordinary field: an agent may `set` it on its own items and the user on any, through `canvas apply` (undo, import and paste need it written) | `crates/core/src/canvas/file/ops.rs` | small |

Closed by slinger-bee990 (the script API ported to the owner's design;
rows keep the numbers they were written with): 1 (handles), 2
(`fig.text(str, x, y)`, `fig.path`), 3 (`fig.plot` with `xRange`, `yRange`,
`ticks`, `frame`, `p.x`, `p.y`, `p.bars`, `p.fn`, `p.vector`, `p.bounds`), 4
(`fig.stack`, `fig.grid`, `fig.table(rows, x, y)`, `fig.group(h1, h2, ...)`), 7
(`fig.ellipse`, `fig.slot`, a box's `label` and `size`, a text's `align`), 8
(`name` on any call, `h.name()`, `fig.part(name, {color}, fn)`), 10 (the owner's
three warnings, beside the ones built overnight), 11 (Appendix B runs unchanged
as `crates/script/src/tests/softmax.js`: 53 items in its 15 parts, not the 56
its heading says, see below; the redraw acceptance stays on Appendix D, since
Appendix B's script reads no data), 12 (the global `data`, `null` when none),
14 (`width` on lines, paths and arrows; an arrow with heads stays 2 wide, a line
of another width is a two-point stroke) and 16 (`good`, `bad`, `warn`; `error`
is gone). Row 9's `anchor` and nearby bounds wait for the View that
slinger-fad0bb adds.

Appendix B's count: as the API reads, the script draws 35 texts and 18 boxes
(4 headings, 5 paragraphs, 8 answer boxes with 7 labels, 4 plot titles, 4
dotted plot frames, 3 bars with 3 labels, 9 hand-drawn bar labels, 3 "Why"
boxes with their labels). The spike's prelude, which is not in the
repository, drew 3 more; nothing in Appendix A says what (a value over each
of the 3 bars would make 56).

Count left: 2 contract (5, 6), 1 medium (9), 3 small (13, 15, 17).

## As built (written overnight 2026-11-24, not the owner's text)

The sections below are the page that was rebuilt overnight from the plan and
its seeds, with its headings one level down. They describe what was built.
Where they disagree with the owner's text above, the owner's text is the
design and the row in "Differences" is the gap.

### Owners

| Step | Seed | Owns |
|---|---|---|
| 1 | slinger-e8d6e3 | this page, SPEC D6 and section 4, the "Figures" open picks |
| 2 | slinger-c0c4c2 | core: `fill`, `dash`, `heads`, `closed`; file, ops, show, raster |
| 3 | slinger-450877 | app: draw them; the fill pass; the theme's `agent` key |
| 4 | slinger-097d16 | core: `locked`, `fig`, `part`; the address; `Canvas::figures` |
| 5 | slinger-dd5add | app: the gesture table; `unlock-figure`; paste and duplicate |
| 6 | slinger-b860be | core: the figure line in the packet, `over=`, `read --in` a part |
| 7 | slinger-3ce137 | `crates/script`: the engine, the prelude (Appendix C), limits |
| 8 | slinger-387b76 | core: place shapes as a figure; replace by match; `figure_drawn` |
| 9 | slinger-14a8ab | `canvas draw` through control, daemon, CLI; `--dry-run --png` |
| 10 | slinger-2ee5da | `canvas redraw`, `canvas figure` |
| 11 | slinger-9af735 | guide, both skills, the sling note, the e2e sling |
| 12 | slinger-b0b24a | the fluency round; records it under "Fluency" here |
| 13 | slinger-3ca75e | tldraw import keeps fill, dash, isLocked |
| 14 | slinger-9dcf50 | acceptance in a driven window |

### What an old reader does with new fields

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

### The model (B6: where the style and the membership live)

#### A: optional fields on the item object (picked)

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

#### B: new kinds, or a style table

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

#### A headless line: an arrow with `heads: none` (picked) or a two-point stroke

- **Arrow, `heads: none`.** Its ends bind to items as an arrow's do, so a
  connector follows what it joins; it draws at the box edge's width; it is
  an `arrow` line in the packet (with `from`/`to`). An old reader draws a head
  on it: a wrong look, nothing lost.
- **Two-point stroke.** An old reader draws it right; but it cannot bind,
  and the packet treats an agent's stroke as ink (one `ink` line with
  `by`), so a table's rule lines would read as ink.

**Pick: the arrow.** `line()` in the script API makes one. A stroke stays
what a pen (or `poly`) draws.

### Draw order

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

### Colour

#### The names (picked)

A script, and only a script, takes a colour as hex (`#rrggbb`,
`#rrggbbaa`) or as one of these names. The canvas file keeps hex only.

| name | canvas value | what it is |
|---|---|---|
| `agent` | `#8a5cf5` | the agent colour; every shape's default |
| `ink` | `#000000` | the user's default ink (the app shows it light on dark, as any black) |
| `muted` | `#8c8c8c` | secondary text, tick labels |
| `accent` | `#4c9aff` | the theme's accent (`focus`) |
| `good` | `#2f9e44` | right, done (slinger-bee990) |
| `bad` | `#e5484d` | wrong, missing (was `error`) |
| `warn` | `#c77700` | check this, nearly |
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

#### Stored on the item, or resolved to hex at run time (B6)

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

#### A fill in the dark theme (B6)

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

#### Defaults that nest

Each shape's colour, and its `dash`, `fill` and text `size`, is the first of:
the shape's own option, then the innermost enclosing `group`, `stack`,
`table`, `plot` or `part` that sets it (they nest in the order the script
calls them; the nearest wins), then `fig.color` (colour only, starts as the
agent colour), then the built-in default (solid, no fill, size 24). A
plot's series colour is the plot's own default for its lines, not for its
axes, which take the plot's colour.

### Figure and part

#### Names

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

#### The address

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

#### A clash

- `canvas draw --name N` when N exists on the canvas: by its maker, it is a
  redraw with the new script and data (replace by match, below); by anyone
  else, `conflict` (exit 4): "figure N is agent:other's", hint: "pick another
  --name". The user (no `--agent`) may draw over a figure of any maker, as
  the user may change any item.
- A redraw on another page moves the whole figure there (drawn anew, new
  ids). When some of its items are another maker's (the user drew over the
  agent's figure), the agent may not move those, so its draw on another
  page is `conflict`: "figure N has user's items on page P", hint: "draw it
  on page P, or pick another --name". No draw leaves one name on two pages.
  Applied ops still do not check names (above).
- A whole figure the app copies, duplicates or pastes takes a free name: the
  name with its trailing `-<n>` taken off, then `-2`, `-3`, ... the first not
  on the canvas (`worksheet` gives `worksheet-2`). Its parts and its lock stay;
  its maker is the user (the user made the copy), so the agent cannot
  redraw the copy, only the original.

### Lock

#### The unlock (designed from the owner's pick)

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

#### The gesture table

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

#### Paste and duplicate

- A paste or duplicate of items that are **not a whole figure** drops `fig`,
  `part` and `locked`: the copies are the user's free items. (With the grab
  being whole, this is a clip from elsewhere: an agent's `canvas show`
  objects, another canvas's partial clip.)
- A paste or duplicate of **a whole figure** (every object of the clip with
  a `fig` has the same one, and the clip holds every item the figure had
  when copied, which `Mirror::copied` records as a count in the clip) keeps
  `part` and `locked` and takes a free name, free on the whole canvas: the
  app reads the other pages' names from `canvas_show`'s `pages` (each with
  its `figures`) and every `fig` the change feed writes. The rule is one function in
  `crates/app/src/mirror/clip.rs` beside `pasted`, which today keeps unknown
  keys.
- Styles (`fill`, `dash`, `heads`, `closed`) are kept in both cases: they
  are the look, not the membership.

### In the packet

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

### The script API

The full reference is Appendix C: it is `crates/cli/src/guide.md`'s
"Drawing scripts". Since slinger-bee990 it is the owner's Appendix A: the
draw calls are methods of `fig` (`text`, `box`, `slot`, `line`, `path`,
`circle`, `ellipse`, `arrow`, `group`, `stack`, `grid`, `table`, `plot`,
`part`), each returning a handle with bounds and moves; the globals are
`fig`, `data`, `measure` and `warn`. Units are canvas units, y down,
origin the figure's own top left before placement. Nothing leaves the
engine until the script ends: `crates/script/src/prelude/handle.js` keeps
every shape (a `Leaf`) and every `Group`, a move changes them, and
`checks.js` `finish` resolves each shape's style (its own, then the
nearest group that holds it, then the `fig.part` it was drawn in, then
`fig.color` as it was when drawn) and emits the shapes in the order made.

#### Limits and purity (step 7)

- 1 s of run time (an interrupt handler), 32 MB of memory, 512 KB of stack,
  a script of at most 1 MiB, data of at most 256 KiB as JSON, at most 5,000
  items. Each limit fails with its own message and the line:col where the
  script was.
- No `Date`, timers, `console`, `import`/`require`, `eval` of strings, or IO.
  `Math.random` is a seeded generator (seed 1 each run), so the same script
  and data give the same bytes out, which replace by match relies on.
- Errors keep the script's line and column: `script:12:7: fig.box: w must be a number above 0, got "wide"`.
  Each API call reads its site as it starts (`entry` in `base.js`) and an
  error it throws leads its stack with that site, so a call deep in a plot
  or a table still names the script's line.
  A bad argument fails at the call, by the function's name. A run the
  clock stopped names a line the loop is at or after, and says so
  (`script:3:1: the script ran longer than 1 s, stopped at this line or in
  a loop after it`): QuickJS checks the clock at a call, where the stack
  names the call, and at a loop's jump back, where it names the last
  position recorded before the loop's head (a line before the loop, or the
  first line of its function). Which check fires moves with any work run
  before, the loop's own calls included, so no loop is named inside its
  body every time (slinger-71c6cb). Warnings
  (the owner's three: a label taller than its box, a word wider than its
  text's frame, two named parts that overlap; and text over a line, texts
  overlapping, a box drawn again on another's frame, a label longer than
  its arrow, a shape far outside the others) come back beside the shapes,
  with their line, and never fail a draw.

#### How shapes cross to the daemon (B6)

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

### The verbs

#### `canvas draw` (step 9)

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

#### `figure_drawn`

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

#### `canvas redraw` (step 10)

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

#### Replace by match (step 8, B6)

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

#### `canvas figure` (step 10)

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

### Fluency

Step 12 (slinger-b0b24a) ran its round on 2026-11-24 with `claude-haiku-5-5`
(Claude Code subagents, model "haiku"; 7 runs of about 27k tokens each, no
tools used). Each run got only the guide's drawing text, as `slinger guide`
prints it (the `canvas draw` paragraph, "Drawing scripts", which is
Appendix C, and the command's usage line), plus one task and the command line
that would run it. Each script ran once through
`canvas draw --dry-run --png` on a scratch daemon whose canvas held a text
(t1), an answer box (b2, 300x150) and a second text (t3).

The guide had no API before this step: `canvas draw --help` sent agents to
`slinger guide` for it, so the round's first change was to put Appendix C
there, as "Drawing scripts".

| task | placement | first run | read in the PNG | fix |
|---|---|---|---|---|
| state diagram: five states, five labelled arrows, a part per state | `--beside t3` | ran | "cancel" and "refund" on top of each other where two arrows meet one box (warned: overlaps) | arrow labels move along the arrow or to its other side to clear texts and lines drawn before them; a fresh run read cleanly |
| plot from data: six months of sales, a dashed target, named month ticks, a title | `--beside b2`, `--data @FILE` | ran | the hand-made title covered the y label (warned: overlaps); month names drawn by hand as texts; `p.at` guessed as `[x, y]` or `{x, y}` | ticks take `[value, "label"]`; `plot` takes `title`, set above the y label; the guide says `p.at` returns `[x, y]`; a fresh run used all three and read cleanly |
| vector addition: a table with empty cells, an answer box, three parts | `--at 600,40` | ran | correct; the two empty cells drawn twice (a table box, then the part's own box) | later (slinger-caafca): a box with no fill on the frame of an earlier box warns, naming the table's line, and the guide says a named cell is boxed already; an arrow's label longer than the arrow warns |
| kanban board from data: three columns, filled cards, a part per column | `--beside t1` | ran | correct | none |
| marks over an item: dashed outline, ruled lines, a label and a tick, fitted to `fig.view.target` | `--over b2` | ran | correct, fitted to the box's size | none |

Result: five of five ran first time; after the two fixes, five of five read
correctly (the last three tasks from the first runs, the first two from
fresh runs on the fixed guide). No run hit an error, so no error text
changed. What small models reached for that the API lacked: a plot title
and named ticks (both added); nobody guessed a wrong option name.

### Files to split first (measured 2026-11-24, limit 500)

Plan risk 3. A step adds to none of these before it moves code out (or puts
its code in the new files this page names). Lines at the time of writing:

| step | near the limit |
|---|---|
| 2, 4 (core model) | `crates/core/src/canvas/item.rs` 457, `crates/core/src/canvas/file/json.rs` 447, `crates/core/src/canvas/file/mod.rs` 480, `crates/core/src/canvas/file/log.rs` 484; tests: `crates/core/tests/canvas_fold.rs` 496, `crates/core/tests/canvas_file.rs` 435 (new tests in a new `crates/core/tests/canvas_style.rs` and `canvas_figure.rs`) |
| 3 (app draw) | `crates/app/src/scene.rs` 497, `crates/app/src/scene/canvas.rs` 480, `crates/app/src/mirror.rs` 458, `crates/app/src/platform/canvas.rs` 454, `crates/app/src/platform/mod.rs` 454, `crates/app/src/scene/ink.rs` 453, `crates/app/src/mirror/tests.rs` 495 |
| 5 (app lock) | `crates/app/src/scene/shapes.rs` 478, `crates/app/src/scene/shapes/tests.rs` 492, `crates/app/src/scene/tools.rs` 472, `crates/app/src/scene/tools/tests.rs` 492, `crates/app/src/scene/select.rs` 440, `crates/app/src/window.rs` 482, `crates/app/src/tests.rs` 491, `crates/app/src/config.rs` 496 (the `agent` key goes in `config/theme.rs`), `crates/app/src/scene/slung.rs` 500 |
| 8 (place) | `crates/core/src/canvas/file/log.rs` 484 (the code goes in `file/figure.rs`) |
| 9, 10 (verbs) | `crates/daemon/src/tests.rs` 485, `crates/daemon/src/store.rs` 490, `crates/cli/src/tests.rs` 486 (tests in new files) |

### Recommended (picked overnight, owner to confirm)

Since the owner's draft came in: picks 1 and 2, the fill order in 3, the
address in 6, the unlock command in 7, the `over=` and `read --in` of 9,
the match by part and order in 11 and pick 13 (a seeded `Math.random`) are
the owner's decisions above. The rest stay open in `docs/open-picks.md`;
where the build differs from the owner, see "Differences".

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
4. **Colour names:** `agent` `#8a5cf5`, `ink`, `muted`, `accent`, `good`,
   `bad`, `warn`, `series1`..`series6` and the nine packet names; resolved to hex at run
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

## Appendix A: the reference the agents were given

This becomes a section of `slinger guide`. Headings are one level down here.

### Drawing on a slinger canvas with a script

You write one JavaScript file. Slinger runs it in a sandbox and turns what it
draws into ordinary canvas items, placed as one figure.

- Coordinates are canvas units in the figure's own frame: (0, 0) is its top
  left, x grows right, y grows down. Slinger decides where the figure goes on
  the page. Never assume page coordinates.
- Text is monospace: a character is `size` tall and `size / 2` wide. So text
  size is exact: `measure('hello', {size: 24})` is `{w: 60, h: 24}`.
- Plain modern JavaScript (ES2023). No imports, no network, no files, no
  timers, no `Date`, no `console`. Top-level code runs once.
- The global `data` holds the JSON passed with the script (or `null`).

#### Draw

Every call returns a handle. The last argument is always an options object.

```js
fig.text(str, x, y, {size = 24, w, align = 'left'})   // w wraps the text to that width; align: 'left' | 'center' | 'right' (x is then the centre or the right edge)
fig.box(x, y, w, h, {label, size = 20})               // label: text inside the box, top left, wrapped
fig.slot(x, y, w, h, {label, by = 'user'})            // a box meant to be filled in: dashed for the user, dotted when by: 'agent'
fig.line(x1, y1, x2, y2)
fig.path([[x, y], ...], {closed = false})             // a polyline; closed joins the last point to the first
fig.circle(cx, cy, r)
fig.ellipse(cx, cy, rx, ry)
fig.arrow(x1, y1, x2, y2, {heads = 'end', label})     // as line; heads: 'end' | 'both' | 'none'
fig.arrow(from, to, {heads, label})                   // from, to: [x, y] or a handle (the arrow then meets its edge)
```

Options every call takes:

- `color`: `'#rrggbb'` or `'#rrggbbaa'`. Default: the colour of the group or part it is in, else the figure's colour (the agent colour).
- `fill`: a colour to fill a box, circle, ellipse or closed path. Fills draw under all ink and text, so they never hide writing. Use a light or translucent colour.
- `dash`: `'dashed'` or `'dotted'` (boxes, lines, paths).
- `width`: line width (lines, paths, arrows; default 2).
- `name`: names this part of the figure, such as `'answer-a'`. A later command can find it by name. Name every area that will be written in or updated.
- `locked`: default `true`: the user's pen and eraser pass over it. Pass `false` for something the user should be able to move.

#### Handles

```js
h.x  h.y  h.w  h.h  h.right  h.bottom  h.cx  h.cy     // its bounds, read-only
h.moveTo(x, y)   h.moveBy(dx, dy)
h.below(other, gap = 24)   h.above(other, gap)   h.rightOf(other, gap)   h.leftOf(other, gap)
h.centerIn(other)
h.name('part-name')
```

Nothing is on the canvas until the script ends, so build a thing, read its
size, then place it.

#### Helpers

```js
measure(str, {size = 24, w})                 // {w, h} of text, wrapped to w when given
fig.group(h1, h2, ...)                       // one handle over several
fig.stack([h1, h2, ...], {dir = 'down', gap = 24})   // dir: 'down' | 'right'; moves each after the one before; returns a group
fig.grid([h1, ...], {cols = 3, gap = 24, x = 0, y = 0})
fig.table(rows, x, y, {size = 20, header = true, widths})   // rows: arrays of cells; columns fit their longest cell
const p = fig.plot(x, y, w, h, {xRange: [0, 1], yRange: [0, 1], ticks = 0, frame = 'dotted'})
p.x(v)  p.y(v)                               // data value -> figure coordinate (y is up in data)
p.bars(values, {labels})                     // one bar per value across the plot
p.fn((x) => y, {color, width})               // a curve
p.vector([x1, y1], [x2, y2], {label})        // an arrow in data coordinates
```

#### Example

```js
const title = fig.text('Shares of [2, 1, 0]', 0, 0, { size: 32 });
const e = [2, 1, 0].map(Math.exp), sum = e.reduce((a, b) => a + b);
const p = fig.plot(0, title.bottom + 40, 380, 300, { yRange: [0, 1] });
p.bars(e.map((v) => v / sum), { labels: ['A', 'B', 'C'] });
fig.slot(420, title.bottom + 40, 500, 300, { label: 'Why is A more than twice B?', name: 'why' });
```

## Appendix B: the 106 worksheet as one script

Ran on both JavaScript engines in the spike: 56 items, 15 named parts.

```js
// The teacher's 106 worksheet, as one script.
const W = 1700, COL = 820, GAP = 60;
let y = 0;
const heading = (s, size = 32) => { const t = fig.text(s, 0, y, { size, w: W }); y = t.bottom + 12; return t; };
const para = (s) => { const t = fig.text(s, 0, y, { size: 20, w: W }); y = t.bottom + 24; return t; };
const yours = (x, w, h, label, name) => fig.box(x, y, w, h, { dash: 'dashed', label, name, locked: true });
const mine = (x, w, h, label, name) => fig.box(x, y, w, h, { dash: 'dotted', label, name });

heading('106: Logits, softmax, loss (GPT-2 small)', 48);
para('Predict first, in the dashed boxes. Then sling the page: I put GPT-2\'s actual output next to your prediction, and you write what surprised you.');

heading('Problem 1: what GPT-2 thinks comes next');
para('Prompt: "The Eiffel Tower is in the city of" ___\nGPT-2 gives every one of its 50,257 tokens a logit (a raw score). Softmax turns the logits into probabilities.');
yours(0, COL, 300, '(a) Top token and roughly its probability. Then your guesses for the next four.', 'p1a');
mine(COL + GAP, COL, 300, "GPT-2's actual top 5 (I fill this in after you predict)", 'p1a-actual');
y += 330;
yours(0, COL, 220, '(b) Surprised by the probability? Why might a model spread its bets like this?', 'p1b');
yours(COL + GAP, COL, 600, "(c) The top logit = residual vector · that token's row of W_U. In 101 terms: what is each row of W_U, and what is the logit?", 'p1c');
y += 640;

heading('Problem 2: how softmax reacts');
para('Each token gets a share proportional to e^logit; the shares add to 1. Below: three tokens with logits [2, 1, 0]. The dotted frame is 100%. Sketch the bars you predict for (a), (b), (c), then write a line on each.');
const softmax = (z) => { const e = z.map(Math.exp), s = e.reduce((a, b) => a + b); return e.map((v) => v / s); };
const cases = [
  { title: 'Given: [2, 1, 0]', z: [2, 1, 0], shown: true, name: 'given' },
  { title: '(a) add 5 to all: [7, 6, 5]', z: [7, 6, 5], name: 'p2a' },
  { title: '(b) double all: [4, 2, 0]', z: [4, 2, 0], name: 'p2b' },
  { title: '(c) top +1 only: [3, 1, 0]', z: [3, 1, 0], name: 'p2c' },
];
const PW = 380, PH = 400;
cases.forEach((c, k) => {
  const x = k * (PW + 50);
  fig.text(c.title, x, y, { size: 22 });
  const p = fig.plot(x, y + 60, PW, PH, { yRange: [0, 1] });
  if (c.shown) p.bars(softmax(c.z), { labels: ['A', 'B', 'C'] });
  else ['A', 'B', 'C'].forEach((l, j) => fig.text(l, x + (j + 0.5) * (PW / 3), y + 60 + PH + 6, { size: 18, align: 'center' }));
  p.bounds.name(c.name);
  if (!c.shown) fig.box(x, y + 60 + PH + 50, PW, 300, { dash: 'dashed', label: 'Why:', name: c.name + '-why' });
});
y += 60 + PH + 50 + 300 + 40;
para('Then: what does (a) tell you about the logits in Problem 1 all being around -92?');
yours(0, 1380, 400, '', 'p2-92'); y += 440;

heading('Problem 3: loss, or how wrong was the model?');
para('Loss for one prediction = -ln(probability the model gave the right token). Same prompt as Problem 1.');
yours(0, 1000, 600, '(a) Rank the loss if the right answer were " Paris", " London", " banana". Roughly how far apart?', 'p3a');
mine(1000 + GAP, 640, 600, 'Actual losses (I fill this in after you predict)', 'p3a-actual');
y += 640;
yours(0, W, 450, '(b) Why does the loss punish a confident wrong answer so hard?', 'p3b');
```
## Appendix C: the script API as built (in the guide as "Drawing scripts")

Since slinger-bee990 this is the owner's Appendix A with what the build
adds (colour names, `fig.grid` options, `t.cell`, the plot's named ticks,
`title`, `frame: 'axes'`, `p.dots`, the warnings, the limits). The guide
prints it as below.

You write one JavaScript file (ES2023, as QuickJS runs it). Slinger runs it
in a sandbox and turns what it draws into ordinary canvas items, placed as
one figure.

- Coordinates are canvas units in the figure's own frame: (0, 0) is its top
  left, x grows right, y grows down. Slinger decides where the figure goes on
  the page (`--at` or `--over` place (0, 0); `--beside` and `--in` place
  its frame). Never assume page coordinates.
- Text is monospace: a character is `size` tall and `size / 2` wide, so
  `measure('hello', {size: 24})` is `{w: 60, h: 24}`.
- No imports, no network, no files, no timers, no `Date`, no `console`.
  `Math.random` is seeded: the same script and data draw the same figure.
- The global `data` holds the JSON passed with `--data` (or `null`).
- Nothing is on the canvas until the script ends, so build a thing, read its
  size, then place it.

**Draw.** Every call returns a handle. The last argument is always an
options object.

```js
fig.text(str, x, y, {size = 24, w, align = 'left'})   // w wraps the text to that width; align: 'left' | 'center' | 'right' (x is then the centre or the right edge)
fig.box(x, y, w, h, {label, size = 20})               // label: text inside the box, top left, wrapped
fig.slot(x, y, w, h, {label, by = 'user'})            // a box meant to be filled in: dashed for the user, dotted when by: 'agent'
fig.line(x1, y1, x2, y2)
fig.path([[x, y], ...], {closed = false})             // a polyline; closed joins the last point to the first
fig.circle(cx, cy, r)
fig.ellipse(cx, cy, rx, ry)
fig.arrow(x1, y1, x2, y2, {heads = 'end', label})     // as line; heads: 'end' | 'both' | 'none'
fig.arrow(from, to, {heads, label})                   // from, to: [x, y] or a handle (the arrow then meets its edge where it is now)
```

Options every call takes:

- `color`: `'#rrggbb'`, `'#rrggbbaa'` or a name: `agent`, `ink`, `muted`,
  `accent`, `good`, `bad`, `warn`, `series1` to `series6`, `black`,
  `white`, `red`, `green`, `blue`, `yellow`, `orange`, `purple`, `grey`.
  Default: the colour of the group or part it is in, else `fig.color` (the
  agent colour; assign it to change every later default).
- `fill`: a colour (stored as a 20% tint; `#rrggbbaa` as given) or `true`
  (a tint of `color`), for a box, circle, ellipse or closed path. Fills draw
  under all ink and text, so they never hide writing.
- `dash`: `'dashed'`, `'dotted'` or `'solid'` (boxes, lines, paths, arrows).
- `width`: line width (lines, paths, arrows; default 2). An arrow with heads
  is always 2 wide.
- `name`: names this part of the figure, such as `'answer-a'` (`a-z`, `0-9`,
  `-`, up to 40). A later command finds it as `<figure>/<name>`. Name every
  area that will be written in or updated.
- `locked`: default `true`: the user's pen and eraser pass over it. Pass
  `false` for something the user should be able to move.

An unknown option, or one the call cannot take, fails the call and lists
what it takes.

**Handles.**

```js
h.x  h.y  h.w  h.h  h.right  h.bottom  h.cx  h.cy     // its bounds, read-only
h.moveTo(x, y)   h.moveBy(dx, dy)                     // each move returns h
h.below(other, gap = 24)   h.above(other, gap)   h.rightOf(other, gap)   h.leftOf(other, gap)
h.centerIn(other)                                     // other: a handle or {x, y, w, h}
h.name('part-name')
```

`below` and `above` move only y, `rightOf` and `leftOf` only x. A box's
label and an arrow's label move with it.

**Helpers.**

```js
measure(str, {size = 24, w})                 // {w, h} of text, wrapped to w when given
fig.group(h1, h2, ..., {color, fill, dash, locked, name})   // one handle over several; its options reach every shape in it that sets none
fig.part(name, {color, fill, dash, locked, size}, () => { ... })   // every shape the function draws is in part name; returns a handle over them. Parts do not nest
fig.stack([h1, h2, ...], {dir = 'down', gap = 24})   // dir: 'down' | 'right'; moves each after the one before; returns a group
fig.grid([h1, ...], {cols = 3, gap = 24, x = 0, y = 0})    // rows of cols; each column as wide as its widest
fig.table(rows, x, y, {size = 20, header = true, widths, rowH, parts})   // rows: arrays of cells; columns fit their longest cell
const p = fig.plot(x, y, w, h, {xRange: [0, 1], yRange: [0, 1], ticks = 0, frame = 'dotted'})
p.x(v)  p.y(v)                               // data value -> figure coordinate (y is up in data)
p.bars(values, {labels})                     // one bar per value across the plot
p.fn((x) => y, {color, width, steps = 50})   // a curve; or an array of [x, y] in data
p.vector([x1, y1], [x2, y2], {label})        // an arrow in data coordinates
p.dots([[x, y], ...])                        // filled dots of radius 4
p.bounds                                     // a handle over the whole plot: p.bounds.name('chart')
```

- `fig.table`: one box per cell and one text per non-empty cell; `header`
  tints the first row; `widths` is one width or one per column; `rowH`
  defaults to 1.6 of the size; `parts` names cells (an array of rows of part
  names, `null` for none). It returns a group with `t.cell(r, c)`, a cell's
  `{x, y, w, h}`: a cell is boxed already, draw in it, not another box on it.
- `fig.plot`: `frame` is `'dotted'`, `'dashed'`, `'solid'`, `'axes'` (two
  axis lines) or `'none'`. `ticks: n` labels n steps along both axes;
  `xticks`/`yticks` list them instead (a value, or `[value, "label"]` such
  as `[0, "Jan"]`); `xlabel`, `ylabel`, `title` (above the plot); `size`
  (labels, default 18). `bars`, `fn` and `dots` each take the next series
  colour unless given one.

**Context.** `fig.name` (the figure's name), `fig.view.target`
(`{x: 0, y: 0, w, h}` of the `--over` or `--in` item, else `null`),
`fig.view.page` (the page's name). `warn(message)` adds a warning.

**Warnings** do not fail the draw: a label taller than its box, a word wider
than its text's frame (it is cut), two named parts that overlap, text over a
line, texts overlapping, a box drawn again on the same frame without a fill,
an arrow's label longer than the arrow, a shape far from the others.

**Errors** name the call and the line: `script:12:7: fig.box: w must be a number above 0, got "wide"`.
A script stopped at 1 s names a line its endless loop is at or after.
Limits: 1 s, 32 MB, 5,000 shapes, a 1 MiB script, 256 KiB of data.

**An example.** `bars.js` draws a title, a bar chart of its data and an
answer area beside it that a later sling can name (`rain/why`):

```js
const title = fig.text(data.title, 0, 0, { size: 32 });
const top = Math.max(...data.rows.map(([, v]) => v));
const p = fig.plot(0, title.bottom + 40, 380, 300, { yRange: [0, top], yticks: [0, top] });
p.bars(data.rows.map(([, v]) => v), { labels: data.rows.map(([l]) => l) });
p.bounds.name('chart');
fig.slot(420, title.bottom + 40, 500, 300, { label: 'Why is March the wettest?', name: 'why' });
```

```bash
slinger canvas draw <CANVAS> bars.js --name rain --beside c12 --data '{"title": "Rain (mm)", "rows": [["Jan", 80], ["Feb", 65], ["Mar", 90]]}' --agent claude
slinger canvas redraw <CANVAS> rain --data '{"rows": [["Jan", 80], ["Feb", 65], ["Mar", 90], ["Apr", 40]]}' --agent claude
```

## Appendix D: the reference worksheet as built (ported to Appendix A, slinger-bee990)

The acceptance script (steps 7, 9, 14). With the data below it draws **56
items in 15 parts** (33 texts, 17 boxes, 4 arrows of which 3 are lines,
2 strokes: the plot's curve and the closed `check` circle). The redraw with the second data patch fills
`actual-top-5` and `feedback` by name: 62 items, 6 texts added and the
`check` circle filled (one `set`); every other item is unchanged and the
user's strokes are untouched. The script is `crates/script/src/tests/worksheet.js`; its tests
check these counts in QuickJS.

```js
const d = data;
const pct = Math.round(d.rate * 100);
const value = (t) => d.start * (1 + d.rate) ** t;

fig.part('title', () => {
  fig.text(d.title, 0, 0, { size: 40 });
  fig.text('Work in the dashed boxes. The dotted ones are mine.', 0, 56, { color: 'muted' });
});
fig.part('name', () => {
  fig.text('Name', 620, 8, { color: 'muted' });
  fig.line(690, 32, 900, 32);
});

// 1. Read a graph.
fig.text(`1. ${d.start} is saved at ${pct}% a year, added once a year.`, 0, 120, { name: 'q1' });
const p = fig.plot(60, 190, 480, 280, {
  xRange: [0, d.years], yRange: [0, 250], frame: 'axes',
  xticks: [0, 2, 4, 6, 8, 10], yticks: [0, 100, 200],
  xlabel: 'years', ylabel: 'balance', name: 'q1-plot',
});
p.fn(value, { steps: 40 });
fig.text('1a. About how much after 5 years?', 600, 190, { size: 20, w: 300 });
fig.slot(600, 250, 300, 90, { name: '1a-answer' });
fig.text('1b. When has it doubled?', 600, 360, { size: 20, w: 300 });
fig.slot(600, 400, 300, 90, { name: '1b-answer' });

// 2. Fill the table.
fig.text('2. Fill in the balance.', 0, 540, { name: 'q2' });
fig.table([
  ['year', ...d.table.map(String)],
  ['balance', value(d.table[0]).toFixed(0), '', '', ''],
], 0, 580, { widths: 150, size: 24, name: 'q2-table' });
fig.slot(0, 680, 900, 120, { name: '2-working' });

// 3. Predict, then compare.
fig.text(`3. Rank these by balance after ${d.years} years.`, 0, 840, { name: 'q3' });
fig.stack(d.accounts.map((a) => fig.text(`${a.name}: ${a.start} at ${a.rate}%`, 0, 880, { size: 20 })), { gap: 4 })
  .name('3-accounts');
fig.slot(0, 1010, 430, 200, { name: '3-predicted' });
fig.arrow(436, 1110, 464, 1110);
fig.text('Actual top 5', 470, 1010, { color: 'muted', size: 20 });
fig.part('actual-top-5', () => {
  fig.slot(470, 1040, 430, 170, { by: 'agent' });
  if (d.actual) fig.stack(d.actual.map((n, i) => fig.text(`${i + 1}. ${n}`, 486, 1052, { size: 20 })), { gap: 8 });
});

// Feedback, written on a later turn.
fig.part('feedback', () => {
  fig.slot(0, 1250, 900, 140, { by: 'agent' });
  if (d.feedback) fig.text(d.feedback, 16, 1266, { w: 868 });
});
fig.circle(880, 1230, 8, { fill: d.feedback ? 'series3' : undefined, name: 'check' });
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
