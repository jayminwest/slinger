# Slinger spec

Draft 2, 2026-11-10. Source: the board `idea.tldraw`, page "fable-5.2", rounds 1 to 4 and the owner's handwritten answers. The page "spec" in that board holds the same text for markup. This file replaces the earlier design notes.

Each decision has an id (D1, D2, ...). Each open item has an id (O1, O2, ...). Refer to them by id.

## 1. What Slinger is

- One window onto your computers. You think with a pen on a canvas. Agents do the work.
- It is a harness as much as it is a workspace. It is not a chatbot with tools, and not a wrapper over one agent CLI.
- The core move is the sling: point at anything in any pane, add a prompt, send it to an agent. The agent answers with things (marks on the canvas, files, a UI it builds), not with chat.
- It adds to the terminal, multiplexer and editor you use now. It does not replace them.
- It is built for one person. It is opinionated on purpose.

## 2. Principles

- P1. One thing at a time. Alignment and understanding before velocity.
- P2. Agents get the visual thinking (layout, arrows, emphasis), not only text.
- P3. One core library, thin front ends. Each command does one thing. New commands never change old ones.
- P4. One grammar for input. A tablet button is a key. Slinger sees key commands only.
- P5. Config is plain text, edited in place, reloaded live, the same on every machine. Machine-specific values go in an overlay file.
- P6. No chrome, but every state is visible: the active tool, colour, page and selection always show; a command confirms itself briefly. One focus border, one theme.
- P7. A write never waits on the network.
- P8. The spec stays small. A thing is not designed until it is needed.

## 3. Decisions

- D1. Stack: Rust on AppKit, the same as Flick.
- D2. Dictation: Slinger calls Flick's dictation. Slinger has no speech code.
- D3. Client and server are both custom. herdr is an inspiration only. There is no herdr adapter.
- D4. Client and server are separate from day one. They talk over a local socket. One server per machine.
- D5. The canvas is a native rebuild. It does not embed tldraw.
- D6. Canvas v1: pen at a constant width (no pressure), eraser, lasso, select, text, box, arrow, pages, undo, endless pan and zoom, and PDF pages. Nothing else. Amended 2026-11-12: a `pdf` item shows one page of a PDF, or a part of one (a snippet); the PDF is a blob in the record. Opening a PDF makes a canvas of its pages stacked on one canvas page, read in reader mode (fit to width, vertical scroll). Only the app reads a PDF; the core and the server never parse one. See `docs/design/pdf.md`.
- D7. Canvas files use Slinger's own format. Slinger can import a tldraw file. It cannot export one.
- D8. An agent gets a packet, not a screenshot (section 6).
- D9. Handwriting recognition runs only when the user asks. It is never a layer between the canvas and the agent.
- D10. First harness: Claude Code, as a session that keeps running. A sling goes into a session that already exists and remembers the earlier slings. Slinger never starts a fresh agent for one sling. Second harness: the Slinger harness on Daffy.
- D11. Agents act through a small command set (section 7). The set is built to grow.
- D12. Pane types in v1: canvas and agent UI. Browser and terminal come later.
- D13. An agent UI can be a pane, and it can be an item on a canvas. The canvas can hold live items that run code (section 8).
- D14. The record is an append-only event log plus blobs (section 9). It is not where files live.
- D15. Each machine writes its own record. One always-on machine receives a copy of every record, for search and backup.
- D16. A workspace has directories. It is not a directory (section 10).
- D17. The sidebar is optional and it collapses. Agent status is a glance, not a dashboard.
- D18. Keyboard input always works. Terminal and code entry are not design goals.
- D19. Code that an agent writes for a UI lives in the record as a blob. It is not a file in a directory. A pane or a canvas item refers to it by hash.
- D20. Slinger does not make an agent durable. The harness does: cc-durable for Claude Code today, Daffy later. Slinger builds no durable-agent machinery.
- D21. Three surfaces show the commands: a key overlay, a command palette and the macOS menu bar. All three read one command registry: each command's title, group and live key. A command added to the registry shows in all three.

## 4. Not in v1

