# Canvas file format

Closes SPEC O5. Inputs: D6, D7, B4, B5, B6, K5, R1, and the canvas plan (pl-2a55).
Two alternatives, then the pick, then the format the codec seed builds.

## What the file must do

- Hold one canvas: its pages and the D6 items (stroke, text, box, arrow).
- Answer "what changed after point p" (R1, `canvas read`, the change feed).
- Record who made each change and when (section 7: agent name and time; Spike B:
  stroke time helps clustering).
- Carry a version. A change never breaks a reader (B5). A reader accepts unknown
  operations (plan acceptance 4).
- Survive a crash in the middle of a write without losing an acknowledged change.
- One writer: the daemon on one machine (K5). No merge, no locking between processes.

## Evidence: the real board

`idea.tldraw` (tldraw 3: a zip holding a SQLite file) has 5 pages, 7563 draw shapes,
83 geo, 57 arrows, 52 text, 114 bindings. The draw shapes hold 170 938 points
(median 21 per stroke, 99th percentile 63). As tldraw JSON the strokes take 4.8 MB.

A throwaway script (scratchpad, not in the repo, B9) wrote the pages and strokes in
both encodings below, with points as delta integers in tenths of a unit:

| | bytes | gzip | parse every line (Python `json`) |
|---|---|---|---|
| A, operation log | 2 289 553 | 402 787 | 33 ms |
| B, item snapshot | 2 160 397 | n/a | about the same |

Rust `serde_json` is faster than Python's `json`, so a full load of the largest real
board is tens of milliseconds in either alternative. Size and load time do not
separate them today. Growth, the change feed, crashes and versions do.

## Shared choices

Both alternatives use these. They are part of the pick.

- **Text, JSON Lines.** UTF-8, one JSON object per line, every line ends in `\n`.
  `grep` and `jq` work on it. Keys are short but whole words or common short forms.
- **Coordinates** are canvas units (1 unit = 1 point at zoom 1), stored as integers in
  tenths of a unit. Tenths keep 0.8 screen pixels of error at 8x zoom and keep
  numbers short.
- **Stroke points** are one flat array `[x0, y0, dx1, dy1, dx2, dy2, ...]`: the first
  pair absolute, each later pair the change from the one before. No pressure (D6).
  Deltas are small, so the median stroke is about 120 bytes.
- **Ids** are opaque strings to the codec, at most 64 bytes. The model seed decides how
  they are made.
- **Undo is not in the file.** Undo and redo write the inverse change as ordinary
  operations (undo of `add` is `remove`; undo of `remove` is `restore` with the whole
  item, which the daemon holds in memory, and
  `page.restore` puts back a removed page with its items; undo of a `move` is a
  `set` of the old shape, so it is exact). The undo stack lives in the daemon's
  memory, per canvas, and is empty after a restart. If a persisted undo stack is ever
  wanted, an optional `"undoes": n` field adds it with no version change.
- **A drag writes once.** A client sends one `move` when the pen lifts, never one per
  frame. This keeps the log the size of the decisions, not of the motion.

## Alternative A: an append-only operation log

The lean in O5. One file per canvas, `<canvas-id>.canvas`. Line 1 is a header. Every
later line is one operation, with the same shape as a record event (number, time,
source, kind, payload).

A small canvas, two pages, a stroke, a box, a moved stroke, an agent's text, an erase:

```
{"slinger":"canvas","v":1,"id":"c1","first":1}
{"n":1,"t":"2026-11-10T16:41:46.982Z","by":"user","op":"page.add","page":"p1","name":"ideas"}
{"n":2,"t":"2026-11-10T16:41:47.010Z","by":"user","op":"page.add","page":"p2","name":"spec"}
{"n":3,"t":"2026-11-10T16:41:50.500Z","by":"user","op":"add","item":{"id":"i3","page":"p1","kind":"stroke","color":"blue","width":20,"pts":[2096,7386,-29,-64,0,-40,40,80]}}
{"n":4,"t":"2026-11-10T16:41:52.120Z","by":"user","op":"add","item":{"id":"i4","page":"p1","kind":"box","color":"black","x":1800,"y":7000,"w":900,"h":600}}
{"n":5,"t":"2026-11-10T16:41:55.000Z","by":"user","op":"move","ids":["i3"],"dx":120,"dy":-40}
{"n":6,"t":"2026-11-10T16:42:10.400Z","by":"agent:scout","op":"add","item":{"id":"i6","page":"p1","kind":"text","color":"black","x":2800,"y":7000,"text":"Two pages, one box."}}
{"n":7,"t":"2026-11-10T16:42:30.000Z","by":"user","op":"remove","ids":["i3"]}
```

