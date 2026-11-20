# Canvas storage: canvases as files in the workspace folder

Status: picked overnight 2026-11-20 (slinger-7c18bc), owner to confirm.
Nothing here is built yet. Inputs: SPEC D6, D15, D19, K5, O8; the canvas file
(`docs/design/canvas-file.md`); the workspace file (`docs/workspace.md`); the
measurements in `docs/perf/canvas.md`; the owner's decision of 2026-11-20 in
the seed: canvases become visible files in the workspace folder, with their
blobs alongside, and a design doc comes first.

Each question below gives the alternatives, then the section "Recommended
(picked overnight, owner to confirm)" at the end picks one per question. The
plan's later steps build on that section.

## Not reopened here

- **The format stays.** Alternative A of `canvas-file.md`: one append-only
  JSON Lines log per canvas, one `write` and one `fsync` per change. No
  database for canvases. Moving a file changes where it is, not its bytes.
- **The owner's decision**: canvases go in the workspace folder, blobs with
  them. The questions are how, and how to get there without losing a canvas.
- **One writer per canvas (K5).** Two machines writing one synced file is
  not supported. This design detects it and keeps both sides; it never
  merges.

## Today (read from the code, 2026-11-20)

- Every canvas is `canvases/<id>.canvas` in the daemon's data directory
  (`crates/daemon/src/store.rs`, `CANVASES`; `Canvases::path` in
  `crates/daemon/src/canvases.rs`). The file name is the id; `canvas_list`
  lists the directory.
- A workspace's `slinger-workspace.toml` sits in its folder; its canvas does
  not. `Workspaces::open` (`crates/daemon/src/workspaces.rs`) writes the toml,
  then calls `canvases.create(id)` and ignores `conflict`. So a synced folder
  opened on a second machine gets a new empty canvas of the same id, and a
  new folder with a closed workspace's name takes over that canvas.
- A canvas is read once and then held open: the daemon keeps the `File` and
  appends to it until it stops. It never looks at the path again. A sync tool
  that replaces the file (a temp file renamed over it) leaves the daemon
  appending to the old, unlinked inode: every later change is acknowledged
  and lost. That is harmless today only because nothing else writes into
  `canvases/`.
- PDF and live-UI items name a blob by `hash`; the blob is only in this
  machine's `record.sqlite` (O8). A copied `.canvas` loses its PDFs.
- The only guard is `record.lock` on the data directory. Two daemons with two
  data directories (the owner's and a test's) never share a canvas today,
  because each has its own `canvases/`. Once canvases live in folders they
  can: both can open the same workspace folder.
- `canvas_show` already returns the file's `path`; the app shows it in the
  title and in "show in Finder". So a new path reaches the user with no app
  change.

## Measurements this design rests on

From `docs/perf/canvas.md` (synthetic canvases from
`cargo run --release -p slingerd --example synthetic_canvas`; scale 1 is
5.7 MB, scale 5 is 28.9 MB):

- Opening a canvas (the first `canvas_show`) costs about 19-21 ms per MB:
  104.9 ms at 5.7 MB, 596.2 ms at 28.9 MB. Moving the file to another local
  folder does not change this: it is the replay, not the disk.
- Adding a stroke costs 4.5-5.2 ms at every size, "mostly the fsync"; at load
  6.4 it went to 14.5 ms, because it waits on `fsync`. Every change pays one
  sync, wherever the file is.
- The warm `canvas_show` of one page and the packet and backdrop replays grow
  with the file (22.5 to 306 ms; 508 to 2 383 ms). They are in memory and do
  not touch the file; the location does not change them.

Taken for this doc (2026-11-20, same machine, load 1.6-2.5, other agents
idle; `F_FULLFSYNC` as Rust's `sync_all` uses on macOS; 7 runs each, median
then max; a throwaway script in the scratch directory, not in the repo):

| step of a move | 5 658 561 B (scale 1) | 28 888 569 B (scale 5) |
|---|---|---|
| copy to a temp file, sync, rename, sync the directory | 11.5 (15.5) ms | 16.3 (18.0) ms |
| SHA-256 of the source and of the copy | 6.7 (8.0) ms | 27.2 (36.8) ms |
| a rename in one directory, then sync the directory | 5.0 (5.6) ms | 4.8 (5.2) ms |
| one `stat` of the file | 1.8 µs | 1.2 µs |

So moving one 28.9 MB canvas, checked byte for byte, is about 50 ms, and
checking that the copy opens (a replay) is the 596 ms above. A `stat` before
each append is about 1/3000 of the 5 ms an append already costs.

