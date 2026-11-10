# Packet format

Status: decided 2026-11-10 (slinger-9857). Pick: **B, one text document in
reading order**. The core builds it (slinger-8e05, slinger-6ebe); `canvas read`
serves it (slinger-a318).

Sources: SPEC.md section 6 (R1 to R7), D8, D9, B5, B6, and the Spike B
result in section 14. The spike code (`slinger-spikes/packet`) was not on this
machine when this was written, so the evidence used is what SPEC.md section 14
records (B9 says that is where spike evidence lives).

## What a packet must carry

- **Images of new ink**, one per cluster (R1, R3), at a fixed scale. A
  cluster too big for one image is split into overlapping tiles (Spike B
  finding 3).
- **An index**: one line per new item: id, box, kind, what it points at (R4).
  Items an agent made are text, never pixels (R2).
- **An outline** of the rest of the page, one line per item (R5).
- **A version** (B5). No raw stroke points (R6). Stored as a blob (R7).

Spike B numbers both alternatives are measured against: the whole board
"idea" (5 pages, 204 items) was 220 crops at scale 0.35 plus a text index,
about 57 000 tokens: 38 000 image (about 173 per crop) and 19 000 text (about
45 per line over about 424 lines). The screenshots it replaces cost 129 000.

## The example page

Both examples encode the same small page. Since the last sling (canvas ops 41
to 58) the user wrote a black note and a red note; the red note has an arrow
to text the agent wrote before. Older ink, a box and a text item are below.

## Alternative A: a JSON manifest plus image blobs

One JSON object. Images are separate PNG blobs named from the manifest.

```json
{
  "v": 1,
  "canvas": "idea",
  "page": "fable-5.2",
  "from": 41,
  "to": 58,
  "scale": 0.35,
  "new": [
    { "id": "c42", "kind": "ink", "box": [120, 80, 520, 140], "colour": "black", "strokes": 23, "images": ["c42.png"] },
    { "id": "c51", "kind": "ink", "box": [120, 260, 300, 90], "colour": "red", "strokes": 9, "to": "t12", "images": ["c51.png"] }
  ],
  "made": [
    { "id": "t12", "kind": "text", "box": [480, 240, 360, 120], "by": "claude", "text": "Packet: images of new ink plus a text index." },
    { "id": "a13", "kind": "arrow", "box": [420, 280, 60, 20], "by": "claude", "from": "c51", "to": "t12" }
  ],
  "page_outline": [
    { "id": "c3", "kind": "ink", "box": [40, 1200, 900, 400], "colour": "black", "strokes": 210 },
    { "id": "c17", "kind": "ink", "box": [1000, 1240, 560, 220], "colour": "blue", "strokes": 64, "in": "b9" },
    { "id": "b9", "kind": "box", "box": [980, 1200, 620, 300], "by": "claude" },
    { "id": "t8", "kind": "text", "box": [40, 1700, 700, 60], "by": "claude", "text": "Spike B: 44% of the screenshot cost." }
  ]
}
```

- **Token cost.** 1073 characters pretty, 866 minified: about 216 to 268
  tokens at 4 characters a token, 27 to 34 per item line. Punctuation-heavy
  JSON tends to tokenize worse than that, so this is a floor. For the board:
  424 lines × 30 ≈ 12 700 text + 38 000 image ≈ **51 000 tokens, 39%** of the
  screenshots.
- **Delivery.** In a sling the manifest goes as one text block and the images
  follow as image blocks, each with a label (`c42.png`) the agent must match
  back to the manifest. Through the CLI the agent reads the JSON, then each
  file.
- **Crop.** `slinger canvas crop idea c51`, or `--box 120,260,300,90`.
- **Version change.** Read `v`. Unknown keys are ignored; a breaking change
  raises `v`. Standard JSON parsers do the work.

## Alternative B: one text document in reading order

Plain text, one line per item, with image references on the lines where the
image belongs. The agent reads it top to bottom like a page.

```
slinger-packet 1 canvas=idea page=fable-5.2 from=41 to=58 scale=0.35
# new
c42 ink 120,80 520x140 colour=black strokes=23 img=c42.png
c51 ink 120,260 300x90 colour=red strokes=9 to=t12 img=c51.png
# made
a13 arrow 420,280 60x20 by=claude from=c51 to=t12
t12 text 480,240 360x120 by=claude "Packet: images of new ink plus a text index."
# page
c3 ink 40,1200 900x400 colour=black strokes=210
b9 box 980,1200 620x300 by=claude
c17 ink 1000,1240 560x220 colour=blue strokes=64 in=b9
t8 text 40,1700 700x60 by=claude "Spike B: 44% of the screenshot cost."
```

- **Token cost.** 552 characters: about 138 tokens, about 17 per line, half
  of A. For the board: 424 lines × 17 ≈ 7 200 text + 38 000 image ≈ **45 000
  tokens, 35%** of the screenshots.
- **Delivery.** In a sling the text is cut after each line that has `img=`
  and the image block goes right there, so each image sits under its own
  index line with no label to match. Through the CLI the agent reads the text
  and opens each file.
