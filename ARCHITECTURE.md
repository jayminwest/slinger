# Architecture

Read this before you change a contract. Update it in the same commit.

## Layers

```
crates/core   slinger-core: domain logic, typed errors. No terminal, process or front-end deps.
crates/cli    slinger: argv -> control-socket requests -> output contract (docs/cli.md).
              Does not link the core.
crates/control  slinger-control: the control protocol (docs/service.md): requests, replies,
              the wire format and the socket client. Does not link the core.
crates/daemon slingerd: owns the IO the core leaves out (socket, signals, startup, the
              record's SQLite file) and serves the core. A thin main over its library.
              No domain logic.
crates/app    slinger-app: the macOS app. Control-socket requests -> a scene -> one window.
              Does not link the core.
xtask         quality gates. Not shipped.
```

One request travels this path, and no other:

```
argv ─ crates/cli ─┐
window ─ crates/app ─┤
                     ├─ control::Client ─ socket ─ crates/daemon ─ core
other programs ──────┘        (one JSON line each way)
```

Only the daemon links the core, so each capability has one implementation
and one path to it. No binary crate depends on another. The clients' tests
dev-depend on the daemon's library, so they run against a real socket.

### Inside the core

```
canvas/      pages of items, changed only by operations. Build (Canvas::new),
             apply (Canvas::apply(Op)), query (pages, items, drawn, item). Ops are data,
             so a log of them replays to the same canvas, ids included. The
             storage layout is private. Keeps the author rule: an agent changes
             only items it made; removing what is already gone succeeds.
             History: undo and redo per author; a step is one op or a group
             (lasso move, erase gesture). Undo applies inverse ops (Restore,
             PageRestore put things back under their old ids) and returns
             them, so the op log stays append-only. Each step has a mark (the
             log uses its first line's number); the file keeps each line's
             place in it (Step: `step`, `undoes`, `redoes`), and Rebuild makes
             the history again as the lines replay, so it outlives a restart.
             canvas/pdf.rs: the `pdf` kind's rules (docs/design/pdf.md):
             frame, blob id, page, page size, crop and name as numbers and
             text; Shape::shown, the crop or the whole page. The core never
             reads a PDF. `drawn` puts pdf items first: they are the paper
             under every other kind, for the render and every hit test.
canvas/delta.rs  what is new after the last sling (R1): an op log and a
             marker (ops applied before it) in; the items new, changed or
             removed against the canvas at the marker out. Compares the two
             canvases, so added-then-removed is nothing. No errors.
canvas/geometry.rs  pure queries on a page: item at a point, items in a
             lasso (nonzero winding), items an eraser touches (strokes, text,
             a box by its outline, arrows; never a live item, the
             button or a pdf item), items touching a
             box, bounds of a set, free space beside an item (free_beside: right,
             below, left, above, then further right). No errors: degenerate input
             finds nothing.
canvas/file/ the canvas file (docs/design/canvas-file.md): a versioned, append-only
             JSON Lines log of operations. Header and line encode, read (cuts a
             torn last line), replay into a canvas, snap to the stored tenths. No IO.
             Log holds a file open for the daemon: requests (ops as JSON, the
             author, the time) in, numbered lines to append out, with the
             History beside it. Canvas ids (check_id) never name a path.
             An arrow added in a request may name an item an `add` before
             it makes (`$n`, its place in `made`): ids are given in order.
             Log::import stores a whole canvas (an import) as a new file
             that replays to it, ids, makers and times included.
             Log keeps every op it applied with its line's number, so
             Log::packet builds a page's packet with the op range in the
             file's line numbers and takes item ids as the file or a packet
             writes them.
             Log::after finds where the lines after a number lie in the file,
             and `lines` reads them back whole, for the change feed.
             look.rs: what an agent asks beyond the packet (R5, R6): crop (an
             item, a cluster c<n> or a box, as render images, at most MAX_CROP
             units a side) and strokes (a stroke's or a cluster's points).
             put.rs: an agent's answer (SPEC section 7): text, a box, ink, an
             arrow or a live item (an agent UI's showing, kind `live`) beside
             an item, placed by free_beside or at the agent's `How::at`, in
             `How::colour`, text at `How::text_size` (tenths, checked against
             TEXT_SIZES by put::text_size), as one undo step; check_put says
             first whether a put would go.
             edit.rs: the agent changes (text, text size, size, place) or removes what it
             put; a move or resize (here or by canvas_apply, Log::followed)
             refits the bound ends of the arrows `by` may change, in the
             same step. The author rule stays in Canvas (may_change),
             checked first.
packet/      what an agent reads from a page (docs/design/packet.md). Index::new takes a
             canvas page, the clusters of the user's ink and what changed, and gives
             the `new`, `made` and `page` lines in reading order, plus the render list:
             the new clusters that need images. Agent-made items are lines, never in
             the render list (R2). Owns the line grammar, ids and reading order.
             A `pdf` item is a text line (id d<n>, never an image: R8); an ink
             line names the pdf items under it (`over`), and `page` holds a
             pdf line only when an ink line names it or it is selected.
             cluster(canvas, strokes) groups strokes into notes by distance, colour
             and time, sized by each pen's median stroke; a long stroke (frame, arrow,
             divider) joins only ink it touches, and an agent's stroke stays alone.
             Same strokes, same clusters.
             build(log, ask) is the one entry: an op log, a page, the last sling's
             marker and a selection in; a Packet (header, sections, images, and
             the selection as packet ids: a stroke as its cluster c<n>) out.
             It hides the delta, clustering (whole page), rendering and the index.
             Packet::text / Packet::read own the versioned format (VERSION, B5).
             backdrops(log, ask) names the areas of `pdf` items under the new
             clusters' images (Area), for the app to render; with the rendered
             Backdrops in Ask, those images show the page under the ink and an
             ink line ends with the PDF's words under it (words.rs, capped at
             MAX_WORDS_CHARS).
tldraw/      the tldraw importer (SPEC D7): import(bytes, time) -> canvas and a
             report of what it made and skipped. Reads the zip + SQLite file
             and the older JSON one itself (no IO, no new dependencies); every
             item goes through Canvas::apply. Import only, never export.
render/      images of items (docs/design/packet.md, Images): a set of items or a
             box of a page in, PNGs out, at a fixed scale, cut into overlapping
             tiles past the size limit (SCALE, MAX_EDGE, OVERLAP, MARGIN). Owns
             the rasterizer (raster.rs) and the PNG container (png.rs). Pure and
             deterministic: the same input gives the same bytes. Text draws
             at its item's size: a cell a character, half the size wide (render::CELL,
             about the app's font, so a full row fills its frame there too) and a size tall.
             text_size: the frame text needs as drawn; text_frame: the one rule for a text
             item's frame (Shape::fitted), which Canvas::apply and the file's
             snap both keep, so no text item covers less than it draws.
             backdrop.rs: a Backdrop (a `pdf` item's pixels over an area, raw
             RGB from the app, and its Words); an ink image draws the usable
             ones under its ink, cut to the item's frame, white elsewhere.
sling.rs     the sling message (SPEC section 6, D10): a packet, the prompt, the
             selected packet ids, first-or-later and the verbs the caller serves in;
             text and image blocks out, each image after the packet line naming it.
             Line 1 is `slinger-sling <VERSION>`; then the packet, a `# reply` note
             (whole on a session's first sling, one line after) and the prompt last.
             The note names only the verbs passed in. No errors.
             first: whether a sling is the first into a conversation (no
             `sling_sent` of the agent names that session id).
             landed: the `reply_landed` an agent's answer adds, for its latest
             sling on that canvas.
feedback.rs  notes about slinger itself (docs/service.md, Feedback): a note
             and its context, a resolution, both as JSON Lines; reading a
             file's text back (newest first, open or all) and resolving a
             note by its `ts`; the local time with its offset and the build
             string. No IO: the daemon hands in the text, the time and the
             build.
record/      the record's formats (SPEC section 9): events as versioned JSON
             (encode, decode, to_value), the kinds a writer may append
             (Body::new checks a payload against its kind) and blob ids
             (SHA-256, `sha256:<hex>`). No IO; the daemon stores what it encodes.
ui.rs        agent UIs (docs/design/live-items.md): the rules for a name and its
             code, the versions rebuilt from `ui_put` events (Catalog), what a
             put adds and what a reference (`name`, `name@3`, blob id) names.
             No IO; the daemon hands it the events.
ui/showing.rs  showings (`ui open`): a showing's id (`<name>#<n>`, n its
             `ui_opened` event), its hosts (a pane, a canvas), the limits on
             its data and replies, and the line its agent's session receives
             for each reply. No IO.
workspace.rs workspaces (SPEC section 10): the definition file's format
             (docs/workspace.md), the rules a set of them keeps (one primary
             root, name and canvas each), what a new one gets (slug of its
             name) and which workspace an event's canvas or agent tags. No IO;
             callers read and write the bytes. `peek` reads the name and
             canvas of a file that breaks a rule, for listing it only.
workspace/rules.rs  the rules: one workspace's (`Workspace::read` refuses
             with a `Rejected` that names the field) and a set's (`check`).
workspace/panes.rs  a workspace's panes (SPEC section 7): the tree of panes
             (a canvas or a UI each) and splits, numbered in order; open
             beside a pane or the window, close (the sibling takes the
             space; a pane already gone or the last one changes nothing),
             resize (the divider after a pane; none after the last), each
             with the `pane_moved` it records, and the `[layout]` text
             (version 1, `panes`). Workspace::panes reads it (one pane on the
             canvas when unset or unreadable), set_panes stores it.
workspace/profile.rs  an agent's profiles (`[[agent.profile]]`): a name,
             a working directory and permission rules, each rule checked
             against the tools it knows (an unknown one is refused, never
             passed on); find, set in place and remove on a Workspace.
session.rs   sessions (SPEC section 5, section 7 status set): a session per
             agent of the open workspaces (agent, home, delivery, status), the
             three statuses, and the record event each writes (blocked, done)
             or none (working), and back. on_canvas: the workspace a sling on a
             canvas goes to. No IO.
session/listen.rs  listen delivery: the slings that wait for one agent's
             listener, oldest first, and whether one is there (it asks, asked
             or said done within GRACE, or works on a sling it took, and the
             process it named for that work, `worker`, runs). No clock, no
             waiting, no process IO.
