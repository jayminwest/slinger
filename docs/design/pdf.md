# PDF support

Status: decided 2026-11-12 (slinger-10ab9d, plan pl-d2285b, parent
slinger-af3f3a). Amends SPEC D6, section 4, O8 and B6, and adds R8. Inputs: D6, D8, D19,
P7, R1 to R7, B1, B4, B5, B6, O8, and the plan's alternatives (a PDF pane, image
snippets, the agent reading the blob, pages rendered at import, the daemon
asking the app, one canvas page per PDF page: all rejected by the owner).

The later steps follow this page literally. Where it says "must", a step that
cannot do it stops and reports; it does not pick another way.

## The idea in one paragraph

A PDF is not a pane and not a document type. It is one new canvas item kind,
`pdf`: a frame on a canvas page that shows one page of a PDF, or a part of one
(a crop). The PDF's bytes are a blob in the record (D14, D19). Opening a PDF
makes a new canvas whose first page holds one `pdf` item per PDF page, stacked
top to bottom; the user writes on it with the ordinary tools. A snippet is the
same item with a crop, copied and pasted through the clipboard path that exists.
Only the macOS app can read a PDF. The core and the daemon never parse one: to
them a `pdf` item is a frame, a blob id and numbers. At sling time the app
renders the parts of the PDF that lie under the new ink and pulls out their
words; the core draws the ink over those pixels and puts the words in the
packet.

## Steps and who owns what

| Step | Seed | Owns |
|---|---|---|
| 1 | slinger-10ab9d | this page, the SPEC amendment |
| 2 | slinger-02e4e9 | core: `Shape::Pdf`, the file codec, geometry, the packet line; `canvas-file.md`, `packet.md` (the item line) |
| 3 | slinger-693cf2 | app: `platform/pdf.rs` (drawing, page sizes), the blob cache, the `open-pdf` command |
| 4 | slinger-405914 | app: reader mode (scene and layout only) |
| 5 | slinger-82bd78 | app: the `crop-pdf` tool, the cropped item on the clipboard |
| 6 | slinger-f20d48 | core, control, daemon, app: `sling_backdrops`, `backdrops` on `sling_send`, render over a backdrop, words in the packet; `packet.md` (backdrops, words), `service.md` (the verbs) |
| 7 | slinger-604081 | the guide, CHANGELOG, ARCHITECTURE, `service.md` and `cli.md` prose |

Each step that changes a contract updates `ARCHITECTURE.md` in the same commit
(B10). Step 7 fills in what is left.

## Units and coordinates

- **Canvas units** as everywhere: stored as integer tenths in the canvas file
  and on the wire, `f64` units in the core.
- **Page points.** A PDF page is measured in PDF points (1/72 inch) of its
  crop box, **as displayed**: the page's `/Rotate` applied, origin at the
  top-left, y down. Stored as integer tenths of a point, like canvas units.
  `platform/pdf.rs` is the only code that converts to PDFKit's or
  CoreGraphics' bottom-left, unrotated space. Nothing else knows that space.
- **Shown region.** The part of the page an item shows: its `crop`, or the
  whole page `[0, 0, pagesize]` when it has none. The item draws its shown
  region stretched to fill its frame exactly. The tools that make and resize
  `pdf` items keep the frame's aspect equal to the shown region's, so the
  stretch is uniform; the core does not check aspect (rounding to tenths would
  make an exact check fail).
- **Mapping.** A canvas point `p` in the frame is page point
  `shown.x + (p.x - frame.x) * shown.w / frame.w` (and the same for y). This
  arithmetic needs no PDF, so the core, the app's scene and the crop tool each
  do it from the item alone.

## The item (step 2)

### In the canvas file

One more row of the item table in `canvas-file.md`. Keys in this order:

```
{"id":"i3","page":"p1","kind":"pdf","x":0,"y":0,"w":16000,"h":20706,"hash":"sha256:9f86…","pdfpage":1,"pagesize":[6120,7920],"name":"paper.pdf"}
{"id":"i9","page":"p1","kind":"pdf","x":24000,"y":3000,"w":6000,"h":2400,"hash":"sha256:9f86…","pdfpage":3,"pagesize":[6120,7920],"crop":[720,1800,3000,1200],"name":"paper.pdf"}
```