- **Undo:** inverse operations appended (above). History is complete in the file.
- **Pages:** `page.*` operations; every item names its page. Load builds a per-page
  index in memory.
- **7500 strokes:** 2.3 MB, one `add` per stroke. Load replays every line: tens of ms.
- **File size:** the live items plus every change ever made. Each erase adds about
  80 bytes, each lasso move about 16 bytes per id. Churn grows the file without bound
  until compaction exists (below).
- **Change feed and R1:** the operations after point p are the lines with `n > p`.
  The daemon keeps them in memory, so a feed read touches no disk.
- **Old reader, new file:** an old reader skips operation kinds and fields it does not
  know and appends only its own lines. It never rewrites the file, so nothing a newer
  writer put there is lost. A `v` it does not know stops it before it reads anything.
- **Crash mid-write:** the only write is an append of whole lines. The worst case is a
  torn last line. It was never acknowledged (the daemon replies only after `fsync`),
  so the reader cuts it off and loses nothing a client was told was saved.

## Alternative B: a snapshot of items with an operation tail

Two files per canvas. `<canvas-id>.canvas` is a snapshot: a header with the last
operation folded in, then the pages, then one line per live item. Every item carries
its maker, its time and the number of its last change. `<canvas-id>.canvas.tail`
holds the operations after the snapshot, in the A format. When the tail passes 1000
operations, or the daemon stops, the daemon folds it: write the new snapshot to a temp
file, `fsync`, rename over the old one, `fsync` the directory, then empty the tail.

The same canvas after a fold:

```
{"slinger":"canvas","v":1,"id":"c1","through":7}
{"page":"p1","name":"ideas"}
{"page":"p2","name":"spec"}
{"item":{"id":"i4","page":"p1","kind":"box","color":"black","x":1800,"y":7000,"w":900,"h":600},"by":"user","t":"2026-11-10T16:41:52.120Z","n":4}
{"item":{"id":"i6","page":"p1","kind":"text","color":"black","x":2800,"y":7000,"text":"Two pages, one box."},"by":"agent:scout","t":"2026-11-10T16:42:10.400Z","n":6}
{"gone":["i3"],"n":7}
```

- **Undo:** the same inverse operations, written to the tail. History before the last
  fold is gone.
- **Pages:** page lines in the snapshot, `page.*` operations in the tail.
- **7500 strokes:** 2.2 MB snapshot. Load reads the snapshot and replays the tail.
- **File size:** bounded by the live items plus 1000 operations, plus tombstones.
- **Change feed and R1:** "what changed after p" needs every removal since p, so the
  snapshot must keep tombstones (the `gone` line), as tldraw's own file does, and
  must decide when to drop them. A client behind `through` cannot be given the
  operations; it gets the whole snapshot instead. That is a second reply shape.
- **Old reader, new file:** an old reader that folds rewrites the snapshot. To lose
  nothing it must carry unknown items, fields and operations through verbatim, on
  every fold. A bug there destroys data a newer writer made.
- **Crash mid-write:** the tail has A's torn-line case. The fold adds another: a crash
  after the rename and before the tail is emptied leaves operations in both files, so
  the reader must skip tail lines with `n <= through`. Two files, three write steps,
  more states to test.

## The pick: A, the append-only operation log

1. The change feed and R1 are "lines with `n > p`"; B needs tombstones and a second
   reply for clients behind a fold.
2. A has one write (append, `fsync`) and one crash case (a torn last line); B adds a
   second file, an atomic rename and a fold that can be cut in half.
3. A never rewrites, so unknown operations survive an old reader by construction.
4. Size and load are equal on the real board (2.3 MB against 2.2 MB, tens of ms).
5. A's cost is growth from churn; compaction can come later with no version change.

## The format the codec builds

Everything here is fixed. The model seed (slinger-5814) owns the meaning of each
operation; this section owns the bytes.

### File

- One file per canvas, named `<canvas-id>.canvas`, in `canvases/` in the daemon's
  data directory (docs/service.md).
- UTF-8 JSON Lines. Every line, including the last, ends in `\n`. No blank lines.
- A writer writes keys in the order shown and no spaces outside strings, so the same
  operation is always the same bytes. A reader accepts any key order and whitespace.

