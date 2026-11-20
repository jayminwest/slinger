# Folding a box

Status: picked overnight 2026-11-20 (slinger-ef37a2, plan pl-f130a2, parent
slinger-cb1ac1), owner to confirm. Inputs: D6, B4, B5, B6, the canvas file
(`docs/design/canvas-file.md`), the packet (`docs/design/packet.md`), and the
owner's pick in the fold prototype (tabs B and C).

The later steps of the plan build on the section "Recommended" at the end. A
step that cannot do what it says reports that; it does not pick another way.

## The look (picked by the owner, not reopened here)

A box is the item `canvas put --box` already makes. Folding adds no new noun:
it is something you do to a box.

- **Open.** A box's contents are what lies inside its edges, by geometry, live.
- **Fold.** The box shrinks to a **miniature** of itself and gives up its room.
  At that moment its contents are frozen into a stored list; the miniature
  draws them, scaled. Notes written later where the box used to be never join it.
- **Step in.** A tap on a miniature makes the pane show only that box's
  contents; the footer shows the trail (page, box, inner box); escape steps
  out. Which box a pane is inside is view state of the pane, like its page.
  To see two drawers at once, split the pane: each pane keeps its own.
- **Unfold.** The box opens in place when its old room is free. When it is not,
  it opens as a **peek**: drawn above the page, and escape or a tap outside
  folds it back. A peek writes nothing.

Rejected by the owner (plan pl-f130a2): a fold to one line, every box a drawer,
named frames or pages inside pages. An outline inside one text is left open.

## Owners

| Step | Seed | Owns |
|---|---|---|
| 1 | slinger-ef37a2 | this page |
| 2 | slinger-bd9c19 | core: the `fold` field, fold and unfold, carrying contents on move and erase |
| 3 | slinger-3242a7 | core: the packet line, the outline, `read --in`, crop of a folded box |
| 4 | slinger-e96a9a | the verbs through control, daemon and CLI, with their docs and `guide.md` |
| 5 | slinger-1350e7 | app: miniature, fold mark, fold key, unfold or peek |
| 6 | slinger-b1ccf1 | app: step in, the trail, step in beside |
| 7 | slinger-9923a6 | sling a box (with slinger-387333) |

## What an old reader does today (checked before picking)

Acceptance 5 of the plan: a build from before folds opens a canvas file with
folds and loses nothing. The reader in `crates/core/src/canvas/file/` does this:

1. **Unknown operation kinds** (`op` it does not know) and lines by an author it
   does not know read as `Line::Unknown` (`file/mod.rs`, `read_line`): kept,
   skipped by replay.
2. **An `add` of an unknown item kind** is `Line::Unknown` too; replay still
   moves the id counter past its id (`replay.rs`, `reserve_unknown`), so later
   ids mean the same. A later `move`, `set` or `remove` of it is a no-op (B4).
3. **Unknown fields on a known item** are ignored (`json::read_shape`): the
   item in memory has only the fields that reader knows.
4. **The file is never rewritten.** Every write is an append. The only whole
   file written (`whole.rs`) is a new canvas built in memory (an import). So
   every line a newer writer wrote stays on disk.
5. **But an old reader writes whole items from memory** in two places: `restore`
   (undo of an erase) and `page.restore` (undo of a page removal), through
   `json::whole_item`. Such a line lacks every field the old reader dropped in
   (3), and a newer reader replaying it gets the item without them. This is
   the one way an old reader takes something away.
6. **An old `set` keeps unknown fields.** It writes only the fields it knows,
   and replay lays a `set`'s fields over the item's current ones
   (`replay.rs`), so a field it did not name is left as it was.
7. **A known operation with a known field of the wrong type is damage**
   (`canvas_corrupt`): `read_change` fails the line. A new design never changes
   the type of an existing field (an integer `w` stays an integer).