| key | type | meaning |
|---|---|---|
| `x`, `y`, `w`, `h` | tenths | the frame; `w` and `h` above 0 |
| `hash` | blob id | the PDF's bytes in the record |
| `pdfpage` | integer ≥ 1 | the page number in the PDF, from 1 (as a reader says "page 3") |
| `pagesize` | `[w, h]`, tenths of a point, both above 0 | the whole page as displayed |
| `crop` | `[x, y, w, h]`, tenths of a point, optional | the shown region; inside `[0, 0, pagesize]`, `w` and `h` above 0. Absent: the whole page |
| `name` | string, optional | the PDF's file name when it was opened, without its directory: 1 to 255 bytes, no `/`, no control character |

- No `color`: like `live` and `button`, a `pdf` item has none.
- `pagesize` is stored, not read from the PDF, so every mapping above is
  arithmetic, the core can say what a crop shows without a PDF, and a canvas
  whose blob is missing still has the right shapes.
- Additive, no change to `v`: a reader from before the kind keeps the `add` as
  a line it does not know, its id counter still moves past the id, and the
  app before this change draws nothing for it (`mirror::read` returns `None`
  for a kind it does not draw; step 2 adds a test that says so).

### In the core

```rust
/// One page of a PDF, or part of one (docs/design/pdf.md). The core never
/// reads the PDF: it knows the blob id and the numbers below.
Shape::Pdf {
    frame: Rect,               // canvas units
    hash: String,              // a BlobId as text, as Live's
    pdf_page: u32,             // from 1
    page_size: (f64, f64),     // page points, as displayed
    crop: Option<Rect>,        // page points; None: the whole page
    name: Option<String>,
}
```

- `Shape::check`: the frame as `Rect::check`, with `w` and `h` above 0;
  `hash` parses as a `BlobId` (format only: the daemon does not look the blob
  up, as for `live`); `pdf_page >= 1`; both sizes finite and above 0; a crop
  finite, `w` and `h` above 0, inside the page; `name` as above. Each failure
  is `invalid` with a hint.
- A method `Shape::shown(&self) -> Option<Rect>` (the shown region) is the one
  place the "crop or whole page" rule lives.
- `bounds()` is the frame.
- The core never reads, parses or opens a PDF, and adds no PDF crate. The
  `core-is-pure` rule already forbids the IO; review forbids the parsing.

### Geometry

- **Draw order.** `pdf` items draw under every other kind, among themselves in
  item order. They are the paper: ink drawn before a snippet was pasted still
  shows over it. The core's render, the app's scene and every hit test use
  this order.
- **Hit at a point.** A `pdf` item is hit inside its frame, after every other
  kind (anything drawn over it wins).
- **Eraser.** Never touches a `pdf` item (as `live` and `button`).
- **Lasso.** Takes a `pdf` item only when the lasso holds its whole frame.
- **Move** as any framed item. **Resize** keeps the aspect: the select tool's
  handles scale a `pdf` item uniformly (`scene/select/scale.rs`), whichever
  handle is dragged.
- **Agents** may not change a `pdf` item the user made (the author rule). An
  agent cannot make one in the first cut: `canvas put` has no `pdf`, and
  `canvas apply --agent` of one is allowed by the rules but is not in the
  guide.

### The packet line

Kind `pdf`, id letter `d` (`d3`): no other kind uses it. Fields after the box,
in this order, each only when it applies: `by`, `pdfpage`, `crop` (whole page
points, `x,y,w,h`), `name`, `blob` (the blob id), `in`. Never an image (R8).

```
d3 pdf 0,0 1600x2071 pdfpage=1 name=paper.pdf blob=sha256:9f86…
d9 pdf 2400,300 600x240 pdfpage=3 crop=72,180,300,120 name=paper.pdf blob=sha256:9f86…
```

- An ink line gets a new field `over`: the ids of the `pdf` items whose frame
  its box overlaps, in draw order, comma-separated. It goes after `in` and
  before `img`. The core knows this from the canvas alone, so a sling from the
  CLI, with no backdrops, says it too.
- **Which `pdf` lines appear.** `new` never holds them (it is ink only).
  `made` holds one an agent made. `page` holds a `pdf` item only when it is
  selected or an ink line of the packet names it in `over`. A 300-page PDF
  must not cost 300 lines a sling; `canvas show` lists them all.