### Header (line 1)

```
{"slinger":"canvas","v":1,"id":"<canvas-id>","first":1}
```

- `slinger` is always `"canvas"`. A file without it is not a canvas file.
- `v` is the format's major version, an integer. Today 1.
- `id` is the canvas id.
- `first` is the `n` of the first operation line. A new file has 1. A reader must
  honour any value, so a later compaction (a file that starts with one `add` per live
  item, numbered from `first`) needs no version change.

### Operation lines

Envelope, in this key order, then the operation's own fields:

| key | type | meaning |
|---|---|---|
| `n` | integer ≥ 1 | sequence number: `first` on line 2, then exactly +1 per line |
| `t` | string | RFC 3339 UTC time with milliseconds, e.g. `2026-11-10T16:41:46.982Z` |
| `by` | string | `"user"` or `"agent:<name>"` |
| `op` | string | the operation kind |

Operation kinds in v1 (their fields follow the envelope):

| `op` | fields |
|---|---|
| `page.add` | `page` (id), `name` (string). The page goes last. |
| `page.rename` | `page`, `name` |
| `page.move` | `page`, `index` (integer, 0-based position after the move) |
| `page.remove` | `page`. Its items go with it. |
| `add` | `item` (an item object, below) |
| `remove` | `ids` (array of item ids). Ids already gone are fine (B4). |
| `move` | `ids`, `dx`, `dy` (tenths) |
| `set` | `id`, `fields` (object of item fields to replace whole, e.g. `{"text":"..."}`) |
| `restore` | `item`: a removed item put back whole, under its own id. The item object adds `by` and `t` (its maker and time) after `page`. Undo and redo write it. |
| `page.restore` | `page`, `name`, `index` (0-based position), `items` (whole items, as in `restore`). Undo writes it. |

Item objects. Every item has `id`, `page`, `kind` and, but for `live`, `button` and `pdf`, `color`; then by
kind:

| `kind` | fields (lengths and positions in tenths of a unit) |
|---|---|
| `stroke` | `width`, `pts` (delta array, above, at least one pair) |
| `text` | `x`, `y`, `w`, `h` (the frame the text is laid out in; `w` and `h` read as 0 when missing), `text`, then optional `size`: the text's size, one character cell, from 40 to 4000 tenths (4 to 400 units). A missing or `null` `size` is 240 (24 units), and a writer leaves it out at 240, so a file from before sizes reads and writes the same bytes. A frame narrower than one character cell gets the size of the text unwrapped; one with no height keeps its width and gets the height of the text wrapped to it. Zero-width characters (U+200B, U+200C, U+200D, U+2060, U+FE0F, U+FEFF) take no cell. Readers and writers apply this rule, so a line written without a frame size replays to the same frame. A `set` of a text carries `"size":null` for the usual size, which clears an earlier one. |
| `box` | `x`, `y`, `w`, `h` |
| `arrow` | `pts` (delta array, two or more pairs); optional `from`, `to` (item ids it points at) |
| `live` | `x`, `y`, `w`, `h` (both above 0), `ui` (the UI's name), `hash` (the blob id of the version shown), `open` (the showing, `pick-slot#12`) |
| `button` | `x`, `y`, `w`, `h` (both above 0): the built-in "send to agent" button (SPEC section 8). It names no code: the app draws it, and a tap on it slings what is new on its page |
| `pdf` | `x`, `y`, `w`, `h` (both above 0), `hash` (the PDF's blob id), `pdfpage` (the page number, from 1), `pagesize` (`[w, h]`, the whole page as displayed, in tenths of a PDF point, both above 0), then optional `crop` (`[x, y, w, h]` in tenths of a point from the page's top left, inside the page, `w` and `h` above 0; absent: the whole page) and optional `name` (the PDF's file name without its directory, 1 to 255 bytes, no `/`, no control character). One page of a PDF, or part of one, stretched to fill the frame (`docs/design/pdf.md`). A `set` carries `"crop":null` and `"name":null` to clear them |

`color` is a string: a palette name the model defines, `#rrggbb`, or `#rrggbbaa` when
the colour is not opaque. The model has no palette yet, so a reader reads any other
string as black.

### Ids, units and the first page

- Item ids are `i<n>` and page ids `p<n>`, from the model's counters (slinger-5814).
- A new canvas has one page, `p1`, named `Page 1`, before any line. The file does not
  record it.
- Ids rise. On replay the ids in the file are authoritative: before each `add` and
  `page.add` the reader moves its counter up to the id the line names, so an `add` it
  skips (an item kind it does not know) never shifts the ids after it. An id at or
  below one already handed out is `canvas_corrupt`.
- The model holds `f64` canvas units. A writer rounds each value to the nearest tenth,
  halves away from zero; a reader gives `k / 10.0`. Stroke points are rounded one by
  one before their deltas are taken, so error does not build up along a stroke. A
  stroke's width is at least one tenth. The daemon applies the rounded operation
  (`canvas::file::snap`), so the canvas in memory is the one the file rebuilds. One
  exception: a `move` adds two grid values in `f64`, which can land a hair (about
  1e-12 of a unit) off the grid in memory, while a later `restore` or `set` of that
  item writes the grid value. The two agree at tenths, which is all the file keeps.
- A `set` lays `fields` over the item's current fields. The writer sends every field
  of the new shape, with a missing optional field (an arrow's `from` or `to`) as
  `null`, which clears it. Keys inside `fields` are written sorted.
- A v1 reader reads `page.move` as a line it does not know and skips it: the model
  cannot reorder pages yet.
- A text's `size` is a new optional field: a reader from before it ignores it and
  draws the text at 24 units, so nothing else of the file reads differently.
- A live item (an agent UI on the canvas, `docs/design/live-items.md`) is a new item
  kind in `add`, not a new operation: a reader from before it keeps that `add` as a
  line it does not know, its counter still moves past the id, and a later `move`,
  `set` or `remove` of the id finds no item and changes nothing, so the rest of the
  file replays the same. A `button` is added the same way, and so is a `pdf` item
  (`docs/design/pdf.md`): an app from before it draws nothing for one.
- A workspace's new canvas has the user's button as its line 1 (SPEC section 8):
  an ordinary `add`, so the format does not change.

### Versions (B5)

- New operation kinds, new item kinds and new optional fields are additive. They do
  not change `v`.
- A reader skips an operation kind it does not know, keeps an item of a kind it does
  not know without drawing it, and ignores fields it does not know. Because the file
  is never rewritten, all of them stay on disk.
- `v` goes up only when an old reader would misread the file (for example, a new
  coordinate unit). A new kind of thing is a new `op`, never a new meaning for an old
  one, as with CLI verbs (section 7).
- A reader that meets a `v` above its own fails with `canvas_too_new` (hint: upgrade
  slinger) and never writes to that file.
- A reader that meets a `v` below its own may upgrade the file: write the whole file
  in the new version to a temp file, `fsync`, rename it over the old one.

### Writing

- A new canvas is written as its header to a temp file, `fsync`ed and renamed into
  place, so a canvas file always has a whole header.
- Each change is one `write` of its whole line(s) to a file opened for append, then
  `fdatasync`. The daemon replies to the client only after that. One `apply` request
  with several operations is one write and one sync.
- Only the daemon writes (K5).

### Reading and damage

- Header missing or not a canvas header: `canvas_corrupt` (line 1).
- Last line without `\n`, or a last line that does not parse: a torn append. The
  reader cuts the file back to the end of the last whole line, says so on stderr, and
  opens the canvas. Nothing acknowledged was lost.
- Any earlier line that does not parse, or an `n` that is not the one expected:
  `canvas_corrupt` with the line number (hint: the file was changed outside slinger;
  restore it from backup). The daemon does not open or write that canvas.
- An operation that refers to an item or page that does not exist is applied as a
  no-op (B4). It is not damage.

### What the codec seed (slinger-1923) builds

- Encode and decode of the header and each operation, byte for byte as above.
- A reader over a whole file that returns the header, the operations, and how many
  torn bytes were cut, or a typed error (`canvas_corrupt` with a line, `canvas_too_new`).
- Unknown operations come back as a kept, opaque variant, not an error.
- Tests: round trip of every kind; a torn last line; a bad middle line; a gap in `n`;
  a header with `v` 2; an unknown `op`; an unknown field; a file with `first` above 1.

### Not in v1

- Compaction. Add it when a real canvas file is several times its live size. The
  `first` header field already makes room for it.
- A persisted undo stack (see shared choices).
- Images (but for PDF pages, the `pdf` kind), frames, rotation (SPEC section 4).
