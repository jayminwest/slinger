# Pen-less input: the mouse's press and the text editor

Design for slinger-755df3, step 1 of plan pl-c5e2a3 (pen-less use: keyboard
and mouse as a full way in). Two questions, two alternatives each, then the
pick that tonight's seeds build on (slinger-ea308d, the caret in a canvas
text; slinger-9c9553, the pointer).

The pen stays the primary input. Both picks keep the rule: **a tablet pen
does what it does today**, the same strokes, taps and tools.

## Where things stand (2026-11-20)

- `platform/input.rs` turns every left-button event into an `Event::Press`,
  `Drag` or `Lift` carrying a `Pen`. It already knows a tablet event from a
  mouse event (`pen()`: subtype `TabletPoint` or `TabletProximity`) but only
  keeps that to tell the eraser end. `Pen` has `touch` (a palm: never draws)
  and `eraser`; it has no "tablet" bit.
- So a mouse or trackpad press does whatever the chosen tool does. The
  default tool is the pen: a mouse drag draws ink.
- Pan today: a two-finger scroll or wheel, a right-button drag
  (`input.rs` keeps `RIGHT` and sends `Event::Scroll`), alt+arrows
  (`pan-left` and so on). Zoom: pinch, cmd/ctrl+scroll, cmd+= / cmd+-.
  A middle button does nothing; space does nothing outside a text.
- A canvas text is append-only (`scene/shapes.rs`, `Shapes::typed`): the
  caret is after the last character, delete pops the last one, the arrow
  keys (Unicode private range `U+F700..U+F8FF`) are dropped. A paste,
  and Flick's dictation (a paste), goes in at the end (`Shapes::pasted`).
  The caret is drawn by `platform/canvas.rs` (`Mark::Caret`, `end_of`)
  from `NSLayoutManager`: always the end.
- While a text is typed, `Canvases::takes` passes on every chord without
  cmd or ctrl, so it comes back as `Event::Text` with its modifiers lost.
- The sling's entry, a feedback note and a page's new name use a native
  `NSTextField` (`platform/entry.rs`), with the caret, input methods and
  dictation of any Mac field.

## Question 1: what a mouse or trackpad press does with no pen near

"A mouse press" below means a left-button press whose event is not a tablet
event and not touch: a mouse, a trackpad click or a trackpad click-drag.

### 1A. The press does what the chosen tool does (as now)

Nothing changes in how a press is read. The mouse draws with the pen tool,
erases with the eraser, and so on. To move things, the user picks the select
tool (`tool-select`, cmd+7) and goes back to the pen (cmd+1) to draw.
slinger-9c9553 adds only the pans (below).

- For: zero risk to the pen; one rule for every device; nothing to learn.
- For: a trackpad user who wants to scribble can.
- Against: the plan's gap stays. The default tool is the pen, so the first
  drag a mouse user makes on a note is a stray ink line. Pen-less work is
  mostly typing, placing and moving, so the mouse user lives in the select
  tool and must remember to switch back whenever the tablet is used.
- Against: the tool a pen-less user wants and the tool a pen user wants are
  the same setting, so one undoes the other on a machine with both.

### 1B. A mouse press with the pen tool selects and moves

The pen tool is the tablet's. When the chosen tool is the pen and the
press is a mouse press, it acts as the select tool: a press on an item picks
it, a drag moves the selection, a drag on empty canvas draws a selection
box, shift adds or takes out, a double press on a text edits it. Every other
tool chosen on purpose (text, box, arrow, lasso, eraser, crop) does with the
mouse what it does with the pen: someone who picks the box tool wants boxes.

- For: a mouse user never makes a stray line, and needs no tool switch for
  the common case. A pen user who picks up the mouse for a moment can move
  things without leaving the pen tool.
- For: the tablet path is untouched: a tablet event always gets the chosen
  tool, and the decision is one function.
- Against: a mouse user who wants to scribble cannot with the pen tool.
  (Follow-up if asked: a `[pointer]` config key, `press = "select" | "tool"`.)