- `pdf` is a kind an older packet reader skips (B5): no version change.

### What the core renders without a backdrop

`canvas crop`, and a sling with no backdrops, draw a `pdf` item as its frame
in 2-unit grey lines (`#999999`) with `pdf p<pdfpage>` in the bitmap font at
its top-left, as a live item shows its name. Step 6 adds the backdrop; an item
with a backdrop in an image draws the backdrop and no label there.

## Drawing in the app (step 3)

- `crates/app/src/platform/pdf.rs` holds every PDFKit and CoreGraphics call and
  the only new `unsafe`. New objc2 crates or features (`objc2-pdf-kit`, the
  `CGPDF*` features of `objc2-core-graphics`) go in the root `Cargo.toml`.
  Measure `check:binary-size` before and after; if it fails, stop and report
  (the budget does not move).
- Its safe functions, and nothing else of PDFs leaves the platform:
  - `pages(path) -> Result<Vec<(f64, f64)>, PdfProblem>`: each page's size in
    page points, as displayed. A locked (password) or unreadable PDF is a
    problem with a message for the user.
  - drawing a `pdf` item into the canvas view: its shown region into its frame,
    clipped to it, white under it.
  - `backdrop(..)` for step 6 (below).
- `scene::Shape` gets a `Pdf` variant with the same fields as the core's; the
  mirror reads and writes the object above (`mirror.rs`, `mirror/write.rs`).
- **The bytes.** A PDF is drawn from a file the app fetches once per blob id
  with `blob_get`, on a thread of the backend, into `pdf/<hex>.pdf` in the
  app's own directory (where `backend/ui.rs` writes UI code). Until the file is
  there the item draws as a light grey frame with "PDF" in it; a blob that
  does not come (`not_found`, O8, or slingerd not read) stays that way and
  is asked for again the next time a pane draws it, after a wait that
  doubles with each miss from 1 s to 64 s (slinger-e0699c).
- **Smooth pen (P7).** Draw only the `pdf` items that touch the visible
  rectangle. Drawing a page must never delay a stroke: if vector drawing of a
  visible page costs more than a frame, cache a bitmap of it per item at the
  current zoom, made off the pen's path. Opening a document and reading page
  sizes happen once per blob id, not per draw.
- AppKit drawing stays on the main thread; CoreGraphics rendering into a bitmap
  context (the backdrop) may run on any thread with its own `CGPDFDocument`.

### Opening a PDF

A command `open-pdf` ("Open PDF…", group File) in the command registry (D21),
so the key overlay, the palette and the menu bar show it. It:

1. shows the open panel for one `.pdf` file (`platform`, main thread); the
   panel also takes a `.tldraw` or `.tldr` file, which the backend thread
   makes a canvas with `canvas_import` instead of the steps below, under the
   canvas id below without `.pdf` (`my-board`, then `my-board-2`:
   slinger-8d8e97), then opens the same pane (slinger-99061d);