8. An old reader rebuilds undo stacks from `step`/`undoes`/`redoes`. It can
   undo a newer writer's `set` step; its inverse is a `set` of the fields it
   knows, so (6) leaves a new field in place.

So a design keeps every item through an old reader when no item's existence
depends on a field of another item, and no item leaves the page's item list.

## The two alternatives (B6): how the file stores a fold

Both keep the miniature's frame as the box's frame (so `x`, `y`, `w`, `h` are
always what is on the page) and differ in where the frozen contents live.

### A: the contents stay items on the page; the box lists their ids

A folded box gains one optional field, `fold`: its open size and the ids of
its contents. The contents stay ordinary items with their own lines, at their
absolute positions; they are hidden because a folded box lists them.

```
{"n":9,"t":"…","by":"user","op":"add","item":{"id":"i4","page":"p1","kind":"box","color":"#000000","x":1800,"y":7000,"w":9000,"h":6000}}
{"n":12,"t":"…","by":"user","op":"set","id":"i4","fields":{"color":"#000000","fold":{"w":9000,"h":6000,"items":["i5","i6","i8"]},"h":1600,"kind":"box","w":2400,"x":1800,"y":7000},"step":12}
```

- Fold and unfold are a `set` of the box: no new operation kind. Undo is the
  `set` of the old shape, exact, as for any edit.
- Positions relative to the box are kept by carrying: a move of a folded box
  writes one `move` naming the box and every item it holds, at any depth, so
  member minus box origin never changes. An erase does the same with `remove`.
- Every query of a page (drawing, hit test, packet, room, `in`) skips hidden
  items. One rule in the core decides "hidden".
- **Old reader:** sees a small box at the old room's top left and every
  content item where it was, visible. It can move, edit or erase each one, and
  a newer reader sees those changes. Its `restore` of the box (5) drops `fold`:
  the box comes back open and small, its contents visible. The fold is lost,
  never an item. An old move of only the box leaves the contents behind: a
  newer reader still holds them in the box (see "Move").
- Cost: a filter on every page query, and the carry rule on move and erase.

### B: the contents move into the box

A fold removes the contents from the page and stores them whole inside the
box, positions relative to the box's top left:

```
{"n":12,"t":"…","by":"user","op":"set","id":"i4","fields":{…,"fold":{"w":9000,"h":6000,"items":[{"id":"i5","kind":"stroke","dx":40,"dy":20,…},…]}},"step":12}
{"n":13,"t":"…","by":"user","op":"remove","ids":["i5","i6","i8"],"step":12}
```

- Relative positions are literal; a move of the box moves its contents with no
  carry rule; a page's item list never holds a hidden item, so no filter.
- Editing inside a folded box (step in, then write) needs new operations that
  address an item inside an item, and new rules for the change feed, arrows and
  ids of nested items.
- **Old reader:** the `remove` (13) takes the contents off its page: they are
  gone from what it shows, though on disk. An erase of the box there, then
  undo, writes a `restore` without `fold` (5): after that line no reader has
  the contents at all. Fails acceptance 5. Writing the fold as a new operation
  kind instead (skipped by the old reader) keeps the items visible to it, but
  its edits of them name ids a newer reader no longer has on the page, so they
  are no-ops there: the old reader's work is lost instead.

## The rules (for the pick, A)

### Contents at fold time