- Against: if a tablet event ever arrived without a tablet subtype, the pen
  would select instead of draw (plan risk 3). Wacom drivers and Sidecar's
  Apple Pencil send tablet subtypes; the owner hand check (slinger-7ada2a)
  confirms on the real tablet.
- Against: the footer and cursor must say what the press will do, which
  depends on the device, not only the chosen tool.

### Both: panning without a right button

Shared by 1A and 1B (slinger-9c9553 builds it either way):

- **Space-drag**: hold space and drag with the left button (mouse,
  trackpad, or pen) to pan the pane under the press, as a two-finger scroll
  there would. Only while nothing takes typed text: no canvas text being
  typed, no entry open, no pop-up, palette, key overlay or page menu open.
  Otherwise space types a space, as now.
- **Middle button drag**: pans as the right-button drag does today.

## Question 2: the editor for a canvas text

slinger-ea308d asks for: arrows, word and line jumps, shift-selection,
forward and word delete, select-all, cut and copy of selected characters,
paste and dictation at the caret.

### 2A. A pure editor in the scene

The text being typed holds the text, a caret and a selection anchor in a
new pure module. Keys move the caret; the scene computes the caret bar and
the selection rectangles from `glyphs` (the same measures every hit test,
the frame and the core's rows use) and draws them as items, as it draws the
caret, frame and hint today.

- For: one renderer. What is typed is drawn by the same code that draws it
  after it ends, in the core's rows, at any zoom, in a pane with a page
  turn or a pan under way. No second view to keep in place.
- For: testable. Every key is a pure function of (text, caret, anchor,
  layout); tests drive it as `shapes/tests.rs` does today. No AppKit in a
  decision (ARCHITECTURE, P2).
- For: builds on `Shapes::typed` and the paste path, which already work.
- For: likely small in binary: string slicing and a few rects; and the
  caret's `NSLayoutManager` lookup in `platform/canvas.rs` can go.
- Against: no input methods (marked text: Japanese, Chinese, Korean),
  no spell check, no system text services. Dead-key accents (alt+e, e)
  work: `NSEvent.characters` gives the composed character.
- Against: caret position is the measured width of the row's prefix, which
  can be off a point from the drawn glyphs (kerning across the caret).
- Against: we write and own every editing key.

### 2B. A native `NSTextView` laid over the canvas while a text is typed

On edit, `platform` places an `NSTextView` at the text's frame times the
zoom, font at the text's size times `FONT` times the zoom, transparent,
container as wide as the column; the scene hides the item, as `EDITING`
does today. Each edit goes to the app as the whole text (as the entry's
`Event::Typed`); escape and a press outside end it.

- For: everything a Mac text view does, free: caret, selection, input
  methods, dictation, spell check, services, word and line keys.
- For: the sling's entry already works this way (`platform/entry.rs`).
- Against: two renderers. `NSTextView` wraps its own way, not in the core's
  rows (docs/design/canvas-file.md, slinger-f66a97): a wrapped text jumps
  when it ends. Keeping them equal means a custom layout or giving up the
  rows rule.
- Against: the view must follow every pan, zoom, pane resize and page turn
  while typed, and lose and take back the keyboard as the entry does
  (`Keyboard`, `went_on`): more state in `platform`, the module that is
  hardest to test; none of the editing is testable.
- Against: return, escape and the window's `sendEvent:` key routing must
  be special-cased for a second kind of field; a pen tap inside it goes to
  the view, not the scene, so the scene's rules for a press near a typed
  text move into AppKit.

## Recommended (picked overnight, owner to confirm)

**Question 1: 1B, the mouse selects with the pen tool. Question 2: 2A, the
pure scene editor.** Both leave every tablet event as it is today; both are
undone by one function each (1B: `Tools::tool` ignoring the device; 2A is
additive over `Shapes::typed`).

### Pick 1: the pointer (slinger-9c9553)

Behaviour:

1. A press is a **tablet press** when its `NSEvent` subtype is
   `TabletPoint` or `TabletProximity`, else a **mouse press**. Touch is as
   now (never draws, never selects).
2. The tool a press uses: the eraser if the tablet's eraser end is in use
   (as now); else the select tool if the chosen tool is the pen and the
   press is a mouse press; else the chosen tool.
3. The footer and the canvas cursor show that tool for the device last
   seen: a tablet proximity event "pen near" sets tablet, "pen left" sets
   mouse, and each press sets it from its own subtype. Before any tablet
   event the device is the mouse. A pen user sees the pen (the first
   proximity event comes before the first press).
4. Space-drag and middle-button drag pan, as in "Both" above. While space
   is held and free, the cursor is the open hand; during the drag, the
   closed hand. A press with space held never reaches a tool.
5. Losing the key window, or a space key-up, ends the hold.

Who owns what:

| Module | Change |
|---|---|
| `scene/ink.rs` `Pen` | new field `tablet: bool` |
| `platform/input.rs` | `pen()` fills `tablet` (the subtype test it already makes). Proximity sends `Event::Proximity { near, eraser }` in place of `Event::EraserEnd(bool)`. Middle button (`OtherMouseDown/Dragged/Up`, `buttonNumber() == 2`) becomes `Event::Scroll` exactly as the right button does (share the `RIGHT` cell). Space with no modifiers: `KeyDown` (repeats too) sends `Event::Space(true)` before the usual key path, which goes on only if it is not used; `KeyUp` sends `Event::Space(false)`. No decisions. |
| `platform/mod.rs` `Event` | `Space(bool)`; `Proximity { near, eraser }` replaces `EraserEnd`. Window resigning key sends `Space(false)`. (mod.rs is near 500 lines: move a doc block or helper out if needed.) |
| `scene/tools.rs` | `Tools::set_device(tablet: bool)`, kept beside the eraser end; `Tools::tool()` applies rule 2; `Tools::chosen()` stays the chosen tool. |
| new `scene/hand.rs` | `Hand`: `space(down, free) -> bool` (took it), `press/drag/lift(at) -> Option<(Point, dx, dy)>` for a space-drag, `cursor() -> Option<Cursor>`. Pure, unit-tested. |
| `scene/cursor.rs` + `platform/canvas.rs` `ns_cursor` | `Cursor::OpenHand`, `Cursor::ClosedHand` (`NSCursor::openHandCursor`, `closedHandCursor`). |
| `main.rs` | route `Event::Space` to `Hand` with `free` from shared/canvases/tools; in `pen()`, a held hand takes `Press/Drag/Lift` before dividers and turns them into `canvases.pan`. main.rs is 499/500 lines: move a helper out first (for example `taken`/`picked` into `window.rs`). |

No new command: space and the middle button are held, not chords. The key
overlay lists them as fixed rows under "Pointer" (slinger-7ada2a,
`scene/overlay/fixed.rs`).

### Pick 2: the caret inside a canvas text (slinger-ea308d)

Behaviour (macOS text conventions; a "row" is one of the core's rows, so
a wrapped line's rows are separate rows):

| Keys | Does |
|---|---|
| left / right | one character (a selection collapses to its start / end) |
| alt+left / alt+right | to the previous word start / next word end |
| cmd+left / cmd+right | to the row's start / end |
| up / down | the row above / below, at the nearest x to where the caret was (kept across repeated up/down); from the first row up goes to the start, from the last row down goes to the end |
| cmd+up / cmd+down, home / end | the text's start / end |
| shift + any of the above | extends the selection from its anchor |
| delete | the selection, else the character before |
| alt+delete / cmd+delete | back to the word start / the row start |
| forward delete (fn+delete) / alt+forward delete | the selection, else the character after / to the word end |
| cmd+a | selects the whole text (not the canvas's items) |
| cmd+c / cmd+x | the selected characters as plain text; with none selected, as today |
| cmd+v, dictation (Flick pastes) | replaces the selection, else goes in at the caret |
| typing, return | replaces the selection, else goes in at the caret; return starts a line |
| escape, a press outside | ends the text, as today |
| double tap inside | selects the word there (or the run of spaces or punctuation); a drag after it keeps the word (slinger-ed74af) |
| triple tap inside | selects the row there |
| ctrl+B / F / P / N, ctrl+A / E, ctrl+H / D, ctrl+K / Y, ctrl+T, ctrl+O | a Mac's emacs keys: a character back / forward, a row up / down, the line's start / end (a wrapped line's rows are one line), delete back / forward, cut to the line's end / put it back (in this text, not the clipboard), swap the characters at the caret, start a line after the caret; shift extends the moves (slinger-ed3fbc, `scene/editor/emacs.rs`) |

- A **word** is a run of alphanumeric characters or `_`. The caret, delete
  and a press inside the text move by grapheme cluster (a ZWJ emoji, a
  letter with its combining marks, a flag), with `unicode-segmentation`
  (slinger-459ce9).
- A tap that **opens** an edit leaves the caret at the end, as today: a pen
  tap still appends. A **press inside the text being typed** (today it does
  nothing) puts the caret at the nearest character boundary; a drag from
  there selects; shift+press extends. Same for pen and mouse.
- The selection draws as filled rectangles, one per row, in the focus
  colour at low alpha, under the text; the caret is a bar `CARET` points
  wide. Both come from the scene.
- These keys are fixed text keys, not commands in the registry, as the
  native entry's are. A chord with cmd or ctrl that the editor does not use
  goes on to the keymap, as now. The key overlay lists the main ones as
  fixed rows under "Typing a text" (`scene/overlay/fixed.rs`; a test checks
  the editor takes each chord listed there).
- **Precedence**: while a canvas text is typed, every key in the table is
  the text's, before the keymap. So alt+arrows move the caret by words instead of panning,
  and cmd+a selects the text's characters. Lane work on the items
  (slinger-e96ea3: select all, nudge with the arrows) applies only when no
  text is typed.
- Undo inside a typed text stays as today (follow-up: per-edit undo while
  typing).

Who owns what:

| Module | Change |
|---|---|
| new `scene/editor.rs` | `Editor { text: String, caret: usize, anchor: Option<usize>, goal_x: Option<f64> }` (byte offsets on char boundaries). `insert(&str)`, `key(Chord, &Layout) -> bool` (the table), `selected() -> Option<&str>`, `cut() -> Option<String>`, `place(index, extend)`. Pure, with its own tests. Also takes `Editing` out of `shapes.rs` (452 lines). |
| `scene/glyphs.rs` | one row layout as byte ranges: `layout(text, size, column) -> Vec<Row { start, end }>`; `rows()` rebuilt on it so the rows drawn and the rows moved through never differ. `caret_at(at, text, size, column, index) -> Rect` (x is the measured width of the row's prefix at `FONT`), `index_at(at, text, size, column, point) -> usize`, `selection_rects(...) -> Vec<Rect>`. |
| `scene/shapes.rs` | `Editing` holds an `Editor`. `typed` and `pasted` go through it (typed control characters as now: delete, return, escape). New `key(chord) -> bool`, `copy(cut) -> Option<String>`; `down` inside the typed text places the caret, `drag` while pressed inside selects. `shown` draws the selection rects and the caret bar. |
| `scene/canvas.rs` `Shape::Caret` / `Mark::Caret` | a plain bar rect from the scene; `platform/canvas.rs` drops `end_of` and the layout-manager lookup for it. |
| `canvases/keys.rs` | `Canvases::edit_key(chord) -> bool` (hands to `Shapes::key`, sends on a change). |
| `main.rs` `taken` | `canvases.edit_key(chord)` after `sling_key`, before the pop-up and the keymap. |
| `canvases/clip.rs` | `copy(cut)` asks the typed text first: selected characters become a `Clip` with text only. `can_copy` is true then. `paste` already goes to `Shapes::pasted`. |

### Binary budget

The `slinger-app` ceiling is 1572864 bytes and no budget moves tonight.
2A is plain Rust over strings and rects; it removes the caret's layout
lookup in `platform/canvas.rs`. 1B adds a bool, a small module and two
cursors. If either cannot fit, land the pure modules and hold back the
platform part, per the brief.

### What is left to the owner

- Confirm 1B over 1A, on the real tablet: the pen draws exactly as before,
  and a trackpad press with the pen tool selects (slinger-7ada2a).
- Confirm 2A's missing input methods are acceptable for notes; a native
  view (2B) for CJK input stays possible later, for that text only.