2. on a backend thread: `pages(path)`; `blob_put(path)`; `canvas_create` with
   the canvas id below and `path`: a file of the `add`s on `p1`, in page order
   (a request line holds 65536 bytes, a long PDF's `add`s more), so the canvas
   is made whole or not at all (slinger-39c3af); then `pane_open` of the
   canvas beside the focused pane in the window's workspace;
3. shows a failure as the problem in the pane, with nothing half made but the
   blob (a blob with no item costs nothing).

A drop on the window is the other way in (slinger-fd53aa): `platform` takes
file URLs dragged onto the window and hands them on with the drop's point;
`window::open_pdf` focuses the pane under that point, takes the first file
whose extension is `pdf` (any case) and runs steps 2 and 3 as above. A drop
with no PDF is refused (the drag slides back).

- **Canvas id.** The file's stem, lowercased, each character outside
  `a-z 0-9 - _` made `-`, cut to 56 characters, then `.pdf`: `Paper (v2).pdf`
  is `paper--v2-.pdf`, and a stem left empty is `pdf`. When `canvas_create` says `conflict`, try `-2`, `-3`,
  … before `.pdf`. **A canvas whose id ends in `.pdf` is a PDF's canvas**:
  reader mode keys on this and nothing else (below).
- **Stacking.** All on the canvas's first page, `p1`. One scale for the whole
  document: `k = PAGE_WIDTH / (the widest page's width in points)`, with
  `PAGE_WIDTH = 1600` canvas units. Page `i` gets `w = width_i * k`,
  `h = height_i * k`, `x = (PAGE_WIDTH - w) / 2`, and `y` = the previous page's
  bottom plus `PAGE_GAP = 40` units (the first at `y = 0`). This is pure
  arithmetic in an app module outside `platform` (the app does not link the
  core), tested there. The `add`s carry `name`, no `crop`.
- 1600 units: at reader mode's fit-to-width a pane 1300 to 1900 points wide
  shows the page at a zoom near 1, so handwriting on a PDF has the same size in
  canvas units as on any canvas, and clustering and the packet scale (R3) work
  unchanged.

## Reader mode (step 4)

- A pane showing a canvas whose id ends in `.pdf` is in reader mode, on every
  page of that canvas. The test is one pure function over the canvas id in the
  scene or layout modules; `platform` never decides it.
- **Fit to width.** The zoom makes the column of `pdf` items (the union of
  their frames' x extents on the page shown) fill the pane's width, with 16
  points each side. Recomputed when the pane resizes.
- **Locked vertical scroll.** Scroll and the trackpad move the page up and
  down only; x stays at the fitted column; pinch zoom does nothing. Scrolling
  stops at the column's top and at its bottom.
- **The paper is fixed.** On a PDF's canvas, the select tool and the lasso do
  not take its pages (`pdf` items with no `crop`), so a tap or a drag on a page
  never moves it. A snippet pasted there (a `pdf` item with a `crop`) is taken
  as on any canvas (slinger-f6d05b). Every
  tool works over the pages as on any canvas; pages stack and undo as usual.
- Nothing else changes: no page thumbnails, no PDF navigation, no new chrome.

## Snippets (step 5)

- A tool `crop-pdf` (in the registry; its key is cmd+8, in
  `config.example.toml`). A drag draws a box; the lift intersects it with the
  topmost `pdf` item under the press and puts the cropped item on the clipboard as the clip format writes it
  (`mirror/clip.rs`, a one-object JSON array): the same `hash`, `pdfpage`,
  `pagesize` and `name`, `crop` = the box mapped to page points (the mapping
  above, from the source's shown region, so a crop of a crop works), frame
  `w`, `h` = the box's size. The canvas does not change. A box that touches no
  `pdf` item does nothing.
- Paste is the existing paste: the object lands centred in the focused pane, on
  any canvas of this daemon, and is an ordinary `add`. On a canvas of another
  machine's daemon the blob is missing and it draws grey (O8).
- The crop arithmetic is a pure function outside `platform`, tested with real
  numbers: whole page, a crop, a crop of a crop, a box past the frame's edge.

## Sling: the page under the ink (step 6)

The core clusters ink and decides which images a packet has; only the app can
render a PDF. So the app must render parts of PDF pages that the daemon picks.

### What a backdrop is

A **backdrop** is pixels of one `pdf` item over one canvas rectangle (its
`area`), at the packet scale (`render::SCALE`, 0.35 pixels a unit), plus the
words of the PDF in that area.

- **Pixels: raw RGB, not PNG.** A file of exactly `width * height * 3` bytes:
  8-bit R, G, B, rows top to bottom, no header, no alpha (the page on white).
  `width` and `height` are given by the daemon (below) by the render's own
  rounding, so the app does no rounding of its own. The core reads it with no
  decoder: nothing to get wrong about filters, colour types or ICC chunks, and
  no PNG decoder joins the core. A file of another length is `invalid` (hint:
  a backdrop is width × height × 3 bytes). Files are local and short-lived, so
  their size (a whole page at 0.35 is about 1.2 MB) does not matter. **Step 6
  must not add a PNG decoder.**
- **Words: JSON.** A file holding one array, in the PDF's reading order (as
  PDFKit gives it), of every word whose box overlaps the area:
  `[{"text":"Packet","x":2410,"y":1033,"w":402,"h":118}, …]`, boxes in canvas
  tenths. A word is a run without whitespace. A scanned page has no words: the
  file is left out.

### The two alternatives (B6)

**A. Whole pages, one request.** `sling_send` takes a backdrop for every `pdf`
item the sling might need; the daemon crops each cluster's image from them.

- The app does not know which ink is new: the marker lives in the record and
  "what changed after it" is the core's (`canvas/delta.rs`). To be right it
  must send every `pdf` item on the page that has any of the user's ink over
  it, or copy the core's delta and clustering into the app.
- Cost grows with the document, not the sling: twenty marked pages send about
  24 MB of pixels and about 600 KB of words through files, every sling, to
  build a packet that uses a few clusters of them. A cache in the app (by blob,
  page, frame) saves the render but not the files or the daemon's reads.
- A list of hundreds of backdrops can pass the 65536-byte request line.
- No new verb: `sling_send` gains one optional argument.

**B. Ask for the areas, then sling.** A new verb `sling_backdrops` returns, for
the sling that would go now, the area of each `pdf` item under each new
cluster's image; the app renders only those and passes them to `sling_send`.

- The core alone decides which pixels matter (clusters, their image frames,
  the marker). The app renders the rectangles it is told: no domain logic in
  it.
- Cost grows with the new ink: a handwritten note's image is tens of thousands
  of pixels (about 30 KB raw), and only the words around it travel.
- The request stays small: one entry per (new cluster, `pdf` item) pair.
- A verb more (four places, "Adding a verb"), one more local round trip
  (milliseconds), and a gap between the two requests in which the pen may add
  ink. The gap is defined away: a backdrop is only pixels for an area, so the
  render uses whatever backdrops cover each image and draws white where none
  does; ink added in the gap shows on white. The sling is never refused for it.

Both send the agent the same images: the daemon crops to each cluster's image
either way, so the token cost per sling is the same (plan risk 3 is about
pixels moved on this machine, not tokens).

### The pick: B

1. It keeps one decision in one place (PHILOSOPHY: hide information, one core
   with thin front ends). In A the app must guess, or duplicate, R1 and the
   clustering.
2. Its cost follows the sling, not the document. A's grows with every page
   the user has marked.
3. Its request is bounded; A's can pass the line limit on a long document.
4. Its one cost, a verb and a round trip, is paid once in code and in
   milliseconds per sling. The race it opens is defined out of existence
   (B4), not handled.

**Owner check:** this reverses the plan's lean (A), and the owner has not
confirmed it yet. If the owner prefers A, only the request shape below
changes; the backdrop, its file formats and the render stay.

### The verbs

`sling_backdrops`

| Arguments | Result |
|---|---|
| `canvas`: canvas id, `page`: page id or name (optional, default the first page) | `backdrops`: list of `{item, hash, pdfpage, pagesize, crop, frame, area, width, height}` |

- One entry per pair of a cluster in the packet's `new` section and a `pdf`
  item whose frame overlaps that cluster's image frame (its box plus
  `render::MARGIN`). `area` = that image frame ∩ the item's frame. Entries for
  one item whose areas overlap are not merged (simple, and the words dedupe
  below). Order: by item in draw order, then by cluster in reading order.
- `item` as `canvas show` names it (`i3`); `hash`, `pdfpage`, `pagesize`,
  `crop` (`null` for none) as stored; `frame` and `area` `[x, y, w, h]` in
  tenths; `width`, `height` the backdrop's pixels (`render::pixels` of the
  area's size).
- Nothing new, or no ink over a PDF: an empty list (B4). The canvas and page
  fail as in `canvas_read`.
- The core function is `packet::backdrops(log, ask) -> Vec<Area>`, which
  clusters as `packet::build` does but renders nothing.

`sling_send` gains `backdrops` (optional): a list of
`{item, area, image, words}`: `item` and `area` as `sling_backdrops` gave
them, `image` the absolute path of the raw RGB file, `words` the absolute path
of the words file (optional). The app writes both in a directory of its own
and removes them after the reply, as the CLI does for `blob put -`.

- A backdrop whose item is gone, is not a `pdf` item, or is on another page is
  skipped (B4). A path that cannot be read is `not_found`; a wrong length or
  words that do not parse is `invalid`. Each way nothing is stored.
- The backdrops themselves are not stored: the packet's images (with the page
  in them) and text (with the words) are, as before (R7).