Not taken:

- **A synced or slow disk.** No network volume is mounted here, and the
  owner's `~/sync` folders hold private data and are not touched. The cost
  of an `fsync` on SMB, NFS or a disk that a sync client is reading is
  unknown. The pick below keeps canvases off network file systems for that
  reason.
- **The owner's real canvases** (10-15x the largest here). Scale 5 is the
  low end of that range; the move cost above is linear in size (copy and
  hash), so 60 MB is about twice the scale-5 row.

## Q1. Where in the folder does the canvas go?

- **A. `<dir>/<id>.canvas`, beside `slinger-workspace.toml`.** One visible
  file per canvas, named by its id. `<dir>` is the directory that holds the
  toml (the primary root, for a workspace opened from a folder). Finder,
  `ls`, `git status` and the sync client all see it.
- **B. `<dir>/slinger/<id>.canvas`.** One visible folder for everything
  slinger puts there. Tidier with many canvases; one level deeper, and the
  toml still sits outside it.
- **C. `<dir>/.slinger/canvases/<id>.canvas`.** Hidden. Against the owner's
  "visible files", and some sync tools skip dot folders by default.

## Q2. How does the daemon find a canvas by its id?

Verbs, the CLI, agents, the layout (`canvas:main@1`) and the app all name a
canvas by its id, and should keep doing so.

- **A. A link file in the data directory.** `canvases/<id>.canvas` stays,
  but for a moved canvas it holds only a header that points at the file:
  `{"slinger":"canvas","v":2,"id":"main","at":"/Users/me/notes/main.canvas"}`.
  The data directory's `canvases/` is then the index, one atomic file per
  canvas. An older slingerd reads `v` 2 as `canvas_too_new` ("upgrade
  slinger") and never writes to the file (`read_header` in
  `crates/core/src/canvas/file/mod.rs` checks `v` before anything else), and
  `canvas_create` of that id is `conflict`, which `workspace_open` already
  ignores: an old build can neither lose the canvas nor make an empty twin.
  It burns version 2 of the canvas header on a link; a real format change
  would be version 3.
- **B. One index file**, `canvases/index.json`, `{id: path}`. One more file to
  keep consistent with the canvases (written whole, temp and rename, for
  every create and move). An older slingerd ignores it, sees no file for a
  moved id, and makes an empty canvas: the silent twin again.
- **C. No index: derive the path from the open workspaces.** A canvas named
  by an open workspace is `<dir>/<id>.canvas`; any other is in the data
  directory. No second source of truth, but a closed workspace's canvas
  becomes unreachable by id, a moved folder loses its canvas with no trace,
  and an older slingerd makes the empty twin.

## Q3. Where do the blobs go?

A `pdf` item names a PDF's blob; a `live` item names a UI version's blob.

- **A. `<dir>/.slinger/blobs/sha256-<hex>`, written through.** When a folder
  canvas gains an item that names a blob (an `add`, `restore` or `set` with
  a `hash`), the daemon writes that blob there too: write once, temp and
  rename, never changed (the name is the hash). The record stays the first
  place the daemon looks and becomes a cache: a blob missing from the record
  is read from the folder and stored. Content-addressed files are not for the
  user to edit, so they sit in a dot folder; the canvas is the visible
  document.
- **B. Embed the blob in the canvas log** (base64 in the `add` line). One file
  to move, but a PDF of several MB in one JSON line breaks `grep`/`jq` use,
  is rewritten on every compaction, and bloats every replay.
- **C. A visible `<dir>/<id>.blobs/` folder per canvas.** Visible, but two
  canvases of one workspace that show one PDF keep it twice, and the user
  sees a folder of hex names.
- **D. Leave blobs in the record (O8 as it is).** A synced or committed
  canvas keeps losing its PDFs and UIs on the other machine.

UI code stays in the record by D19 (the catalog of names and versions); only
the versions a canvas's `live` items show are copied beside it.

## Q4. How is a canvas file guarded against a second writer?

- **A. `flock` on the canvas file itself, plus an identity check before every
  append.** The daemon takes `flock(LOCK_EX | LOCK_NB)` on the file it holds
  open, so a second daemon on this machine (a test daemon on another data
  directory, an older build) cannot write it: that one opens it read-only
  and every change is `conflict`, hint "another slingerd on this machine
  holds <path>". Before each append it `stat`s the path and compares device,
  inode, size and modification time with what it last wrote (1-2 µs). On a
  difference the file changed outside slinger (a sync, a `git checkout`, an
  editor): the daemon copies the bytes it holds (the old inode is still
  readable through its open file) to `<id> (slinger <machine> <time>).canvas`
  beside it, opens the new file, refuses the change with `conflict`, hint
  "<path> changed outside slinger; your copy is <name>", and says so on
  stderr. Nothing acknowledged is lost, and nothing is merged.