- **Crop.** The same verb as A: `slinger canvas crop idea c51`.
- **Version change.** Read the second word of line 1. Unknown sections,
  kinds and fields are skipped; a breaking change raises the number. The
  grammar below is small enough that the parser is one function.

## Pick: B

1. The reader is an agent: a page in reading order, each image under its line (P2).
2. About half A's text tokens (17 against 27+ per line); text is a third of a packet.
3. No labels to match images back to a manifest, so no step where crops get mixed up.
4. The grammar is strict and versioned (B5); programs parse it with one function.
5. A wins only for generic tooling, and `--json` still wraps B for that.

## The format, exactly

Text, UTF-8, `\n` line ends, no trailing spaces. Tokens on a line are split by
one space. Coordinates are canvas units, integers (round to nearest), origin
at the page's top left, y down.

```
packet  = header "\n" section*
header  = "slinger-packet" SP version (SP field)*
section = "# " name "\n" line*
line    = id SP kind SP box (SP field)* [SP quoted] "\n"
box     = x "," y SP w "x" h
field   = key "=" value        ; value has no space, no quote
quoted  = '"' text '"'         ; escapes: \" \\ \n
```

**Header fields**, in this order: `canvas` (canvas id), `page` (page name),
`from`, `to` (the canvas op range this packet covers; `from` is the first op
after the last sling's `to`, so R1 is "ops in from..=to"), `scale` (the
render scale, `0.35`), then `at` (see Delivery). `version` is `1`.

**Sections**, always in this order, each always present, even when empty:

| Section | Holds |
|---|---|
| `new` | ink clusters with any stroke in `from..=to`, in reading order |
| `made` | items an agent made or changed in `from..=to`, in reading order |
| `page` | every other item on the page, in reading order |

Reading order: by row, then left to right. Two lines are in the same row when
their boxes overlap vertically by at least half the shorter height. Rows are
formed from the top: the topmost line not yet placed starts a row, and every
line that overlaps it that much joins the row.

**Kinds**: `ink` (a cluster of strokes), `text`, `box`, `arrow`. Ink in the
`page` section is clustered the same way as new ink, and has no image. A
stroke an agent drew is never clustered and never an image (R2): it is an
`ink` line of one stroke, with `by`.

**Ids**: canvas items keep their canvas item number behind a letter for their
kind (`t12`, `b9`, `a13`). A cluster's id is `c` plus the item number of its
earliest stroke, so it stays the same across packets while that stroke does,
and no two lines in a packet share a number. An arrow end bound to a stroke
names the stroke's cluster.

**Fields** (order as listed; each only when it applies):

| Field | On | Meaning |
|---|---|---|
| `colour` | ink | the cluster's stroke colour (clusters never mix colours): `black`, `white`, `red`, `green`, `blue`, `yellow`, `orange`, `purple` or `grey` for those exact CSS colours, else `#rrggbb`, or `#rrggbbaa` when not opaque |
| `strokes` | ink | number of strokes |
| `by` | made items | the agent's name |
| `from`, `to` | arrow | ids at the arrow's ends, when bound |
| `to` | ink | the id an arrow drawn in this cluster points at: the first arrow whose start is bound to one of its strokes |
| `in` | any | the id of the smallest other box that contains this item's box |
| `img` | ink in `new` | image file names, comma-separated, tiles in reading order |

The quoted text is last, only on `text` lines: the full text in `new` and
`made`, the first 80 characters plus `…` in `page`.

**Images**: PNG, rendered at `scale` (pixel = canvas unit × 0.35, rounded
up), ink in its own colour on white, with a 16 canvas unit margin. Names are
`<id>.png`. A cluster whose image has a long edge over 1568 px is cut into
tiles of at most 1568 px a side that overlap by 64 px, named `<id>-1.png`,
`<id>-2.png`, ... in reading order.

**Nothing new** (B4): both `new` and `made` are empty and `from` is greater
than `to`. It is a valid packet.

## Delivery and storage

- **Record (R7).** The packet text is one blob, each image is one blob, and
  the `sling sent` event's payload lists the text hash and each image name
  with its hash. The stored text never has `at`, so the hash depends only on
  the page.
- **Sling.** The server sends the text as content blocks, with each image
  block placed after the line that names it.
- **`slinger canvas read`.** The daemon writes the text and the images into
  one directory and adds `at=<that directory>` to the header, so `img=` names
  resolve as `<at>/<name>`. `--json` returns `{ "packet": <hash>, "dir":
  <path>, "text": <the text> }`.
- **`slinger canvas crop <canvas> <id>`** or **`--box x,y,w,h`**: one image
  (or tiles) at the same scale, by the same rules. A cluster id resolves
  through the record's latest packet for that canvas.
- **`slinger canvas strokes <canvas> <id>`**: raw points, the R6 escape hatch.

## Versioning (B5)

- Within version 1, changes only add: a section, a kind, a header field or a
  line field. Readers skip unknown sections, unknown kinds and unknown
  fields.
- Removing or renaming anything, changing a field's meaning or the box
  syntax, or reordering sections is version 2. A program that reads a version
  it does not know fails with `unsupported_version`.