- The CLI's `slinger sling` sends none (only the app can render). Its packet is
  still valid: ink on white, `over=` naming the pages, no words.

### In the app

- `backend/sling.rs`: before `sling_send`, `sling_backdrops`; for each entry,
  the renderer writes the two files; then `sling_send` with them. A failed
  render leaves that backdrop out; the sling still goes.
- The backend may not call `platform` (`app-logic-is-pure`). `main.rs` hands
  the `Slinger` a plain function, `platform::pdf::backdrop`, when it starts
  it; the app's tests hand it one that writes a solid colour.
- `platform::pdf::backdrop(entry, blob file, out dir)`: render the area of the
  item (its shown region mapped as above) into a CoreGraphics RGB bitmap of
  exactly `width × height`, white first; write the raw bytes. Words: PDFKit's
  characters on that page with their bounds, split at whitespace into words,
  mapped to canvas tenths, those overlapping the area, in the page's order.
  It runs on the sling thread with its own documents; never on the main
  thread, never with AppKit.

### In the core's render and packet

- `render::Backdrop { item: ItemId, area: Rect, width: u32, height: u32,
  rgb: Vec<u8>, words: Vec<Word> }` and `render::Word { text: String,
  frame: Rect }`. `packet::Ask` gains `backdrops: &[Backdrop]` (empty today).