session/stream.rs  Claude Code's stream format (D10): the arguments that start
             or resume a session's process, with its limits (what it reads,
             runs and loads, from its workspace's agent, or under one of its
             profiles: `Limits::under`), a message as one
             input line (text, or a sling's text and base64 PNG blocks), and
             what an output line says (its session id, the end of a turn).
session/hooks.rs  Claude Code's hooks (SPEC section 5): the `--settings` JSON
             whose hooks run the CLI (turn start working, stop done, a wait
             for the user blocked, a file write `file touched --hook`), and
             the file a `PostToolUse` payload names.
error.rs     the typed errors and their stable codes.
```

**The record's formats are API (B5).** A reader keeps an event of a kind
it does not know (`Body::Other`) and every field it does not know, at any
depth, and `encode` writes them back unchanged. A payload changes only by
adding fields, and the change bumps that kind's `version`.

### Inside the daemon

```
lib.rs       argv, startup and signals: `run`.
server.rs    the control socket and `answer`, one arm per verb. `serve` takes the
             store, so a verb that reads or writes the record reaches it here.
             Blob and UI verbs read and write the file the client names: a
             blob's bytes travel as a file, never in the line (docs/service.md).
             `ui_put` decides its version inside the store's write
             (`Store::append_if`), so two puts cannot take one number.
store.rs     the record on disk: one SQLite file (rusqlite, bundled) with
             the event log and the blobs. The only module that knows its
             layout. Stores each event as the line `record::encode` wrote.
             Locks the data directory, so one store owns the record.
             Hands out the canvases, the workspaces and the sessions, so
             `answer` reaches all data one way.
store/schema.rs  the file's layout, its version and the steps that bring
             an older file up to date. The file keeps the machine name it
             was created under, so a host rename keeps the tag.
store/waiting.rs  the slings that wait for a listener, kept in the record
             (a table of their own, layout 3) from `sling_sent` until the
             listener deals with them, on the record's connection.
canvases.rs  canvas files on disk, the only writer (K5): one lock per canvas,
             so changes are numbered in the order they apply; append, sync,
             then reply. A failed write closes the canvas; the next request
             reads the file again. Cuts a torn last line. A new canvas's
             file (empty or imported) is written aside and renamed in whole.
             Crops render under the canvas's lock; `answer` writes their
             images to the file the client names, tiles beside it.
canvases/feed.rs  the change feed: the stored lines after a number, read back
             from the file, and a wait on the canvas's condition variable that
             a change or `Canvases::stop` (called when `serve` stops) ends.
lock.rs      the lock on a file that says who owns a place: the socket's
             `.lock` (which daemon serves the path) and the data directory's
             `record.lock` (which store owns the record). Dropping it unlocks
             it, so a spawned process's copy of the file, open from its fork
             to its exec, cannot hold the place a moment past its owner's stop.
server/import.rs  `canvas_import`: reads the client's file, calls the tldraw
             importer, stores the canvas, replies with the counts.
workspaces.rs the open workspaces: the list of directories that hold their
             files (`workspaces/open.json`), written whole; the files are read
             on every request and checked by the core's rules, each on its
             own: a file that breaks one is listed `Broken` with its error
             (`slingerd` names it on stderr at startup) and every verb on
             that workspace fails with it; the others serve, and a new one
             may not take the name or canvas a broken file states
             (`workspace::check_reserved`). The store asks
             it for the workspace tag of each event it appends. `panes`
             changes a workspace's layout under the list's lock and writes
             the file whole when it changed; `edit` does the same for any
             change the core makes (the profiles).
server/profile.rs  `profile_list`, `profile_set`, `profile_remove`: the core
             checks the profile, a `dir` must be there (kept with its links
             resolved), and `Workspaces::edit` writes the file.
server/pane.rs  `pane_list`, `pane_open`, `pane_close`, `pane_resize`: the canvas or UI a
             pane shows must exist; the core changes the layout; each change
             appends its `pane_moved`, tagged with the workspace. `open` is
             shared with `ui_open`.
server/showing.rs  `ui_open`, `ui_shown`, `ui_reply`: an open checks all
             before it appends `ui_opened` and opens the pane, or, on a
             canvas, appends it under the canvas's lock once the put will go
             (Canvases::put_when) and puts the live item; a reply counts
             the showing's replies inside the store's write, appends
             `ui_reply`, then hands the core's line to the agent's process.
server/read.rs  `canvas_read`: the marker is the `to` of the packet the
             canvas's last `sling_sent` names (0 before one), so it lives in
             the record and survives a restart. Stores the text (no `at`)
             and each image as blobs, then writes them into `--out` or the
             store's `packets/<hex>/`. `restore` writes a listener's sling's
             packet there again from its `sling_sent` blobs when a file is
             missing; `session_listen` calls it on each sling it hands out.
sessions.rs  the statuses agents set since the daemon started (`working`
             lives only here); else each agent's last status event in the
             record. `status_set` appends its event inside the same lock.
             Start, stop and send find the agent by its workspace.
             `touched` reads and hashes a file an agent wrote and appends
             `file_touched` (path and hash, never the bytes).
             `listen` takes the agent's next sling for a listener and sets
             `working`; start and stop release a listener's claim. A stop
             drops the agent's `working`: nothing it stopped says `done`.
listeners.rs the listeners, one per agent (`session::listen`), behind one
             lock and a condvar: `listen` holds an ask open until a sling
             comes, the wait passes or the daemon stops. Each lock finds a
             listener whose worker process ended `gone` (`alive`: kill 0);
             `freed` names those agents, and `sessions.rs` drops the
             `working` their sling set. Writes each change through to
             `store/waiting.rs` under the same lock: a sling is kept when it
             waits and forgotten when the listener deals with it (asks again,
             says done or blocked, or is released); a restart starts with
             every kept sling waiting and nobody there.
processes.rs the agents' processes, one per agent: spawned in the agent's
             home (or its profile's dir) within its limits, with `SLINGER_AGENT` and `SLINGER_SOCKET`,
             written to under one lock, read by a thread each so a pipe never
             fills. Keeps each session id in `sessions.json` once the output
             names it, under `<agent>#<profile>` for a profile, so each
             profile resumes its own conversation; drops it when a resume fails (an error exit before the
             output names it) or on a `fresh` start. A start under another
             profile replaces the running process; a send for one profile
             reaches only a process under it (`conflict` otherwise). Dropping one closes its input, waits a grace period,
             then kills it; `serve` ends them all when it stops. No restart on death (D20).
             Each starts with the core's hooks in `--settings`, running the
             CLI at `--slinger` (default: beside slingerd).
             `processes/turns.rs` counts `pending`: a result answers the
             messages its turn took in, so one still unanswered 2 s after a
             result with no output since was taken into that turn. The
             reader keeps the context of the last model reply (`context`).
feedback.rs  the feedback file (`--feedback`, default `feedback.jsonl` in
             the checkout it was built from, else the data directory): one
             lock, each note and resolution appended as one line, the
             file read back for a list or a resolve; the time from `date
             +%z` and the build from `SLINGER_BUILD_SHA` at build time.
build.rs     stamps that build: `SLINGER_BUILD_SHA` and `SLINGER_BUILD_DIRTY`
             from git (`HEAD`, tracked changes), unless the environment
             sets them; neither without git, so notes say `dev`.
server/feedback.rs  the four feedback verbs on the wire.
server/session.rs  the session verbs on the wire, each session with its process
             and its listener. `session_listen` is a long poll; a reply that
             cannot be written puts its sling back (`unheard`).
server/sling.rs  `sling_send`: the workspace of the canvas, the marker, the
             packet, `sling::message` with the verbs this daemon serves
             (VERBS), delivery to the agent's process, then the blobs and
             `sling_sent`. With no profile named, a listener that is there
             takes it in place of the process, and with neither it waits for
             one: its images go to the store's `packets/<hex>/` and its text
             to the agent's listener. The reply carries the message's token estimate. Nothing is stored when delivery fails. One sling at a
             time (`Sessions::slinging`), so packets never overlap.
             `sling_backdrops` names the PDF areas a sling of a page would
             draw over; `sling_send`'s `backdrops` (files the app wrote)
             are read here and handed to the packet.
server/agent.rs  `canvas_put` on the wire: the put, then `reply_landed` for the
             agent's latest sling on the canvas (`sling::landed`), if any,
             looked up under the sling lock so a fast reply finds its sling;
             `canvas_edit` and `canvas_remove`, which pass the agent's name
             to the core's `Log::edit` and `Log::remove`.
```

### Inside the app

```
main.rs      wires the modules below. The only caller of platform. With
             `--preview UI [--data JSON]` it shows one agent UI alone and
             writes each reply's `ui_reply` line to stdout instead; with
             `--background` last, that window is never shown. It routes
             each event (route.rs, which the window's answer and the tests
             both call; the answer adds a tick's news): keys, the pen, the
             mouse, the trackpad. A pen or
             mouse press first focuses the pane under it (its content or its
             footer; not a touch, a divider, or under the key overlay or the
             palette), then does its work there; a press an agent UI's page
             takes still draws the new focus (Step::Through). A press on
             a footer's or the sidebar's button goes no further; the lift
             on it runs it.
window.rs    runs a command on the focused pane, does a tap on a button that
             does more than run one (`tap`: the page name opens the page
             menu, a page menu's page turns the pane, the footer's agent
             opens its profile menu and a profile slings under it, a
             workspace shows its panes), opens the folder `open-workspace`
             chose as a workspace (workspace_open: backend/agents.rs) and
             shows it, or flashes why not, closes the shown one for
             `close-workspace` (workspace_close by its folder) and shows the
             first, or flashes why not, starts, stops or restarts the
             shown one's agent for `start-agent`, `stop-agent` and
             `restart-agent` (`agent`: session_start, session_stop or
             session_start `fresh` by its folder, backend/agents.rs) and
             flashes its name or why not, finds what `show-canvas-file` and
             `show-workspace-folder` show in the Finder (`finder`: the
             canvas file, or the shown workspace's folder from the
             sidebar; main reveals it), saves the focused pane's agent UI
             in a folder for `export-ui` (`export_ui`: the bytes the pane
             has, named as ui_export names them; main picks the folder and
             reveals the file), builds the window's scene (`view`: the footers,
             the sidebar's foot, the lit button, the tap targets), and
             finds the canvas pane under a point and the focused pane's
             frame. A cut, copy or paste no text field took (`edit`), and
             whether one would do anything (`can_edit`, which greys its
             Edit menu item): under the key overlay or the page menu
             nothing; the open palette's search takes pasted text; else the
             canvases (canvases/clip.rs).
shared.rs    the window's panes as the server keeps them for its workspace,
             the window's size, the agent UIs the panes show and their notes,
             the footers' and the sidebar's tap targets (scene/taps.rs), and
             the key overlay, the command palette and the page menu over
             them: it routes a press and its lift to the first of those
             that takes it, follows the server's layout and sends the pane
             commands.
backend.rs   the app's IO: the one module that talks to the daemon, and the
             one that reads and watches the config files. Its Watch asks the
             daemon for the workspaces and their agents' statuses, the
             profile each runs under and the names of its profiles
             (workspace_list, session_list: backend/agents.rs) and for the window's workspace's
             panes (pane_list: the config's, else the first open one) every
             second on a thread of its own, and the window's timer takes the
             news without blocking. When the layout changes it fetches the
             code of each agent UI the panes show first.
             Its Follower keeps one canvas page coming: `canvas_show`,
             then the change feed. Its Outbox sends the pen's changes
             (`canvas_apply`, one each: a stroke's, text's, box's or
             arrow's `add`, an erase's or a delete's `remove`, a drag's
             `move` with a `set` of each arrow that follows, a text edit's
             `set`, a page's `page.add`, `page.rename` or `page.remove`;
             an undo or a redo as `canvas_undo` or `canvas_redo`) on a
             thread of its own, in the order handed in, and hands back each
             answer (a new page's id with it).
backend/sling.rs  the Slinger: each sling as one `sling_send` (canvas, page,
             the selected ids, the prompt) on a thread of its own, and the
             daemon's answer back (`Heard`). The daemon picks the agent.
             A sling under a picked profile first starts the agent under
             it (`session_start` with `profile`: the daemon leaves one
             running under it as it is and restarts one under another),
             then sends with `profile`.
             Before it sends, with a Render given, it asks `sling_backdrops`,
             renders each area of the fetched PDF into a per-sling temp dir
             and sends the files as `backdrops`; the dir goes after the reply.
             After a sling goes it follows the agent's session
             (`session_show` at once, then every 0.5 s: process, pending,
             answer, context for the pill's count) until nothing is pending or the process stops, or the
             next sling. A sling that waits for a listener is out until one
             takes and answers it; while nobody listens the pill says so.
backend/feedback.rs  the Noter: each feedback note as one `feedback_add`
             (the text and what the window showed) on a thread of its own,
             and the daemon's answer back.
backend/flick.rs  Flick's dictation (SPEC D2, K6): `dictation start`,
             `stop` and `cancel` as Flick's own control requests (one JSON
             array line to `$FLICK_SOCKET`, else Flick's default socket; one
             `{"ok"}` or `{"error"}` line back) on a thread of its own. No
             speech code: Flick records, transcribes and types the text.
backend/ui.rs  what a UI pane shows (`ui_shown`: the version, and a showing's
             id and data), its code by ui_export into a directory of the
             app's own, read, removed; and a showing's reply (`ui_reply`),
             with a note when its agent did not take it.
backend/panes.rs  the server's panes: `layout` for the Watch, and `open`,
             `close` and `resize`, which send the window's own splits,
             closes and moved dividers to the server as `pane_open`,
             `pane_close` and `pane_resize`.
backend/pdf.rs  PDFs (docs/design/pdf.md): `fetch` gets a pdf item's blob
             once into the app's cache (`$TMPDIR/slinger-app-pdf/<hex>.pdf`,
             written to a part file and renamed); `open` makes a PDF a
             canvas: `blob_put`, then `canvas_create` (the file's stem, `-2`...
             on a taken id) with a file of operations: one whole-page item a
             page stacked down its first page (scene/pdf.rs), so the canvas
             is made whole or not at all; then `pane_open` beside the
             focused pane. A file that cannot be read makes nothing. A
             tldraw file (`is_tldraw`, by its extension) is made a canvas
             by `canvas_import_id` instead, under the same id without
             `.pdf` (`-2`... on a taken id), then the same `pane_open`.
mirror.rs    one canvas page as the server holds it: `canvas_show` in, each
             feed reply applied in order; reads the canvas file's items
             (tenths, delta points, `#rrggbb`, an arrow's bound ends) into
             scene items, and writes the pen's changes as `canvas_apply`
             operations, and the frame a typed text is stored with (the
             core's render cells at the text's size (half a size wide, glyphs::CELL), `size` left out at the
             usual 24 and `null` in a `set`). A text item reads in with its
             stored column and the frame its glyphs fill in the core's rows
             (scene/glyphs.rs), never the stored width, so every
             hit test, the lasso, the selection and the caret follow the
             glyphs, texts saved earlier included. Keeps the canvas's pages,
             with their names. Items carried to another page are `add`s
             there, an arrow carried with its ends still bound to them. Pure.
             A `pdf` item reads in only with its hash, page and page size
             (tenths of a point) and a crop, when it has one, inside them;
             it is written with no colour, and a `set` clears a crop or
             name it has not. `add_pdf`: an opened page's `add`.
mirror/clip.rs  the selection's items on the clipboard: their file objects
             as a JSON array, as a carry writes them (no id, maker, time
             or page; an end bound to an item copied with it as `$n`), and
             its texts' plain text, one a line; a paste reads the objects
             back as a carry's items for a page, centred on a point, an
             arrow still bound to the items pasted before it; plain text
             alone reads as one text item (`text_items`). Pure.
canvases.rs  the canvas panes: one follower and mirror per page shown, each
             pane's page, viewport and last scene (kept on screen until a
             page turned to arrives); the pen's ink, the eraser's and
             lasso's work and the text, box and arrow tools' over each page,
             and the outbox they share. A selection no pane shows clears.
             A press on a canvas closes the open entry (the sling's, the
             feedback's, a page's new name's), but one on a button sends
             the sling's.
canvases/live.rs  the live items' pages: each showing a pane draws fetched
             once on a thread of its own, put over its item; a press on an
             item whose page shows goes to the page unless it grabs the
             selection, or the item is under 48 points wide on the screen.
             A view kept off the screen (scene/live/away.rs) due to close
             draws the window again.
canvases/pdf.rs  the `open-pdf` command (cmd+o: the open panel, a PDF or
             a tldraw file, then
             backend/pdf.rs's `open` on a thread, one at a time; a failure
             is a problem until the next open) and each pdf item's file:
             asked for once per hash on a thread as a pane first shows it,
             grey until it comes, then drawn (a file's coming repaints).
canvases/reader.rs  reader mode on a pane showing a PDF's canvas (its id
             ends in `.pdf`, scene/pdf.rs's `reader`): each draw fits the
             page's column of pdf items to the pane's width (layout.rs's
             `Viewport::reading`, so a resize refits); a scroll moves up and
             down only, stopping at the column's ends; zoom does nothing.
canvases/button.rs  the "send to agent" button (SPEC section 8): a tap slings
             what is new on its page (`sling_now`, nothing selected, the
             config's prompt), or sends the open entry; the eraser never
             taps one. A failure shows in the sling's pill, never the
             keyboard entry.
canvases/slung.rs  the one-tap sling and what follows it: `sling-now` (the
             selection's ➤ Sling, the footer's ➤ Sling) sends the open
             entry, else the selection, else what is new on the page, with
             `[sling] prompt` when nothing is typed; no agent in the
             workspace flashes "No agent in this workspace" and sends
             nothing. Each workspace's agent (`agents`, from the sidebar).
             canvases/profile.rs: the profile menu's rows (`no profile`,
             then the agent's profiles, the running one marked) and a
             sling under the row tapped, as `sling-now` sends.
             The pill's progress from the Slinger, a tap on the pill, and
             the sling's marks: the buttons beside the selection, the pill
             and the answer's panel.
canvases/clip.rs  cut, copy and paste on the canvas panes: copy takes the
             selection the focused pane shows (cut then removes it, one
             step); paste puts text into the text being typed or a page
             command's question, else items (or plain text alone, as a new
             text of the pen's colour) onto the focused pane's page,
             centred in it, one step, selected once there. A text being
             typed has no selection inside it (its caret is at its end), so
             copy and cut grey out while one is.
canvases/keys.rs  what the keys do on the canvas panes: typed and pasted
             text answer a page command's question or go into the text being
             typed; delete and escape act on the selection from any pane
             that shows its page.
canvases/find.rs  which page, canvas and items each pane shows; a window
             point in canvas points.
canvases/drag.rs  the pen's drag and lift on the canvas panes: a drag of the
             selection read through the pane under the pen; whether a press
             grabs the selection (it wins over a divider within reach); the
             pane whose selection handle the pointer is on, for its resize
             cursor. The selection is of a page, so every pane on its page
             shows it.
canvases/paging.rs  the canvas commands on a pane: pan, zoom (in, out, to
             100%; a pinch and a cmd- or ctrl-scroll come here too, and each
             zoom flashes its level, "Zoom 150%"), `toggle-grid` (the
             dotted grid on every canvas pane, flashing "Dotted grid
             on/off"; the config's `[canvas]` is its start), the page
             commands (each turns only that pane) and undo and redo, each
             through the ink's queue; each pane's page line; `save`, which
             flashes where the pane's canvas is saved (canvases/status.rs
             knows each pane's canvas file, from `canvas_show`'s `path`).
canvases/sling.rs  the sling from the window: opens the entry at the
             focused pane's selection, sends it once the ink of its canvas
             is written (`Ink::settled`), takes the answer. `sling_now` is
             the one entry point for a sling with no typing (a tap on the
             button). With nothing selected the entry opens beside the
             focused page's first button. A typed sling's failure opens the
             entry again; one with nothing typed shows in the pill.
canvases/feedback.rs  the `leave-feedback` command: opens the feedback
             entry in the focused pane with its workspace, canvas, page
             name, tool and selection count, closing an open sling's entry;
             saves it through the Noter, and asks it for the recent open
             notes (`feedback_list`) to list under the entry. It takes the
             keys and the sling's place in the scene while open.
canvases/dictation.rs  the `dictate` command: opens the sling's entry when
             no entry is open, then starts or stops Flick's dictation into
             the open one (sling, feedback, rename); the entry closing
             cancels one going. Flick types the text into the focused
             entry, so it reaches it as typing does. The pen's buttons
             beside a note's or a new name's entry (scene/sling/pen.rs),
             with the selection's in sling_marks.
config.rs    the config file's format: text in, a typed config or an error
             with its line out; merges a machine's overlay over it.
             `config/widths.rs` parses `[pen] widths` by hand (tenths of a
             point: no float parsing, for the size budget), and
             `config/canvas.rs` the `[canvas]` paper (plain or dots, the
             grid's spacing in hundredths of a point), `config/sling.rs`
             `[sling] prompt`, the words a sling with nothing typed sends.
             Pure.
commands.rs  key commands: the closed list of commands, the chord parser and
             the keymap from a chord to a command (and a command to its
             live chord). `registry` gives each command a title and a
             group and prints a chord in macOS glyphs: the key overlay,
             the palette and the menu bar read it (SPEC D21). `menu` lays
             the menu bar out: each menu's items, the key each shows and
             the check mark on the pen's tool. Pure.
panes.rs     the window's panes: what each shows (a canvas or an agent UI, by
             name), how they tile (a layout::Tree), the one focused pane and
             the pane commands. `follow` builds them from the server's pane
             tree, focus kept by number; the pane commands' one dispatch
             (`command`); `focus_at` focuses the pane under a press. Always one pane at least. Pure.
scene.rs     decides what the window shows: each pane's placeholder, an
             agent UI pane's page (a Web) or its name and why it cannot
             show, a note over a UI pane whose agent did not take its reply,
             one 1 pt focus border (only with two panes or more), the
             sidebar's fill and the hairlines (scene/style.rs), problems in the
             focused pane, the sidebar's rows (each with its font) and
             notice, the sling's labelled entry (scene/sling.rs), each
             pane's footer (scene/status.rs) and the key overlay (an
             Overlay: a dim over the window and its lines, each with a font
             and an alignment); the pills over the labels (Lines with
             round corners: the sling's); the flash and the cursor over the
             canvas panes. A canvas pane's chrome is
             decided with its gestures: the selection's box, handles and
             count (`select`), a text being typed's caret, frame and hint
             (`shapes`). Every state shows, nothing else (SPEC P6). Pure.
scene/style.rs  the window's look in one place: corner radii, the 28 pt
             least target for the pen, the hairline, and the surface colours
             (sidebar, footer, panel, hover, chip, edge, selected), each
             mixed from the Theme so a theme change follows. `rules`: the
             sidebar's fill and edge, the line under the title bar and a
             divider on each edge two panes share. Pure.
scene/status.rs  each pane's footer (SPEC P6), 28 pt along its bottom edge,
             under which nothing of the pane shows (`content`: hit-testing,
             paint and the pan and zoom centre use it): a row of
             pen-sized buttons (each at least 28 pt wide), the sidebar's
             (while it is collapsed, in the bottom-left pane), the pen's
             swatch, width bars and tool, `· typing` or `· N selected`,
             undo and redo; ‹ the page name and `i/n` › (the name opens
             the page menu); a canvas pane's agent with its status (with
             profiles, it opens the profile menu) and ➤ Sling; − the
             zoom +; split right, split down and
             close; the `show-keys` and `show-commands` keys, each only
             when bound. Each runs its command on its pane (the press
             focuses it first). A UI pane's has its name and the pane
             buttons. A narrow pane drops whole groups, the lowest rank
             first. `strip`: the open sidebar's foot, its hide button and "+"
             (`open-workspace`).
             `light`: the button under the pen. `tip`: a lit button that
             is only a glyph or a mark names its command and key just
             above the footer, on a rounded panel with a hairline edge
             that the footer carries (`Footer::tip`). A hairline tops
             each. Pure.
scene/status/cells.rs  how a footer's cells are laid: groups that stay or
             go together on the left, in the middle or on the right; the
             middle centred on the footer when there is room. Pure.
scene/taps.rs  the footers' and the sidebar's buttons as last drawn: the
             one under the pen lights, a press arms it and goes no further
             (no ink, no gesture), and the lift on it taps it (`Tap`: run
             a command, open the page menu, turn to a page, open the
             profile menu, sling under a profile, show a workspace); a
             lift off it does nothing. Pure.
scene/pages/menu.rs  the page menu: a tap on a footer's page name opens it
             above the name, one 28 pt row per page (the shown one ✓ and
             bold), then New page, Rename page… and Remove page. It takes
             every press while open; the lift on the row pressed picks it,
             a press outside or escape closes it. Pages that do not fit
             scroll a row at a time with a scroll or a drag on the panel
             (a drag taps nothing); a ▲ or ▼ at a page row's end says
             more pages are above or below. No dim. The same panel is the
             profile menu (`List::Profiles`): a tap on a footer's agent
             opens it over the agent with a row per profile (the running
             one ✓) and no commands. Pure.
scene/flash.rs  the flash: a new tool (from its command or a pop-up) shows
             `<Tool> <key>` (a new width: `Width <n> <pick-width key>`),
             reload-config its title and key, a saved note "Feedback saved",
             a page command what it did (scene/pages.rs's words) and a
             zoom its level,
             in a 14 pt pill, a step lighter than the pane, at
             the top centre of the focused pane for 0.8 s. The pill says when
             it goes; platform sets one timer for then (no repeating
             redraw). Pure.
scene/glyphs.rs  the box a text's glyphs fill, as a canvas view draws it:
             measured by a function platform hands in on the main thread
             (kept per text and size), else an estimate of the system font
             (tests). A canvas text is drawn in the core's rows for its
             stored column, a size apart (the box `canvas put` reports and
             a crop draws), in the system font at 0.83 of its size. Pure.
scene/paste.rs  the clipboard as the app sees it (`Clip`: plain text, a
             canvas's items) and the edits no text field takes (`Edit`:
             cut, copy, paste); pasted text made fit to type: line ends as
             `\n` (or a space, on one line), tabs as spaces, no other
             control or function-key character, so a paste is text, never
             keys. Pure.
scene/cursor.rs  the pointer over a canvas pane, per tool: the pen a ring in
             its colour, the eraser a ring its width, the lasso and the
             shapes cross hairs, text an I-beam, the select tool and an open
             pop-up the arrow. The arrow elsewhere (sidebar, agent UI, chrome).
             On a divider between panes, the resize cursor across it; on a
             selection handle, the diagonal resize cursor through it. Pure.
ui.rs        agent UIs (docs/design/live-items.md): a UI's page (served from
             slinger-ui://<hex>/ with the policy), the sandbox's constants
             (the policy and the shim in ui/policy.txt and ui/shim.js, the
             rule list) and the Bridge, which decides every
             load, navigation and bridge call (context, reply, their limits
             and codes) for one showing. Pure.
scene/sidebar.rs  the agents' statuses (SPEC D17): the open workspaces and
             their agents in; the sidebar's rows (a `WORKSPACES` heading; each
             workspace, the shown one bold on a lighter row; its agent's mark,
             round working, round green done, square red blocked, with
             `<agent> · <status>` (`<agent> (<profile>) · <status>` while it
             runs under a profile) or `no agent`; a blocked agent's note),
             each workspace's rows a target a tap shows (`targets`,
             `pick`), the foot kept for the hide button (`foot`),
             where the panes go beside it, and
             the one notice when an agent becomes blocked or done out. The
             first list is the baseline; a status that moves on clears its
             notice. An agent whose process exited or was stopped is `not
             running` (hollow, muted), one slingerd never started (no
             session id) shows what its hooks said, and one with a message
             pending is working though no hook said so (`Status::of`). `agents`: each
             workspace's agent. `close`: takes the shown workspace out of
             the list for `close-workspace`; `shown_dir`: its folder, which
             the agent commands and `show-workspace-folder` go to. Under the
             shown workspace's agent that does not run, "Start agent" (a tap
             runs `start-agent`). Pure.
scene/sidebar/agent.rs  an agent as the sidebar shows it: its `Status`,
             read against its `Process` (`Status::of`), and its profiles.
             Pure.
scene/sidebar/broken.rs  a workspace whose file the daemon cannot use:
             under its name a red ✗ and the error's message (less the file's
             path: backend/agents.rs), the field that broke a rule, then,
             while it shows, "Close workspace" (a tap runs
             `close-workspace`), then the hint; each wrapped to the
             sidebar's width (`slung::wrap`). No agent line. Pure.
scene/sidebar/notice.rs  the notice: a 28 pt bar above the footers (it
             hides none), "<agent> is blocked: <note>" or "is done", with a
             ✕ (`dismiss-notice`); a tap on the rest shows the agent's
             workspace. Done leaves on its own 5 s after it shows or at the
             pen's next press; blocked stays until ✕, `dismiss-notice` or
             its status moves on. Pure.
scene/slung.rs  the sling's pill (SPEC P6): at the top right of the pane
             that shows the sling's page, however it was made, what
             the agent is doing: "● scout working · 4s · ~12k" ("scout
             (dev)" under a profile, as its session says), "■ scout
             blocked: <note> · ~12k" (the sling's tokens, when the daemon's
             Slung reply counts them), "✓ scout answered · N new items" (a tap brings them
             into view, the next clears it), "✓ scout answered in text · tap
             to read" (a tap opens the answer's panel), "✓ scout is done,
             with nothing on the canvas", a failure's "✗ <message>" and hint,
             "✗ scout stopped before it answered". An answer with no items
             yet waits 1 s for them to land (the pill says "✓ scout
             answered") before it says "in text" (`SETTLE`). An answer
             with its items on the canvas, or done with none, goes 5 s
             after it is told (`DONE`); one in text and a failure 30 s
             after (`CLEAR`); any of them at the pen's next press on the
             canvas. The pill is a Line with a field's round
             corners, as wide as its words measure (scene/glyphs.rs). `out`:
             a sling is on its way or worked on. `slung/words.rs`: the
             elapsed time, the tokens, the arrow to the items, the answer's
             lines (wrapped as scene/glyphs.rs measures them). Pure.
scene/button.rs  the "send to agent" button: an item of kind `button`, drawn
             as an outline with the agent's status mark (the sidebar's) and
             its label; which button is under a point; where a new one goes.
             Pure.
scene/canvas.rs  decides what a canvas pane shows: a page's items and a viewport in,
             the visible marks in screen points and the box to repaint out;
             paint is cut off at the pane's edges (`clip`). Chrome over a page
             is two more shapes no server item has: a panel (a box with a
             fill, dashed or not) and a text's caret. Pure.
scene/pdf.rs  an opened PDF's canvas: its id (the stem kept to a-z 0-9 - _,
             56 bytes, `-n` from the second try, `.pdf`) and its pages
             stacked on one column 1600 units wide by one scale, 40 apart;
             a pdf item's part of its page shown (its crop or the whole
             page). Pdf items are the paper: drawn first (scene/ink.rs's
             `shown`), under every other item. Reader mode: whether a
             canvas is a PDF's (`reader`, by its id alone) and the column
             a pane fits (`column`). On a PDF's canvas the select tool and
             the lasso never take a pdf item (scene/select.rs). Pure.
scene/crop.rs  the `crop-pdf` tool (cmd+8): a dragged box crops the topmost
             pdf item under the press (else the topmost the box meets) to a
             new pdf item, `crop` the box mapped from the item's shown region
             to page points (`cropped`, on whole tenths, inside the page),
             its frame the box's size. The lift puts it on the clipboard
             (mirror/clip.rs's `snippet`, route.rs answers `Step::Copy`);
             the canvas does not change. Paste is the usual paste. Pure.
scene/title.rs  the title bar: the focused pane's canvas, its file under
             it (`~` for home) and that file as the proxy icon; what `save`
             flashes. Nothing to save by hand: the daemon writes each change
             as it is made (SPEC K5). Pure.
scene/grid.rs  the dotted grid: display only, never an item (nothing
             selects, erases, slings or stores it; the core's images never
             show it). Dots fixed to the canvas, a quiet mix of the
             background and muted, 1 screen point in radius at any zoom;
             every 2nd, 5th, 10th... dot (from the canvas origin) when they
             would come closer than 8 screen points. `Grid::dots`: the dots
             whose ink touches a screen box, so platform draws only those in
             the box it repaints. `Paper`: shown or not (the config at start
             and on an edit of `background`; `toggle-grid` between, not
             saved). A new paper repaints the whole pane (`Scene::on`). Pure.
scene/live.rs  where each live item's page shows: over its item's visible
             mark in each canvas pane, cut off at the pane, at its zoom; and
             how (Shows): its page, still (under 48 points wide on the
             screen: no presses) or away; and the note of a reply its agent
             did not take, along the item's top. Pure.
scene/live/away.rs  the live items gone off the screen (or their pane turned
             to another page): each view kept, hidden, for 30 s after it
             went, then closed; the clock is the caller's. Pure.
scene/ink.rs  the pen's ink: the stroke under the pen and the finished changes
             (strokes, removals, moves) the server's page does not hold yet,
             laid over it; which change goes next (one at a time, oldest
             first, a failed one again first), page changes, undos and
             redos in the same queue; the pen-to-paint delay log (O2). Pure.
scene/pages.rs  pages (SPEC D6): the next and previous page, a new page's
             name, where a pane goes when its page is removed, the page
             line, and the two questions a page command asks (a rename's
             typed or pasted name, in an entry like the sling's centred in
             its pane with the pen's buttons beside it; a removal of a page with items, asked once in the
             footer); what each page command did, said once for the flash
             (`Page 2 · 2/3`, `New page: Page 3 · 3/3`, `Renamed: plan`,
             `Removed: ideas · Undo brings it back`, `The last page
             stays`). Pure.
scene/select.rs  the eraser, the lasso and the selection (SPEC D6): an erase
             hides the strokes, text, boxes and arrows it touches at once and
             removes them on the lift (one change, one undo step); a loop
             selects the items it holds whole, drawn as a tinted box with four
             corner handles and its item count above
             it, all in the focus colour, at screen size; a drag inside
             the box moves them, and a drag of a handle resizes them
             (with the lasso or the select tool); delete
             removes them. Each is one change, so
             one undo step. The selection (canvas, page, server ids) is app
             state the sling and moves between panes read. Pure.
scene/shapes.rs  the text, box and arrow tools (SPEC D6): a tap places a
             text and typed keys (and a paste, at its end) go into it until
             escape or a press outside
             (while typed it shows a focus-colour caret, a dashed muted frame
             and a hint under it: `label`, chrome's 11 pt line),
             which with any tool but the text tool also starts that tool's
             gesture (the text tool on a text, or a double tap with any tool but
             the pen, edits it; with the lasso or the select tool the edit
             waits for a lift where the pen went down, as the second press
             may drag the selection); a drag makes a box or an arrow, whose ends
             bind to the items they land on. An arrow bound to a moved item
             follows it (`follow`): the move's change carries the arrow's
             new ends, so it is still one `canvas_apply`. Each text, shape
             or edit is one change. Pure.
scene/select/hit.rs  which items an eraser's path touches, which items a
             lasso holds and which the select tool's box touches: the core's
             geometry rules (canvas/geometry.rs: `items_erased`,
             `items_in_lasso`), kept here too so they answer at every pen
             event without the socket (P7). Keep the two alike. Which item
             a press picks and which a drawn box takes are the app's alone:
             a drawn box takes a box by its outline (the core's
             `items_in_box` by its whole frame), else the innermost box it
             lies inside. A pdf item is never erased, a lasso holds it by its
             corners, a press picks it inside its frame (ink over it first)
             and a drawn box takes it only whole. Pure.
scene/select/pick.rs  the select tool: a press picks the topmost item (a
             stroke or an arrow by its line, a box by its outline, a text, a
             live item or the button by its frame, 6 screen points of reach)
             and selects it alone, or drags the selection when inside its
             box; a press on empty canvas draws a box that selects what it
             touches (or the innermost box it lies inside, so a box in a box
             can be picked); shift adds or takes out. The lasso's selection, so
             delete, sling, escape and moves act on it alike. Pure.
scene/select/carry.rs  a drag of the selection goes by the canvas pane
             under the pen, read through that pane's own view, at every pen
             event and at the lift: over a pane on the same page the items
             follow the pen and move (one `move`); over a pane on another
             page they show there and go there (on the same canvas one
             `add` and `remove` in one `canvas_apply`, so one undo step;
             onto another canvas an `add` there, then a `remove` here, one
             step on each, a refused add taking the remove with it), and
             are selected there once the server's ids arrive; outside every
             canvas pane they show home and nothing moves. Pure.
scene/select/scale.rs  a drag of a corner handle scales the selection
             about the opposite corner, width and height apart (with shift,
             together by the larger), never below
             5% or turned over: a stroke's points, a box's corners and an
             arrow's ends scale (line widths do not); a text's size scales
             evenly (the geometric mean of the two sides' scales, within the
             text sizes, 4 to 400 units); a button and an agent UI keep their
             size and only their place moves; arrows
             bound to it follow. It shows as the pen goes; the lift writes
             it as one `set` an item, in one `canvas_apply`, so one undo
             step. Pure.
scene/resize.rs  the pen on a divider between panes: the press grabs it,
             each drag resizes the panes at once, the lift names it (the
             number of the pane before it, and its share) for
             `pane_resize`. The pointer within reach of one, or holding
             one, gets its resize cursor. Pure.
scene/overlay.rs  the key overlay (`show-keys`): open or closed, its
             scroll, the row under the pen and the press. While open it
             takes every key (escape and the show-keys key close it, the
             arrows scroll) and every press: one outside the panel or on
             its ✕ closes it; a tap on a row (down and up on it, no drag)
             closes it and runs the command; a drag or a scroll moves rows
             that do not fit. Its rounded panel lists Command::ALL by Group
             in up to three columns (as many as fit 220 pt each, the groups
             cut in order so the tallest is shortest), each row 28 pt with
             a chip of the live key from the Keymap. Line (a label, its font, its corners and edge)
             is shared with the palette. Pure.
scene/palette.rs  the command palette (`show-commands`): open or closed,
             what is typed and the selected row. Typing is scene-drawn: it
             reads each character from the key's Chord (no NSTextField).
             Rows are Command::ALL whose title and group hold the typing as
             a subsequence (a paste goes in too), ranked: title prefix,
             word start, title subsequence, then group. Empty, the command
             it last ran leads, then by Group::ALL with Quit last. 44 pt
             high, each with the group and the live key's chip. While open it takes every key and press; return, or
             a lift on the row the press held, gives back the command,
             which main runs as its key would (as the key overlay's and
             the page menu's rows do). It draws as the key overlay's Overlay: a
             rounded panel, the search with a count, the selected row on a
             filled slab. Pure.
scene/sling.rs  the sling (SPEC section 6): the one-line entry under the
             selection's box (inside its pane), a labelled field: "Sling to
             <agent>" above a 44 pt rounded box with a focus-coloured
             border on the panel colour, and
             beneath it "↩ sends · esc cancels" and the live dictate key.
             Return sends and escape cancels, the wait for the ink, the line
             while it goes, and a failure's message and hint under the entry
             opened again. Pure. `sling/buttons.rs`: the three buttons
             beside a selection's box, ➤ Sling (`sling-now`), + prompt
             (`sling`) and Delete (`delete-selection`), each a 28 pt target
             inside the pane; while a sling is out, Delete alone.
             `sling/pen.rs`: the pen's buttons beside every entry's box
             (the prompt's, a note's, a new name's), ✓ Sling, ✓ Save or ✓
             Rename (Tap::Send, as return), Dictate (`dictate`; ● Stop while
             Flick listens) and ✕ (Tap::Cancel, as escape); the box narrows
             or moves to make room. Every entry shows the live dictate key
             at the end of its keys' one row beneath its box
             (Entry::dictated): a second row aligned right drew over the
             first, as a field does not align styled text.
scene/feedback.rs  the feedback entry (Flick's "leave feedback"): the
             sling's labelled field (Entry::framed), "Feedback" above and
             "↩ saves · esc cancels" beneath, centred in the focused pane
             below the flash with the pen's buttons beside it. Return saves, escape cancels, an empty note
             saves nothing; "saving feedback…" while it goes, a failure's
             message and hint under the entry opened again, and "Feedback
             saved" flashed once on success. The entry, its label, keys and
             lines stand on a card in the panel's colour (Feedback::card),
             not the canvas's. Pure.
scene/dictation.rs  the dictation into the open entry: start, then stop, one
             request to Flick at a time; that Flick listens, or why it
             failed (Flick not running, or its refusal) and what to do, under
             the entry; an answer overtaken by the entry closing is ignored.
             Pure.
scene/tools.rs  the pen's tool, colour and width, and the pop-up at the pen (colour,
             shape, tool, width) that changes them: opens (at the focused
             pane's centre until the pen or pointer has been in the window),
             picks by pen or key, closes. The tool commands pick a tool without it, and
             `width-up`/`width-down` step the width. The width is the pen's
             line alone: boxes and arrows keep one line width. Canvases
             hold it for new strokes (`set_pen_width`). While a tablet pen's
             eraser end is in use the tool is the eraser (footer, cursor,
             gesture); the chosen tool (menu check, flash, pop-up) comes
             back with the tip. Pure.
layout.rs    geometry: sizes in, rectangles out, the canvas viewport (pan, zoom
             about a point or to a level, limits, the level as people read it,
             how far a scroll zooms; the canvas has no edge; a pane's view
             starts with the origin in by a margin; reader mode's fit to
             a column's width, scrolled no further than its ends) and where a pop-up
             opens (at the pen, kept inside the window; each cell at least
             28 pt high, what a pen taps). Pure. 100% coverage floor.
layout/tree.rs  the pane tree: binary splits with ratios to rectangles (no
             negative or oversized one at any size), split, close (the sibling
             takes the space), grow, next, and the pane beside one in a
             direction. Pure.
layout/tree/divider.rs  the dividers between panes: each named by the pane
             before it, the band either side a pen grabs (8 points), the
             share a drag to a point gives its split. Pure.
platform/    draws a scene, turns a key press into a chord (and, when the app
             passes the chord on, into the text it types) and a pen or mouse
             event into a point, a time, whether a finger made it and whether
             a tablet pen's eraser end did (mouse coalescing off: every
             point); `input` does this. A scroll with cmd or ctrl held is a
             zoom about the pointer, as a pinch. A right-button drag is a scroll at
             its press point, so it pans as two fingers do. A tablet
             proximity event says which end of the pen is near
             (Event::EraserEnd). The pointer's cursor is set again from the
             view under it on each move and when the window becomes key
             (a canvas's cursor, the arrow over the chrome). One view per canvas pane repaints
             only the box its scene says changed, and says how long after
             the pen event it painted; its text layout also measures texts
             for scene/glyphs.rs. The window resizes (down to
             layout::SMALLEST) and says its new size as Event::Resized; main
             lays the scene out again at it. All unsafe, objc2 and AppKit
             code. No decisions.
platform/window.rs  makes the window (its style, smallest size and
             delegate) and puts it at its saved frame.
platform/remember.rs  what the window keeps in the user defaults from one
             launch to the next: its frame, and whether the sidebar was open.
             main says whose frame a run keeps (platform::Kept): the app's
             window, or a preview's (`--preview`, which the tests run), saved
             under a name of its own so a preview never moves the app's
             window; a preview never reads or writes the sidebar's. Kept
             also says whether the run comes to the front: a preview in the
             background (`--background`) has no Dock icon and its window is
             never put on a screen.
platform/files.rs  sets the title bar a scene's Title says (title, subtitle,
             represented file), shows a file in the Finder
             (`show-canvas-file`, `show-workspace-folder` and `export-ui`,
             from main), and hands on files dropped on the window.
platform/panel.rs  the open panel, for a Pick: a folder for `open-workspace`
             or `export-ui` (it can make one), or a `.pdf`, `.tldraw` or
             `.tldr` file for `open-pdf`; each with its message and button.
             It runs app-modal with the app in front, the menu bar swapped
             for a bare Edit menu (to the panel's fields) and put back after,
             so no app key shadows the panel's own (⇧⌘D, ⇧⌘O, ⌘1...). The
             panel runs out of process and reads the guarded folders
             (Desktop, Documents, Downloads) as the bundle, whose Info.plist
             (crates/app/Info.plist, scripts/bundle.sh) says why. An agent
             UI's reply while it is up is refused (the app is busy).
platform/menu.rs  makes the menu bar a scene's menus say, again when they
             change. A command item chosen by its key goes to main as that
             key press, so the keymap runs it once; chosen with the pointer,
             as Event::Command. Cut, copy, paste, select all, minimise and
             about go along the responder chain. An undo or redo key main
             passes on (the entry has it) goes on as undo: or redo: along
             the responder chain, to the field's own undo.
platform/keys.rs  a key press as a chord and the text it types (none with
             cmd held), and the clipboard as a Clip: its plain text and a
             canvas's items under the app's own type
             (`com.github.jayminwest.slinger.items`). With no text field
             focused, the window answers cut:, copy: and paste: (the Edit
             menu and its keys) as Event::Edit, and puts the clip of a
             Step::Copy on the clipboard; its validateMenuItem: asks main
             (Event::Can) and greys the item on a pass. Flick's dictation
             pastes, so it lands there too.
platform/overlay.rs  draws a scene's Overlay over the chrome: a box that
             dims the window, then per line a rounded box for its fill and
             edge and a text field centred top to bottom; the scene's pills
             the same way, over the labels.
platform/chrome.rs  the focus border, and the flash's pill (a rounded box
             with its text) with the one-shot timer that ticks when it goes;
             the plain label, the flip to AppKit's coordinates and the
             colour every view is made with.
             Each canvas view sets the scene's cursor over itself (a ring and
             the diagonal resize arrows are drawn once).
platform/entry.rs  the open entry (sling, feedback, rename): a native text
             field in its box, with its label and keys, kept while the scene
             has one, over every view; a new field for each kind of entry.
             One focus path: each draw, and the window becoming key again,
             give it the keyboard if it lost it, and each time it takes the
             keyboard its caret is set to the text colour. Each edit goes to
             main as Event::Typed. The Edit
             menu's cut, copy, paste and select all reach it along the
             responder chain: Flick's dictation pastes with cmd+V.
platform/web.rs  agent UIs in WKWebViews: the scheme handler, navigation and
             UI delegate and message handler in one class that pipes each
             call to its ui::Bridge, the rule list (compiled once; no view
             loads before it is on), and a view kept across draws while its
             page is shown; a live item's in a clip at its pane, just over
             the pane's canvas, at the canvas's pageZoom, hidden while it
             is away. A reply goes to main as Event::Reply.
platform/pdf.rs  every CoreGraphics PDF call: a PDF's page sizes as displayed
             (crop box, `/Rotate` applied; on any thread), a pdf item's part
             of its page drawn stretched into its frame on white (each
             document opened once).
             backdrop: one sling area as raw RGB at the packet's scale, and
             the page's words there (PDFKit) in canvas tenths.
```

**Threading rule:** AppKit runs on the main thread only. Every `platform`
function that touches AppKit takes a `MainThreadMarker`, which only the main
thread can make, so the compiler holds the rule. Long blocking work runs
before the run loop starts or on another thread, which hands its result to
the main thread to draw. Never block for long inside an AppKit callback.

**Config:** `config.example.toml` at the repository root lists every key.
The app reads `config.toml` and merges `config.local.toml` (the machine's
overlay) over it. A main-thread timer stats both files twice a second and,
on a change, rereads them and draws a new scene. The agents' statuses and
the workspace's panes come from `backend::Watch`'s one thread, so a daemon
that is slow to answer never holds up the window; the timer takes its
latest answers and draws only when the scene changed. A pane opened from a
terminal shows within a second, and a daemon that cannot be reached shows
as the problem. The same timer drains the canvas followers.
The release profile builds the app at `opt-level = "z"`. A file
with an error keeps the last good config, and the window shows the file,
the line and what to fix.
A new setting changes `config.rs` (the key, its default, its test) and the
example file, which a test checks against the keys.

**Canvases:** each canvas a pane shows has one follower thread, which
blocks on the daemon's change feed (a long wait, so an idle canvas costs no
requests) and sends what it hears down a channel. The main thread drains
the channels on its half-second timer, applies the operations to the page's
mirror and builds a new scene: a change shows within about half a second.
A lost daemon or canvas is a problem in the pane, and the follower tries
again each second.

**Ink (P7):** the pen draws from local state, never from the server. A
press with the pen tool on a canvas pane starts a stroke in `scene::ink`,
each drag adds a point and draws a scene at once (a stroke that only grew
repaints only its new end), and the lift finishes it. Finished strokes go
to the `Outbox` thread one at a time, oldest first, each as one `add` (one
undo step); the main thread never waits on the socket. A stroke the daemon
did not get stays on screen and goes again before any after it; one it
refuses (its canvas is gone) is dropped and shown as the problem. Local
ink and the server's echo: a stroke draws from the ink until its canvas's
feed has passed the line its `add` was written under; from then on the
mirror holds it. So it never blinks out between the reply and the echo.
The daemon's reply to an add says the ids it made, and the ink shows the
item under that id from then on (and not at all once the mirror holds
it), so it never draws twice. Touch (a palm on a
tablet display) never draws. Each stroke's pen-to-paint delays go to
stderr as one `ink:` line (SPEC O2). Changes not yet written (strokes,
erases, moves, puts, edits, resizes, carries and page changes; not an undo
or a redo, which has no `once`) are kept in `unsent-ink.json` beside the
daemon's record (`backend::Unsent`, rewritten whole each time they change,
removed when none are left) and queued first at the next start, in order,
so what the user did while the daemon is down survives a quit. Each keeps
the ink's key it had, so a kept change naming an item a kept stroke adds
(by an id of the ink's own) is renamed to that stroke's id in the new run
(`Ink::resume`). A pane waiting for the page a kept page add makes is
kept with it (`turning`, by `PaneId::number`, which the same layout gives
the same pane again) and turns to the page once it is added. One app at a
time keeps the file: it holds a lock on
`unsent-ink.json.lock` while it runs, and a second app started beside it
keeps nothing and says so, rather than write over the first's changes. Every `canvas_apply` goes
with `once`, a key of this run and the change (`scene/ink/once.rs`), and a
kept change keeps its key in the file: the daemon applies a request sent
again under a key it applied lately only once, so a stroke written as the
app quit, or a change whose answer was lost, is not written twice. The app binary has a size budget, and the release
profile builds it at `opt-level = "z"`, so it adds no async runtime.

**Eraser, lasso and select tool:** they work on the page as the user sees it (the
mirror with the ink laid over) and answer under the pen from the app's own
copy, by the core's rules (`scene/select/hit.rs`): a round trip to the
daemon per pen event would make the main thread wait or the eraser trail
the pen. Ink not yet written counts too: it shows under an id of the ink's
own until the daemon says the id it made, and a change that names it is
renamed to that id before it goes (`scene/ink/ids.rs`), so it is still one
`canvas_apply` and one undo step; the selection follows the rename. A
change left naming only items never made (their add was refused) is
dropped unsent. Copy and a carry to another page take the items as they
show too (`Mirror::carried`): the mirror's object of each, or the object
the ink's change writes for one not yet written, laid over with how it
shows (`mirror/write.rs`). A finished erase, move or delete joins the ink's queue
as one `canvas_apply`, in turn with the strokes, and shows from there until
the feed brings it. The user may erase, move and delete any item, an
agent's too. The eraser takes strokes, text, boxes (by their outline, so
erasing the ink inside a box keeps the box) and arrows; never a live item
or the button, which are not ink. A tablet pen's eraser end erases with
any tool chosen. Delete and clear act only when the focused pane shows the
selection's page; otherwise their keys go on (a delete in an agent UI).

**Pages and undo (SPEC D6):** each pane keeps its own page and viewport;
a page shown in two panes is followed once. The pane's page name is its
one quiet line, at its foot. Undo and redo are the daemon's (`canvas_undo`,
`canvas_redo`, per author, as `user`), so the user's undo never takes back
an agent's item. They and the page changes wait in the ink's queue, so an
undo pressed right after a stroke undoes that stroke, and show when the
feed brings them. A reply that wrote no line (nothing to undo) has nothing
to wait for. A carry between pages is two steps (the add there, then the
removal here); on one canvas, one undo takes back the removal only.

**Between panes (SPEC section 11):** a press within 8 points of the divider
between two panes grabs it (before any tool sees the press), each drag
resizes the panes at once, and the lift sends `pane_resize` once, so the
workspace keeps the size; the grow and shrink keys send the same. A drag of
the selection that lifts over a pane on another canvas carries its items
there: the mirror gives their canvas-file objects, moved and on that page
(an arrow's ends, which name items here, dropped), and the ink queues an
`add` there and then a `remove` here, in turn with the strokes.

**Sling:** the `sling` command opens a one-line entry at the focused
pane's selection; return hands the selection (server ids only, so ink still
on its way is never in it) and the prompt to `canvases`, which holds it
until every change of that canvas made before it is written
(`Ink::settled`; a daemon that cannot be reached does not hold it), then
gives it to the `Slinger` thread as one `sling_send`, with the page under
any new ink over a PDF rendered first (`sling_backdrops`). The main thread never
waits on the socket. No chat view: the answer comes on the canvas through
the change feed and the agent's status in the sidebar. A failure opens the
entry again with its prompt, and the error's message and hint under it.
The `dictate` command speaks the prompt (SPEC D2): Flick's dictation starts,
the next press stops it, and Flick types the text into the entry; slinger has
no speech code. A Flick not running, or refusing, shows once under the entry.

**Keys:** every action is a named command (SPEC P4), bound to a chord in the
config's `[keys]` table. The window's `sendEvent:` turns each key press into
a chord and hands it to `main`, which looks it up in the keymap: a command
runs and the press stops there; any other key goes on to the window as
usual. The menu bar (commands/menu.rs) shows each command's live key, but
a command's key still runs through the keymap: a menu item that AppKit
fires for a key hands main that key press, so a command never runs twice
and the entry, the overlay and a pop-up still take their keys first. A key
that types (no cmd or ctrl) stays off the menus, which would take it from a
text field. Cut, copy, paste and select all keep their usual keys unless the
keymap binds them (the window's close button quits too). Cut, copy and paste
with no text field focused come to main as Event::Edit (platform/keys.rs,
window.rs's `edit`), greyed when they would do nothing, and a key with cmd
held never types.
The sling's open entry takes every key before the keymap but the `dictate`
chord and the keymap's other chords with cmd or ctrl that a text field does
not use (`scene::sling::edits`): return and escape are its, a key that types,
an edit (cmd+X/C/V/A/Z, cmd or ctrl with an arrow or delete, ctrl with a
letter) or a chord the keymap leaves free goes on to the text field (delete
deletes a character there, not the selection), and cmd+Q still quits. The
Edit menu's Undo and Redo take their keys before the field, so a passed
undo or redo key goes to it as undo: or redo: (platform/menu.rs).
An open pop-up at the pen takes its own keys first (escape, return, space,
the arrows, 1 to 9); any other key goes to the keymap. These keys act only
inside a pop-up a command opened, so they are not commands themselves.
A new command adds a variant and a name to `Command::ALL` in
`commands.rs`, a title and a group in `commands/registry.rs`, an item in
one menu of `commands/menu.rs` (a test holds it), an arm in
`main`'s match, and a line in the example file (with its default chord, if
it has one).
## Boundaries

- Dependencies point inward. Front ends depend on the core. The core never
  depends on a front end.
- Each capability has one implementation, in the core. A front end that
  reimplements logic is a bug.
- Every failure a caller can act on is a typed error with a stable code.
  Front ends map codes to exit codes or statuses. They never parse messages.

Each boundary has something that fails when it is broken:

| Boundary | Held by |
|---|---|
| The core does no process, terminal or network IO and no FFI | `check:layers` rule `core-is-pure` |
| The protocol crate does not link the core | `check:layers` rule `control-is-transport-only` |
| The CLI reaches the core only through the socket | `check:layers` rule `cli-uses-the-socket` |
| The app reaches the core only through the socket | `check:layers` rule `app-uses-the-socket` |
| The agent guide, the skill, the plugin and the sling note (the core's stored snapshots) name only commands and flags the CLI has; the plugin has the binary's version | `crates/cli/src/tests/guide.rs` |
| The record's event and blob-id formats are readable by older readers | `crates/core/tests/record.rs`, with stored fixtures |
| The canvas file format is readable by older readers | `crates/core/tests/canvas_file.rs`, with a stored v1 fixture |
| The wire format (docs/service.md) | the tests of `crates/control/src/wire.rs` |
| The client takes a reply slower than 5 s (the daemon under disk load) as an answer, not `unavailable`; it waits 30 s at most, and no reply in that time hints to retry or restart, not to start the daemon (docs/service.md) | `crates/control/src/client.rs` tests, with a fake daemon |
| The packet's line format (docs/design/packet.md); no agent item is rendered | `crates/core/tests/packet.rs`, with a stored snapshot |
| The packet's image rules: scale, margin, tiles, same bytes for the same input | `crates/core/tests/render.rs`, with golden images |
| The packet format reads back, an old packet keeps decoding, the board costs under half its screenshots | `crates/core/tests/packet_build.rs`, with a stored v1 packet, on the real board |
| The sling message: first, later and listener snapshots; the note names only the verbs given, and points at `slinger guide` when there is one | `crates/core/tests/sling.rs`, with stored snapshots |
| Clustering: colours never mix, long strokes join only ink they touch, each fixture page's count | `crates/core/tests/packet_cluster.rs`, on the real board |
| The workspace file format and rules (docs/workspace.md) | the tests of `crates/core/src/workspace.rs` and its stored fixture |
| The record on disk is append-only and keeps fields it does not know | `crates/daemon/src/store/tests.rs`, against a real file |
| One daemon per socket; a stop loses no reply | `crates/daemon/src/tests.rs`, through a real socket |
| A blob of megabytes comes back byte for byte; a put twice gives one id | `crates/daemon/src/tests.rs`, through a real socket |
| Every UI version stays; the same bytes add none; an export is the bytes put | `crates/daemon/src/tests.rs`, through a real socket |
| A canvas survives a restart; concurrent changes are numbered with no gap | `crates/daemon/src/tests/canvas.rs`, through a real socket |
| `canvas read`: only ink after the marker is new; each packet and image is a blob; the stored text has no `at` | `crates/core/tests/canvas_packet.rs` and `crates/daemon/src/tests/read.rs`, through a real socket |
| An imported canvas reads back with the importer's pages, ids and arrow ends | `crates/core/tests/canvas_import.rs` and `crates/daemon/src/tests/import.rs`, on the real board |
| Open workspaces survive a restart; each has its canvas; one primary root per workspace; a broken file is listed with its error, fails every verb on it, and leaves the others served | `crates/daemon/src/tests/workspace.rs`, through a real socket; `crates/cli/src/tests/workspace.rs` |
| The change feed wakes a waiting client on another's change; a stop ends a wait | `crates/daemon/src/tests/feed.rs`, through a real socket |
| SIGTERM ends the daemon cleanly, with a client waiting on a feed | `crates/daemon/tests/signals.rs`, against the real binary |
| A crop is limited in size; an empty box is a white image; an unknown id is `not_found` | `crates/core/tests/canvas_look.rs`, and `crates/daemon/src/tests/look.rs` through a real socket |
| A status set shows in the session list and the record; blocked and done survive a restart | `crates/daemon/src/tests/session.rs`, through a real socket |
| A put covers nothing: right of the item, else below, left, above, else further right | `crates/core/tests/canvas_put.rs` |
| A put takes a colour and a place; ink goes where any put goes; an arrow takes no place | `crates/core/tests/canvas_put.rs`, `crates/cli/src/tests/put.rs` |
| A put shows with its author; after a sling it lands `reply_landed`; an unknown item is `not_found` | `crates/daemon/src/tests/put.rs`, through a real socket |
| An agent edits and removes only its own items; removing twice succeeds twice | `crates/core/tests/canvas_edit.rs`, and `crates/daemon/src/tests/edit.rs` through a real socket |
| An agent's move or resize (edit or apply) takes its bound arrows' ends along in one step, not another author's | `crates/core/tests/canvas_edit.rs` |
| Three sends reach one process under one session id and their answers are read; a restart resumes it; a dead process makes a send `unavailable`; a daemon stop ends it; a resume that fails drops the id and the next start is new; a fresh start ends the running process and begins a new session; a stop mid-turn ends the agent's `working` | `crates/daemon/src/tests/process.rs`, with a stand-in for Claude Code |
| A session starts with the exact limits its workspace sets: by default its roots, every built-in tool and its MCP servers, the `slinger` CLI and the rules that write files, use the web and skills without asking, and the owner's user config; a profile has only the tools its rules name; wider with the agent's `dirs` and `tools`, narrower with `user_config = false` | `crates/core/src/session/stream.rs`, and `crates/daemon/src/tests/process.rs` with a stand-in for Claude Code |
| A profile is set, listed and removed in the workspace's file, and a bad name, rule or dir is refused loudly with nothing written; a session under one runs in its dir with its rules in place of the agent's and resumes its own conversation; a sling that names one reaches only a process under it | `crates/core/src/workspace/profile.rs`, `crates/core/src/session/stream.rs`, `crates/daemon/src/tests/profile.rs` with a stand-in for Claude Code, `crates/cli/src/tests/profile.rs` |
| Three slings go into one conversation, each with its packet blob and `sling_sent`; old ink is not sent again; a sling that cannot go stores nothing; each verb the note names is served | `crates/daemon/src/tests/sling.rs`, with a stand-in for Claude Code |
| A sling with no process waits for a listener, who takes it as text with its images as files and whose put answers it; a listener outranks a running process and a sling that names a profile goes only to a process; a listener that is gone (its reply unread, or the process it named ended mid-sling) loses no sling, `done` ends its work, a stop ends its `working`, and a daemon stop ends an open ask | `crates/core/src/session/listen.rs`, `crates/daemon/src/tests/listen.rs` through a real socket, `crates/cli/src/tests/sling.rs` |
| An agent verb takes its agent from `--agent` or `SLINGER_AGENT`, else exits 2 | `crates/cli/tests/agent_env.rs`, against the real binary |
| The hooks: each event runs its CLI command and none exits 2; `file_touched` stores a path and a hash, no contents | `crates/core/src/session/hooks.rs`, and `crates/daemon/src/tests/touched.rs` through a real socket |
| A turn's hooks move a session from working to done and record the file it wrote | `crates/cli/tests/hooks.rs`, with a stand-in for Claude Code and the real binary |
| Only `platform` holds `unsafe`, objc2 or AppKit code | `check:layers` rule `platform-owns-ffi`, and `unsafe_code = "deny"` everywhere else |
| Only `main.rs` starts the platform | `check:layers` rule `app-logic-is-pure` |
| Only `platform/pdf.rs` reads a PDF (CoreGraphics, PDFKit or a PDF crate); the core, the daemon, control and the CLI never parse one | `check:layers` rule `pdf-lives-in-platform-pdf` |
| A layout reads back as written; a closed pane's space goes to its sibling | the tests of `crates/core/src/workspace/panes.rs`, `crates/app/src/layout/tree.rs` and `crates/app/src/panes.rs` |
| Panes open, close, resize and list through the socket, each change records `pane_moved`, closing a pane already gone succeeds, the layout survives a restart; the app follows another client's change | `crates/daemon/src/tests/pane.rs` through a real socket, and `crates/app/src/backend.rs` |
| An agent opens a UI with data; each reply is recorded and reaches its running session as one JSON line, in order; a reply for a stopped session stays in the record; a refused open records nothing; replies are bounded | `crates/daemon/src/tests/showing.rs`, with a stand-in for Claude Code, and `crates/core/src/ui/showing.rs` |
| A canvas page reads every kind from the canvas file and follows each feed operation | `crates/app/src/mirror/tests.rs` |
| A live item is one `add` line an old reader skips whole; a put beside an item is one undo step; the packet says it as text | `crates/core/tests/canvas_live.rs` |
| An agent opens a UI as a live item: a refused open records nothing; the item, the showing and its data come back; one undo step | `crates/daemon/src/tests/live.rs`, through a real socket |
| A live item's page shows over its box, follows a pan and a zoom, and takes presses until it is selected | `crates/app/src/scene/live/tests.rs`, and `crates/app/src/canvases/live/tests.rs` against a real daemon |
| A button is one `add` line that needs an area; the packet says it as a `u` line; an agent puts beside it but cannot remove it | `crates/core/tests/canvas_button.rs` |
| A workspace's new canvas starts empty; the `sling` entry opens beside a page's button | `crates/daemon/src/tests/workspace.rs`, `crates/app/src/canvases/button/tests.rs` |
| The button's mark and colour follow its agent's status (none, working, blocked, done), scale with the canvas and repaint on a change | `crates/app/src/scene/button/tests.rs` |
| a button on the page (put through the daemon); a tap slings what is new with no selection, the reply lands beside the note, a typed prompt goes with a tap, a failure shows under the button, the eraser over it slings nothing | `crates/app/src/canvases/button/tests.rs` against a real daemon and a stand-in for Claude Code |
| A follower shows the page, hands on each change, and retries a lost daemon | `crates/app/src/backend/follow.rs`, against a real daemon |
| A canvas scene draws only what is visible and repaints only what changed | `crates/app/src/scene/canvas/tests.rs` |
| Ink shows from the pen until the feed brings it; strokes go one at a time in the order drawn, a failed one again first | `crates/app/src/scene/ink/tests.rs` |
| Ink not yet written is selected, moved and erased at once; each change goes in turn by the ids the daemon made, one undo step each | `crates/app/src/scene/ink/tests.rs`, and `crates/app/src/canvases/tests.rs` against a real daemon |
| Changes not yet written survive a quit and are sent first after the next start, in order, still naming the kept strokes they name; a refused one leaves the file; one written as the app quit is not written twice; two apps keep a file each | `crates/app/src/canvases/kept/tests.rs` and `crates/app/src/backend/unsent/tests.rs`, against a real daemon |
| A `canvas_apply` sent again under its `once` key is applied once and answered the same | `crates/core/tests/canvas_log.rs`, and `crates/daemon/src/tests/canvas.rs` through a real socket |
| An erase, a move and a delete are one change each and show at once; a lasso selects what it holds whole; the selection clears | `crates/app/src/scene/select/tests.rs` |
| The select tool picks the topmost item under a press, a drawn box selects what it touches, shift adds or takes out, a drag moves the selection | `crates/app/src/scene/select/pick/tests.rs` |
| A drag of a corner handle scales the selection about the opposite corner in one change, a text's size scales evenly within its range, bound arrows follow, no side turns over, shift keeps the shape | `crates/app/src/scene/select/scale/tests.rs` |
| A selection dragged into a pane on another canvas is added there and removed here, one step each, the removal only after the add; one dropped outside every pane moves nothing | `crates/app/src/scene/select/carry/tests.rs` |
| A pen grabs a divider within 8 points of it; a drag sets its split's share, held in limits | `crates/app/src/layout/tree/divider/tests.rs`, `crates/app/src/scene/resize.rs` |
| The outbox writes strokes in order as the user; a lost daemon is unreachable, a missing canvas refused; a move and a removal are one undo step each and reach an agent's items; an undo sent after a stroke undoes it and never an agent's item; page changes are written | `crates/app/src/backend/outbox.rs`, against a real daemon |
| Page commands turn, name and remove pages; removing a page with items asks once | `crates/app/src/scene/pages/tests.rs` |
| Every footer button is at least 28 pt square and none overlap; a narrow footer drops whole groups, the lowest rank first; the sidebar button sits in the same spot open or collapsed | `crates/app/src/scene/status/tests.rs` |
| A press on a button goes no further and only the lift on the same button taps it; the open page menu takes every press, lists the pages with the shown one marked and turns the pane to the one tapped | `crates/app/src/scene/taps/tests.rs`, `crates/app/src/scene/pages/menu/tests.rs`, `crates/app/src/shared/tests.rs`, and `crates/app/src/canvases/tests.rs` against a real daemon |
| A `pdf` item is one `add` line that reads back; its blob id, page, page size and crop are checked; it draws under every other kind and the eraser never takes it; the packet names a page only under new ink or selected, and a crop draws it as a grey frame | `crates/core/tests/canvas_pdf.rs` |
| A sling over a PDF asks for the areas under the new ink, draws the page under the ink and ends each ink line with the PDF's words under it; ink drawn between the two requests is on white; a backdrop file that is missing or wrong stores nothing | `crates/core/tests/canvas_pdf.rs`, `crates/core/src/packet/words.rs`, `crates/daemon/src/tests/sling_pdf.rs` through a real socket, `crates/app/src/backend/sling/tests.rs` |
| An opened PDF is a canvas named after its file, one item per page stacked 1600 units wide, in a pane beside the focused one; a PDF that cannot be read makes nothing; a blob is fetched once, and fetches of it at once each get it whole | `crates/app/src/backend/pdf/tests.rs` against a real daemon, `crates/app/src/scene/pdf/tests.rs` |
| Reader mode fits a PDF's canvas to the pane's width and scrolls up and down only; there the select tool and the lasso never take a page; a pdf item resizes keeping its aspect | `crates/app/src/canvases/reader/tests.rs`, `crates/app/src/scene/select/tests.rs`, `crates/app/src/scene/select/scale/tests.rs` |
| A crop box maps to page points inside the page, and its snippet pastes as a cropped pdf item | `crates/app/src/scene/crop/tests.rs`, `crates/app/src/mirror/clip/tests.rs`, `crates/app/src/tests.rs` |
| Layout is fully tested | the 100% floor on `crates/app/src/layout.rs` in `budgets/coverage-budgets.json` |
| The app binary does not grow unnoticed | `check:binary-size` |

The rules are in `budgets/layer-rules.json`. To exempt one file, add an
`allow` entry with a `why`. An entry that matches no violation fails the gate.

## Adding things

- **A capability:** add it to the core with tests. Then expose it from each
  front end that needs it.
- **A command:** see "Adding a command" in `docs/cli.md`.
- **A verb:** see "Adding a verb" in `docs/service.md`.
- **A command a person would use:** it comes to the app too, as a command in
  `Command::ALL` (palette, key overlay and menu bar) or a tap target, or
  with a row in `docs/app-parity.md` saying why not or naming the seed
  that brings it.
- **Something on screen:** get the data in `backend.rs`, decide what to show
  in `scene.rs`, place it in `layout.rs`, and draw it in `platform/`. If
  `platform` needs an `if` about content, the decision is in the wrong module.
- **A front end:** add it as a new consumer of the control socket. Do
  not move logic out of the core to make that easier.
- **A boundary:** add a rule to `budgets/layer-rules.json`, or a test, and a
  row to the table above.