The candidates are the items on the box's page that are **in the same place
as the box**: visible on the page when the box is on the page, or held by the
same folded box when it is folded inside one (stepped in). Of those, the box
takes every item, other than itself, whose bounds (a stroke's with its width)
lie wholly inside the box's frame, edges included.

- **Across an edge:** an item that is not wholly inside stays out and stays
  where it is. Its overlap with the old room makes a later unfold a peek.
- **Nested, open:** an open box wholly inside is taken, and so is each item
  inside it, each a direct member of the folding box. The inner box can be
  folded later from inside (step in); its contents then move from the outer
  list to its own, in the same step.
- **Nested, folded:** a folded box whose miniature lies wholly inside is taken
  as one member; its own list stays its own.
- **Overlapping boxes:** a box that only overlaps is not taken. An item in
  the overlap that is wholly inside the folding box is taken. An item belongs
  to at most one folded box: the one that took it.
- **Arrows:** an arrow is content when its bounds are wholly inside, as any
  item. An arrow with one end bound inside stays on the page (below).
- An empty box folds too: its list is empty.
- Folding a folded box, or unfolding an open one, writes nothing (B4).

### Hidden, read side

- An item is hidden when a folded box on the same page lists it, or lists a
  box that holds it, at any depth. Each item has at most one holder.
- A listed id that is gone, or on another page, is skipped. An item listed by
  two boxes belongs to the lower box id; a list that would make a box hold
  itself, at any depth, is ignored. No list is ever an error.

### The miniature

- Folding sets the box's frame to the miniature and keeps the open size in
  `fold`. The miniature keeps the top left and the open frame's proportions;
  its longer side is a quarter of the open frame's, clamped to 80..320 units,
  and never longer than the open side. The rule is one function in the core
  (`canvas::geometry::miniature`); the app reads the frame from the item.
- The miniature draws the open frame's region scaled into it, with what the
  box holds directly and, for a folded box it holds, that box's miniature.
- A resize of a folded box changes the miniature only, never the open size.

### Unfold and peek

- The old room is the open frame: the box's `x`, `y` with `fold`'s `w`, `h`.
  It is free when no visible item, other than the box, has bounds that
  intersect it.
- Free: unfold writes a `set` of the box: open size back, `fold` cleared. Not
  free: unfold writes nothing and fails with `conflict` naming the items in
  the way (hint: move them, or read the box with `canvas read --in`). The app
  answers that `conflict` with a peek.
- Contents left outside the open frame (see "Move") come back where they are.

### Step in

- View state of a pane in the app, never in the canvas file.
- Inside an open box the pane shows the items wholly inside it now; inside a
  folded box, its list. The view starts at the box's top left and does not
  scroll above or left of it.
- An item added while stepped into a folded box joins it: the request's `add`
  carries `in` (the box's id), and the core writes the `add` and a `set` of
  the box's `fold` in one step. When the item reaches past the open frame, the
  open size grows to hold it (right and down only, so the miniature stays).
- A box that is erased, or a folded box unfolded elsewhere, steps the pane out.
- In the app (slinger-b1ccf1): `step-in` (opt+cmd+I) on the selected box, or
  two quick taps on a miniature with any tool but the pen and the eraser
  (one tap is left to selecting it); `step-out` (opt+cmd+O), the footer's ⤴,
  or escape. Escape takes, in order: the key overlay, the page menu, the
  sling entry, the text being typed, a peek, a pop-up, the selection, and
  only then steps out one box. `step-in-beside` (opt+shift+cmd+I) splits.

### The packet line

- A folded box is one `box` line, its box the miniature, with a new field
  `folded=<n>`: how many lines its contents would make in a packet of their
  own (clusters, texts, boxes, arrows). When a direct member is a text, the
  quoted part is the first line of the first one in reading order, cut at 80
  characters as in `page`. An ink-only box has no quoted part.

  ```
  b4 box 180,700 240x160 folded=7 "Monday: what ships"
  ```

- Hidden items make no line in any section. A folded box that holds ink new
  in `from..=to` carries `new=<strokes>`, and its line goes in `new`, so new
  ink is never silent.
- `in` names the smallest visible box that contains the item; a hidden item
  has no line, so no `in`.
- An arrow end bound to a hidden item names the folded box that holds it,
  outermost on the page (`to=b4`).
- **Outline** (`page` section): the same one line.
- **Read:** `canvas read <ID> --in <box>` is the packet of a box's contents,
  open or folded, as if the box were a page: the box's line is not in it, its
  direct members are, and a folded box inside is again one line.
- **Crop:** `canvas crop <ID> --item <box>` of a folded box renders it open:
  its open frame and what it holds, hidden items drawn. `--box` renders what is
  visible.
- Packet version stays 1: one new field (B5).

### Arrows bound to hidden items

- The file keeps the arrow's `from`/`to` as they were: the binding is to the
  item, which still exists.
- An arrow that is visible and bound to a hidden item is drawn to the frame of
  the outermost folded box that holds the item, at the point nearest its other
  end. On unfold it is drawn to the item again. Nothing is written.
- Carrying a folded box moves its contents, so arrows bound to them follow
  through the existing rule (`Log::followed`, as for any move).

### Move

- A move of a folded box is one `move` line naming the box and every item it
  holds, at any depth. Bound arrows follow in the same step.
- An item moved alone (stepped in) stays in the box wherever it goes.
- An old reader's move of the box alone leaves the contents on disk where they
  were; a newer reader still holds them in the box, the miniature clips them
  to the open frame, and an unfold brings them back where they are.

### Erase

- Erasing a folded box is one `remove` line naming the box and every item it
  holds, at any depth: one step.
- Erasing an item stepped in is a plain `remove`; its id stays in the list and
  is skipped (above). Undo puts it back, in the box.
- An erase that crosses a miniature on the page erases the folded box whole.

### Undo

- Fold, unfold, a carried move and a carried erase are each one step. Undo of
  a fold or unfold is a `set` of the box's old shape (with or without `fold`);
  undo of a carried erase is a `restore` of every item, the box's line whole
  with its `fold`.

### Who may

- The canvas's author rule holds for every item a line names. The user may
  fold, move and erase any box. An agent may fold only a box it made, and
  move or erase a folded box only when it may change every item carried;
  otherwise the request fails as an edit of another's item does today.
- Anyone may add into any folded box (`add` with `in`, or `canvas put --in`):
  a `set` of a folded box that keeps its frame and colour and changes its
  list only by items its author may change is allowed (`Canvas::joins`; see
  "Sling a box"). Taking another's items in or out is not.

### What an old reader keeps

Everything but the fold: every item, visible, editable. The worst case is an
erase and undo of a folded box in an old build: the box comes back open and
small, its contents visible where they were (finding 5 above).

## Recommended (picked overnight, owner to confirm)

Picked by the brief's rule: cheapest to reverse, and changes least of what the
pen does today.

**Store a fold: A, the contents stay items on the page and the box lists
them.** B loses contents through an old reader (finding 5), and B needs a new
way to address items inside items. A is one optional field and no new
operation kind, so taking it back is ignoring one field.

### The file (step 2)

The `box` item gains one optional field, last:

| field | value |
|---|---|
| `fold` | `{"w":<open w>,"h":<open h>,"items":["i5",…]}`: the box is folded; `w`, `h` in tenths (both above 0), `items` the ids it holds directly, in the order taken (item id order). Absent or `null`: open |

- Written by `add`, `set` (as `"fold":null` when open, like a pdf's `crop`),
  `restore` and `page.restore`. No `v` change; a line in `canvas-file.md`'s
  item table and Versions list.
- `canvas_apply` takes `fold` as any field: a client may set it raw (undo,
  import and paste need it). The core checks only that `w`, `h` are above 0
  and `items` are item ids. In a request's `add`, `items` may hold `$n`.

### The core (steps 2, 3)

- `Shape::Box { frame, colour, fold: Option<Fold> }`,
  `pub struct Fold { open: (f64, f64), items: Vec<ItemId> }`. Frame is the
  frame on the page (the miniature when folded).
- `canvas::geometry::miniature(open: Rect) -> Rect`.
- `Canvas::holder(item) -> Option<ItemId>`: the folded box that holds an item
  directly; `Canvas::hidden(item) -> bool`; `Canvas::contents(box) -> Vec<ItemId>`:
  direct contents, live geometry when open, the list when folded. Every page
  query (render, packet, near, hit test) goes through one visible-items
  iterator, so "hidden" is decided in one place.
- `Log::fold(&mut self, by, item, once) -> Result<Written>` and
  `Log::unfold(…) -> Result<Written>` (`Error::Conflict` with the ids in the
  way). Carrying is a request expansion beside `Log::followed`: a request
  `move` or `remove` of a folded box writes the expanded line; replay applies
  lines as they are.
- Packet: the `folded` and `new` fields, the arrow-end rule, `Ask` gains `in`
  (a box) for `read --in`; crop of a folded box draws it open.

### The verbs (step 4)

| control | CLI | reply |
|---|---|---|
| `canvas_fold`: `canvas`, `item`, `by` (default `user`) | `slinger canvas fold <ID> <ITEM> [--agent NAME]` | `item`, `page`, `x`, `y`, `w`, `h` (the miniature, tenths), `seq`. Prints nothing |
| `canvas_unfold`: as `canvas_fold` | `slinger canvas unfold <ID> <ITEM> [--agent NAME]` | as `canvas_fold`, the open frame; `conflict` (exit 4) when the room is taken |
| `canvas_read` gains `in`: an item id | `canvas read <ID> --in <BOX>` | the box's packet |

Both are one undo step, take `once`, and accept packet ids (`b4`). The app
folds through `canvas_fold`, not a raw `set`, so the contents rule is the core's.

### The app (steps 5, 6)

- Draws a folded box's miniature from its frame and `fold`; hides what the
  folds list (a set of ids from the items it already reads: no new reply type).
- Unfold calls `canvas_unfold`; on `conflict` it peeks. A peek and step in are
  pane state.
- The fold mark and the key (step 5, picked in slinger-1350e7, owner to
  confirm in slinger-03095f): the mark is the box's top right corner turned
  down, on every folded box and on an open box selected alone; a press on
  it with any tool but the eraser folds or unfolds that box (the
  selection's corner handle keeps the corner itself). `fold` (opt+cmd+F)
  folds or unfolds each selected box. A peek closes on escape or a press
  outside; a press inside it does nothing.
- Writing while stepped into a folded box sends `add` with `in`.
- Paste of a folded box adds the contents, then the box with `fold.items` as
  `$n` (follow-up for copy and paste, not in the plan's steps).

### Sling a box (step 7)

The packet of a sling of a box is `canvas read --in <box>`; its contents are
`Canvas::contents`. `sling_send` takes `in`; line 1 of the message names
the box (`in=b4`) as the packet header does, and the note on every sling of
a box says to answer with `canvas put --in b4`.

Where the answer lands (settled with slinger-387333, picked overnight
2026-11-20, owner to confirm): the box is **named, never detected** from
ink, and a put **does not default** to it; the agent passes `in`. That is
the reading cheapest to reverse: a default can be added later on top of
the named box, and nothing guesses. A put with `in` goes in the box's room
(`Canvas::free_in`: beside its item first, then reading order, among the
box's contents only; page items outside do not count). An open box with no
room is `conflict`; a folded box takes it below its contents and grows its
open size, as an `add` with `in` does. Joining writes a `set` of the box,
so the author rule takes one exception (`Canvas::joins`): anyone may change
a folded box's list by items they may change (theirs), and its open size,
keeping its frame and colour. So an agent answers in the user's folded box,
and its undo takes the answer out; it still cannot take the user's items
in or out. An open box never grows for an answer.

The app (slinger-1350e7, slinger-b1ccf1) sends `sling_send` with `in`: the
box selected for "sling a box", or the box a pane is stepped into.

### Tests that hold acceptance 5 (step 2)

A file with folds, nested and carried, replayed with every `fold` key stripped
(an old reader's view), has every item of the full replay, at the same place;
and a `restore` of a folded box without `fold` leaves the box open with every
content item visible.