- **Drawing.** For each image (tile) of a cluster, the raster is filled white,
  then each backdrop whose area meets it, in its item's draw order, clipped to
  the item's frame ∩ area: each output pixel takes the backdrop pixel nearest
  its centre's canvas point. Then the ink, as now. `canvas crop` passes no
  backdrops.
- **Cutting tiles to ink and folding specks look at ink only.** Decide each
  image's part from the ink alone, as now; then paint the backdrop under it.
  A backdrop never makes a tile worth an image.
- **Words.** A word belongs to a cluster when the word's centre lies inside the
  cluster's box (its strokes' bounds) grown on every side by half the word's
  height: a circle or a strike-through takes the words in it, an underline the
  words over it and not the line below. Words from several backdrops are taken
  in backdrop order, then file order; a word equal in text and box to one
  already taken is dropped. Joined by single spaces, at most 2000 characters,
  then `…`.
- **The line.** An ink line in `new` with words ends in them, quoted:
  `c42 ink 120,80 520x140 colour=red strokes=3 over=d3 img=c42.png "the exact words circled"`.
  The grammar already allows a quoted text on any line, and `Entry::parse`
  reads it; `packet.md` changes "only on text lines" to say so. `page` ink
  lines never carry words.
- **The note.** `sling::INTRO` gains one sentence: a `pdf` line is a PDF page
  (`blob get` fetches the file), and a quoted text on an ink line is the PDF's
  words under that ink.
- Tests: a backdrop under a cluster shows in its image under the ink; a tile of
  backdrop and no ink is still left out; words picked by the centre rule
  (circle, underline, the line below); dedupe; the 2000-character cut; a wrong
  length is `invalid`; a CLI-style sling with no backdrops.
- Measure before closing step 6, as Spike B did: tokens per sling for a circled
  sentence on a PDF, with and without backdrops, written into `packet.md`.

## What no step may do

- Parse, open or inspect a PDF in `crates/core`, `crates/daemon`,
  `crates/control` or `crates/cli`, or add a PDF crate to them.
- Put PDF pixels in the record at open time, or render every page anywhere
  but on screen.
- Send a whole PDF page to the agent as an image, or the PDF file itself, in a
  sling. The agent may fetch the blob itself.
- Add a PNG decoder to the core.
- Call PDFKit or CoreGraphics outside `crates/app/src/platform/pdf.rs`, or
  decide reader mode, stacking or crop arithmetic inside `platform`.
- Make reader mode depend on anything but the canvas id.
- Change `v` of the canvas file or the packet: every change here is additive.

## Not in the first cut

- A PDF from the CLI (`canvas put` or a `canvas pdf` verb): the CLI has no way
  to read page sizes. An agent can `canvas apply` one if it knows them.
- Backdrops for `canvas crop`, so an agent's crop over a PDF shows the page.
- Dropping a PDF file on the window; search, links, outlines, annotations,
  forms; a PDF on a second machine (O8).