- **B. A lock file `.<id>.canvas.lock` with machine and pid, in the folder**
  (the seed's direction). Works across machines only as fast as the sync
  carries it, goes stale when a machine dies, and is one more file in the
  user's folder and in their sync conflicts.
- **C. No guard.** Today's code, with the lost-appends case above.

## Q5. How do existing canvases get there?

The riskiest change in the project: the owner's canvases are the data.

- **A. An explicit move first, automatic later.** Step one is a verb,
  `canvas_move` (`slinger canvas move <ID>`, and `--back`), that moves one
  canvas into its workspace's folder or back. The owner moves one canvas,
  uses it for a while, and moves it back if anything is wrong. Only after
  that does a later build move the rest on start.
- **B. Automatic, on the first start of the new build.** Every workspace
  canvas moves at once. The fastest path, and the one with no rehearsal.
- **C. Never move old canvases; only new workspaces get folder canvases.**
  Nothing old is at risk, and the owner's synced workspaces keep the bug the
  seed is about.

The move, either way (one canvas, the daemon holding `record.lock`, the
canvas's own lock taken so no change runs during it):

1. Copy `canvases/<id>.canvas` to `<dir>/.<id>.canvas.new`, sync, rename to
   `<dir>/<id>.canvas`, sync `<dir>`. If `<dir>/<id>.canvas` exists already
   with other bytes, stop: that is Q6's case, not a move.
2. Check the copy: same SHA-256 as the source, and it opens (`Log::open`).
3. Link the original into `canvases/moved/<id>.canvas` (a hard link: the same
   bytes, no copy), sync.
4. Write the link header of Q2 to `canvases/.<id>.canvas.new`, sync, rename
   it over `canvases/<id>.canvas`, sync `canvases/`. This rename is the
   moment of the move.

A crash before step 4 leaves the original in place and a copy that the next
move overwrites (same bytes) or finds identical; a crash after it leaves the
link, the moved file and the backup. Every step is a whole file or none, and
at every point the original's bytes exist in the data directory. The backup
in `canvases/moved/` is kept until the owner removes it. It holds the
canvas as it was at the move, so it costs the canvas's size at that moment
(28.9 MB for a scale-5 canvas) and is never written again.

**Back** (`canvas move --back <ID>`, the rollback): copy the folder file's
current bytes to `canvases/.<id>.canvas.new`, sync, rename it over the link,
sync. The folder file stays (with a note on stderr), so a synced machine
still has it. By hand, with slingerd stopped, the same rollback is one copy:
`cp <dir>/<id>.canvas "<data dir>/canvases/<id>.canvas"`, because the format
never changed.

## Q6. A folder that already holds the canvas, or a twin

Opening a synced folder on a second machine: `<dir>/main.canvas` is there.

- **A. Adopt it.** `workspace_open` sees `<dir>/<canvas>.canvas` and writes a
  link to it instead of making an empty canvas. If the data directory already
  has a real `main.canvas` (the twin the old bug made): when the twin holds
  no operations (header only), it moves to `canvases/moved/` and the link
  replaces it; when it holds any, both stay, the workspace keeps the data
  directory's canvas (today's behaviour), and the workspace is listed with a
  problem naming both paths and the command that picks one.
- **B. The folder always wins.** The twin moves to `canvases/moved/`. Nothing
  is lost, but the workspace silently shows other content than yesterday.
- **C. The data directory always wins** (today). The bug stays.

## Q7. Synced, slow and odd disks

- **A. Local file systems only; refuse network ones.** Before a move or a
  new folder canvas, `statfs` the folder: `smbfs`, `nfs`, `afpfs`, `webdav`
  and other network types keep the canvas in the data directory, with a
  problem line that says why. Local folders that a sync client watches
  (Syncthing, Dropbox, iCloud Drive on the local disk) are fine: the daemon's
  `fsync` is local, and Q4 catches the client replacing the file.
- **B. Allow any.** A stroke's `fsync` on a network share can take as long as
  the network; unmeasured (above), and the pen's outbox and every sling wait
  on it.
- **C. Write in the data directory, copy to the folder in the background.**
  Breaks "acknowledged means on disk where the user sees it", and doubles
  the writes; two copies to keep in step.

Known odd cases for A: iCloud Drive may evict a file to the cloud
("optimize storage"); reading it then waits for the download (the first
`canvas_show` is slow, not wrong). A sync client's conflict copy
(`main (conflicted copy).canvas`) is not a valid id, so the daemon ignores it;
the user resolves it in Finder.

## Q8. Ids and file names

- **A. The header's id is the canvas; the file must be named `<id>.canvas`.**
  A file renamed in Finder is not found, with a hint that names the expected
  file. Ids stay global, as today; a rename verb and workspace-scoped ids are
  later work (seed findings "lifecycle verbs" and "global namespace").
- **B. The file name is the id** (a Finder rename renames the canvas). Every
  reference (layouts in other workspaces, views, records, agents' notes)
  breaks silently.

## What stays in the data directory

Machine state, never synced and never in the folder:

- `record.sqlite` (the record, and every blob as a cache), `record.lock`,
  `socket`, `stopped`, `slingerd.log`, `sessions.json`, `packets/`.
- `workspaces/open.json`, and rootless workspaces (`workspaces/<slug>/`).
- **Per-pane views** (page, pan, zoom, focus; plan pl-85bfa0 steps 8-9):
  `workspaces/views.json`, keyed by the workspace's directory and the pane id.
  They are where this machine's window looks, so they never go in a canvas
  file or the folder: two machines on one synced folder would fight over
  them, and a view change would write the canvas. A canvas move keeps the
  id, so stored views still apply. A folder the user moves in Finder is a new
  workspace directory, so its views start fresh.
- The app's `unsent-ink.json` (keyed by canvas id; a move does not touch it).
- `canvases/`: links, the canvases that belong to no folder workspace (loose
  canvases, PDF canvases opened with no workspace, rootless workspaces'
  canvases, workspaces whose folder is on another machine or a network
  disk), and the `moved/` backups.

Which canvases belong to a folder: the workspace's `canvas` and the canvases
its layout's `canvas:` panes name, when the workspace's directory is on this
machine and no other open workspace names the same canvas. Loose canvases
going to `~/Documents/Slinger` (the seed's direction) is left for the owner.

## Compaction

Not in this plan (`canvas-file.md`, "Not in v1"). When it comes, it rewrites
a whole file (temp and rename), which in a synced folder sends the whole
file and is the moment a conflict copy is most likely. Q4's check sees the
rename as an outside change unless compaction goes through the same lock and
updates the identity it compares; the plan notes this as a risk.

## Recommended (picked overnight, owner to confirm)

Each pick is the one cheapest to reverse.

1. **Q1: A**, `<dir>/<id>.canvas` beside the toml. One file, the same name it
   has today; moving it back is one copy.
2. **Q2: A**, a link header (`"v":2` with `"at"`) in `canvases/<id>.canvas`.
   It makes an older build refuse rather than fork, and the index is one
   atomic file per canvas. The core owns the link (`canvas::file`), as it owns
   the header; the daemon's `Canvases::path` is the only place that follows
   one.
3. **Q3: A**, `<dir>/.slinger/blobs/sha256-<hex>`, written through on every
   change that names a blob in a folder canvas, read back into the record
   when missing. Lands before any move, so a moved canvas brings its PDFs.
4. **Q4: A**, `flock` on the canvas file and a `stat` identity check before
   each append, with a saved copy and `conflict` on an outside change.
   Lands first, while canvases are still in the data directory: it closes
   the lost-appends hole for every later step.
5. **Q5: A**, an explicit `canvas move` / `canvas move --back` first, with the
   four-step move above; automatic moves on start only in the last step,
   after the owner has moved one canvas and back. The backups in
   `canvases/moved/` are never removed by slinger.
6. **Q6: A**, adopt a folder canvas; a header-only twin goes to `moved/`, a
   twin with content stays and is reported, never chosen silently.
7. **Q7: A**, local file systems only.
8. **Q8: A**, the header's id wins; ids stay global for now.
9. Views stay in the data directory (`workspaces/views.json`), as the
   pl-85bfa0 plan already has them; nothing about views goes in the folder.

What an older build does with the new layout, all picks together: an older
slingerd lists a moved canvas, fails to open it with `canvas_too_new` and a
hint to upgrade, and never writes it or makes a twin; its other canvases
work. An older app needs nothing: it reaches canvases only through the
daemon, and `canvas_show.path` already carries the new path. The way back
is `slinger canvas move --back <ID>` (or the one `cp` above), and every
original stays in `canvases/moved/`.

The steps, their order and their acceptance are the plan for slinger-7c18bc.