- Browser pane, terminal pane, text view of a canvas.
- The Slinger harness on Daffy.
- Sending the record to the always-on machine. Events carry the machine name from day one, so this can be added with no redesign.
- More than one machine in one session.
- Control of the tablet display or its drivers.
- Canvas features outside D6: snapping, frames, images (a PDF page is not an image: D6), rotation, export.
- A PDF pane, PDF export, and a PDF made from the CLI alone (the CLI cannot read a PDF's page sizes).
- A data model beyond events and blobs.

## 5. System

- Client: the window. Panes, canvas, input. It holds no durable state.
- Server: one per machine. It owns workspaces, canvas files, the record, and the connections to harnesses.
- Harness: runs agents. It connects to the server through the CLI. It reports status through hooks.
- Agent runtime: the expected case is a remote machine. The local machine also works.
- Session: one running agent that Slinger can reach. The server keeps a list: agent name, home directory, how to deliver a message to it, and its status. A plain Claude Code session joins the list when it starts to listen, as the "send to agent" listener does today. A cc-durable agent is reached through its inbox, which delivers once and survives a restart.
- CLI: `slinger <noun> <verb>`. Plain text by default, `--json` for machines. The client, agents and the user all use it. It is the only way in.

## 6. The sling and the packet

The six steps: select, prompt, packet, agent, reply, record.

Packet rules:
- R1. Send only what changed after the last sling in that canvas. The server knows which items are new.
- R2. Items an agent made go as text with their id and box. They never go as pixels.
- R3. New ink is put into clusters. Each cluster is rendered at a fixed scale that keeps handwriting readable. The scale does not depend on the viewport.
- R4. The index has one line per item: id, box, kind, and what it points at (for example: cluster 4 points at item 12).
- R5. The rest of the page is an outline of one line per item. The agent asks for a crop when it needs one.
- R6. Raw stroke points are not in the packet. One command returns them for the rare case.
- R7. Every packet is stored as a blob in the record.
- R8. Ink over a PDF page goes with the page under it: its image shows the page beneath the ink, and its line carries the PDF's words under the ink. A PDF page never goes as an image on its own: its line names the blob and the page. Only the app can render a PDF, so a sling without the app carries the ink on white.

## 7. Agent commands

First set. Each is one CLI verb. Each follows the CLI contract (section 15).

- canvas read: the packet for a canvas, or the changes after a given point.
- canvas crop: an image of one item or one box.
- canvas put: add text, a box, ink or an arrow, placed beside a given item id or where the agent says, in a colour it may choose.
- canvas edit, canvas remove: only for items the agent made. An agent cannot change the user's ink.
- pane open, pane close.
- ui open: show an agent UI as a pane or as a canvas item (section 8).
- status set: working, blocked, done.

Rules:
- Everything an agent makes carries the agent's name and the time.
- A new ability is a new verb. Existing verbs do not change meaning.

## 8. Agent UI and live canvas items

- An agent UI is a small interface that an agent builds for one task.
- It can be shown in its own pane, or as an item on a canvas that moves and scales with the canvas.
- The first live canvas item is the "send to agent" button. It is built in, not a script copied into each canvas.
- Every workspace's canvas gets one: when the daemon makes the canvas for a workspace, the core puts the user's button on its first page, at the origin (slinger-0d9e90). It is an ordinary item, not an undo step: the user moves or removes it, and nothing puts it back. Other pages, canvases that exist already, a PDF's canvas and a bare `canvas create` get none; `place-button` adds one anywhere. The `sling` entry opens beside a page's first button.
- An agent UI gets no network. It talks to Slinger through a bridge with two calls: read the context it was given, and reply with JSON.
- The code is stored in the record as a blob (D19). Each UI has a name, and each change is a new blob, so every version stays. `ui export` writes one out as files when you want to read or keep it.
- See O1 for what the code is, and `docs/design/live-items.md` for the bridge.

## 9. The record

- One table of events: number, time, kind, version, source, payload. One table of blobs, keyed by hash. Views are rebuilt from the log.
- Tags on every event: machine, workspace, agent, canvas.
- Events in v1: sling sent, reply landed, file touched (path and hash, never contents), pane moved, agent blocked, agent done.
- The thinking record is a view over the log. It is not a second store. See O3.
- When the harness is Daffy, the agent keeps its own log. The record points at it (agent name and event number) and copies nothing.

## 10. Workspaces

- workspace = name + pane layout + roots (zero or more directories, on any machine) + agent (optional) + its tag in the record.
- A workspace with a durable agent has exactly one primary root. That directory is the agent's home.
- One directory is the primary root of at most one workspace.
- Every workspace has one canvas by default. It is where you talk to the workspace's agent.
- "Open a directory as a workspace" is the normal way to make one. The definition is a small file in the root, so it syncs with the directory.
- A workspace with no root lives in Slinger's config.
- The agent belongs to the directory. Closing the workspace does not stop the agent.
- The teacher setup is a reference for this. It is not the full definition.

## 11. Input

- Pen: draw, select, resize panes, move things between panes.
- Voice: Flick dictation, for prose.
- Keys: every action is a key command in the config file. The tablet's buttons are bound to keys or macros in the tablet's own software. Slinger has no tablet-button code.
- Pop-ups at the pen (colour, shape, tool) open from a key command. They stay: the discovery surfaces (D21) do not replace them.

## 12. Risks

- K1. The canvas rebuild is the largest piece of work. D6 is the limit.
- K2. Pen feel cannot be judged by an agent. Spike A needs the owner's hand.
- K3. tldraw is source-available, not open source. Rebuild the behaviour. Do not copy its code or assets. Reading its file format for the importer is fine.
- K4. An agent UI is untrusted code. No network, no file access, only the bridge.
- K5. Two editors on one canvas file overwrite each other. In v1 the server on one machine is the only writer.
- K6. Overlap with Flick. Flick is the OS layer (windows, launcher, dictation). Slinger is one window's contents. Share code, do not copy it.

## 13. Open items

- O1. What code does a live canvas item run? Decided 2026-11-10 (the owner chose "agent picks"): v1 runs one self-contained HTML file in a sandboxed web view, placed and scaled by the canvas, with the two-call bridge. Both alternatives, the bridge, the blob and the reasons are in `docs/design/live-items.md`. Later: restrict it to Flick's model, a closed set of blocks (text, key and value, list, progress, choice, field) and a closed set of actions, where anything unknown degrades to text, as a second blob kind. Open: when to restrict, and which blocks.
- O2. Pen feel has no number yet. Lean: the test is "not worse than tldraw on the same tablet", judged by the owner, with the pen-to-screen delay logged for reference.
- O3. What exactly the thinking record shows. Decide after the log has real events in it.
- O4. The key commands themselves. Decide when there is a canvas to use them on. Every command has a default key in `config.example.toml`. `show-keys` (the key overlay) and `show-commands` (the command palette) are added (D21). The final keys stay open.
- O5. Closed 2026-11-10: an append-only operation log per canvas. See `docs/design/canvas-file.md`.
- O6. The workspace definition file: its name and fields. Decided 2026-11-10: `slinger-workspace.toml` in the primary root, versioned TOML that keeps unknown fields. The fields and rules are in `docs/workspace.md`.
- O7. The scaffold has no GUI layer and no service layer yet. The brief for that work is `BRIEF-gui-service-layers.md` in the scaffold-project repository. It must be done before Slinger is scaffolded. The spikes do not wait for it.
- O8. A UI's code is in one machine's record (D19), and so is a PDF (D6). A canvas opened on a second machine needs that blob. This waits for the copy to the always-on machine (D15).

## 14. Plan

Step 1. This spec. Done 2026-11-10: the owner read all of it. A line with no mark is agreed.

Step 2. Three spikes. Each is small and each has a pass or fail.

- Spike A, ink. A native window on the tablet display with pen strokes at a constant width. The owner draws a fixed test sheet (fast scribble, small slow handwriting, a resting palm) here and in tldraw, and says better, the same, or worse. Pass: the same or better.
  Result 2026-11-10: pass. The owner drew with the pen on the tablet display and said it felt fine. The window is plain AppKit: one view, smoothed paths at a constant width, mouse coalescing off, repaint of the changed box only. No delay number was recorded.
- Spike B, packet. Build clusters, crops and the index from the board "idea" itself. Pass: an agent reads every prompt on the page correctly, and the packet costs less than half the tokens of the screenshots used in rounds 1 to 4.
  Result 2026-11-10: pass on tokens, near pass on reading. The whole board (5 pages, 7562 strokes, 204 items) is 220 crops at scale 0.35 plus a text index: about 57 000 tokens (38 000 image, 19 000 text). The 57 screenshots of rounds 1 to 4 cost about 129 000. Ratio 44%. A fresh agent read 36 crops from three pages: one word unreadable, two words cut where two notes of different colours sat close together. Findings: (1) a long stroke (frame, arrow, divider) must not join clusters; (2) distance alone is not enough to cluster, so use colour and time too; (3) a crop over the image size limit needs tiles that overlap. Note: the committed fixture (`crates/core/tests/fixtures/idea.tldraw`; see `crates/core/tests/tldraw.rs`) has 7563 strokes, one more than the spike counted: the board was saved again after the spike read it. The fixture is the reference.
- Spike C, one sling. Select, prompt, a Claude Code session that is already running answers, the reply lands on the canvas beside the selection, and the record has the events. Pass: three slings in a row go to the same session with no manual step, and the third answer uses something from the first.
  Result 2026-11-10: pass. A small server held one Claude Code process open (stream input, stream output) and fed it three slings from the page "spec". All three went to the same session id with no manual step. Each reply landed as a text item to the right of the selection, through one `put` command. Sling 3 selected the box of sling 1 again: 0 new strokes were sent (R1), and the answer gave the code word and the note text from sling 1. The record has 9 events (sling sent, reply landed, agent done, three times) and 3 packet blobs. Time from sling to reply: 3 s, 22 s, 2 s. Findings: (1) the session loads the owner's global agent config and can read files, so in sling 2 it went to read the spec (7 turns) before it answered; the real server must set what a session may read. (2) The first sling pays for session start (about 19 000 tokens written to cache); later slings read the cache.
  Result 2026-11-10, the real build (`scripts/e2e-sling.sh`, slinger-e1b0): pass, twice in a row, with Claude Code 2.1.322 on `--model haiku` (claude-haiku-5-5). slingerd on a scratch socket and data directory held the session with its default limits; the canvas had three text notes and one ink stroke under note 1; sling i selected note i. All three slings went to one session id. Each reply landed as one text item 24 units right of its note, through one `canvas put` per sling; the agent never ran `canvas read` and needed no tool beyond `Bash(slinger *)`. The record has sling sent, reply landed and agent done 3 times each and 3 packet blobs. The third answer gave the code word from sling 1. Sling 1 carried its image as a base64 PNG block, and Claude Code took it. Time from sling to reply (to agent done), run 1: 3.3 s (4.1), 0.8 s (1.3), 0.8 s (1.4); run 2: 2.2 s (2.8), 0.9 s (1.4), 0.9 s (1.4). Tokens per sling, run 1 (2 turns each; input, cache write, cache read, output): 4, 4274, 12925, 523; 4, 469, 17979, 165; 4, 410, 18877, 135. Compared with the spike, the limits keep the session from wandering: no sling took more than one tool call, and the first sling writes about 4 300 tokens to cache instead of 19 000.

Stop. We are here. Review the spikes and this spec before any full build is delegated.

Step 3. Not planned yet.

## 15. Build standard

The project is made with scaffold-project (Rust, with the CLI front end). Its philosophy file is the rule for every design choice. Daffy is the reference for how it looks in practice. What that means here:

- B1. Layers, as in Daffy: a core library (canvas model, packet, record, workspaces; no window, terminal, process or network code), the server (the daemon; it owns the IO the core leaves out), the CLI, and the app. Dependencies point inward. The app, the CLI and the server hold no domain logic.
- B2. Deep modules. The canvas, the packet and the record are each one module with a small interface. Each owns one decision (a file format, a layout) that no other module knows.
- B3. The CLI follows the scaffold's contract: one JSON document with `--json`, a stable error code and a hint on failure, typed exit codes, nothing extra on stdout.
- B4. Define errors out of existence first. Removing an item that is already gone succeeds. A packet with nothing new is a valid packet.
- B5. Data formats are API. The packet, the record's events and the canvas file each carry a version. A change never breaks a reader.
- B6. Design it twice. The packet format, the canvas file format (O5), the live-item code (O1) and how the app supplies the page under the ink of a sling (`docs/design/pdf.md`) each get two real alternatives in writing before one is picked.
- B7. Seams are paid for. There is no harness trait until the second harness arrives. Claude Code uses the CLI like any outside program.
- B8. One verify command, the same in the pre-commit hook and in CI. Budgets only tighten. Tests go through public interfaces with real inputs: real boards are the fixtures for the packet and the importer.
- B9. Spikes are throwaway code and stay outside the repository. What a spike proves goes into this spec with its evidence (command output, numbers). Spike code is not promoted.
- B10. The architecture file changes in the same commit as any contract.
