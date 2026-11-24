# Figures: an agent draws with code

Status: draft 2026-11-24, owner to confirm. Nothing here is built. Inputs:
D6, D11, D13, D19, K4, B1, B6, B9, the canvas file
(`docs/design/canvas-file.md`), fold (`docs/design/fold.md`), live items
(`docs/design/live-items.md`) and the engine spike below.

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
