# Live items: what code an agent UI runs

Design for O1 (SPEC.md section 8, K4, B6, D19). Two real alternatives, then
the pick for v1. Agent UIs in panes and live items on a canvas run the same
code through the same bridge, so this page covers both.

## The decision

**v1 runs HTML in a sandboxed web view (alternative A).**

1. Agents already write HTML, CSS and JavaScript well. A has nothing to teach but two calls; B needs a schema in every prompt.
2. A UI built for one task needs what the task needs (a table, a chart, a drag to reorder). B caps that at six blocks.
3. The sandbox is WebKit's own process sandbox plus four checks we can test from outside (below). B has no code, so it is safer, but A's risk is bounded.
4. A costs one native view and a bridge. B costs a normalizer and six native widgets at every zoom, in the module (`platform`) that is hardest to test.
5. B stays the path to "restrict it later" (O1): a second blob kind, served by the same names, versions and bridge.

## What both share

### The blob, its name and its versions

- A UI's code is **one file**, not a bundle. In v1 it is one self-contained
  HTML document: CSS and scripts inline, images and fonts as `data:` URLs.
  One file means one hash per version, no manifest, no path lookup, and an
  export that is one file. A bundle buys nothing that inlining does not.
- Limits: at most **1048576 bytes** (1 MiB) of UTF-8, with no control
  characters but tab, LF and CR. Anything else is `invalid`, with a hint.
  The first draft capped it at 32000 bytes so the code fit one control
  request line. The code now travels as a file on the same machine, like
  every blob (docs/service.md, "How a blob's bytes travel"), so the line
  limit no longer applies. The cap stays to bound the record and to stop a
  file put by mistake, and 1 MiB leaves room for images inlined as `data:`
  URLs.
- The blob is stored in the record's blob table, keyed by its blob id,
  `sha256:` and 64 lowercase hex characters (D19, section 9).
- **Name:** 1 to 64 characters of `a-z 0-9 -`, starting with a letter, such
  as `pick-slot`. Names are global to the machine's record.
- **Version:** `ui put <name>` appends a `ui_put` event with payload
  `{"name","version","hash","bytes"}`, where `hash` is the blob id. Only the
  `ui put` verb writes this kind (`record append` refuses it), and it picks
  the version inside the record's write, so two puts at once cannot take
  one number. The version is 1 for the first put of
  a name and one more than the last for each later put. Every version stays.
- Putting the same bytes as the name's latest version adds no event and
  returns that version (`"new": false`). Putting the bytes of an older
  version makes a new version: a revert is a change like any other.
- **A reference** to a UI is one of: `pick-slot` (the latest version),
  `pick-slot@3` (that version), or the blob id (the latest version holding
  those bytes). `ui open` resolves
  the reference once. The pane or canvas item it makes stores the **hash**
  (D19), so it keeps showing exactly what was opened. To show a new version,
  the agent opens it again.
- `ui export <ref> [--out DIR]` writes `<name>.v<version>.html` (the bytes,
  unchanged) into `DIR`, the current directory by default.

### The bridge

A UI talks to Slinger through two calls and nothing else. The bridge has
its own version, `1`, separate from the control protocol's `v`.

```js
slinger.v               // 1
slinger.context()       // Promise<Context>: what the UI was given
slinger.reply(value)    // Promise<void>: send one JSON value to the agent
```

**context()** resolves to the same object on every call:

```json
{
  "v": 1,
  "ui": { "name": "pick-slot", "version": 3, "hash": "sha256:9f2c…" },
  "host": "canvas",
  "open": "pick-slot#12",
  "data": { "slots": ["Tue 10:00", "Wed 14:00"] }
}
```

- `host` is `pane` or `canvas`. `open` names this showing of the UI: two
  showings of one version have different `open` ids. It is
  `<name>#<number>`, the number of the `ui_opened` event `ui open` appended
  (no reference holds a `#`). It is `null` for a pane opened with
  `pane open --type ui`, which no agent waits on.
- `data` is the JSON the agent passed to `ui open --data`, or `null`. It is
  at most 16384 bytes, checked by `ui open`.

**reply(value)** sends `value` to the agent's session:

- `value` is any value `JSON.stringify` accepts. At most 16384 bytes once
  serialized. A UI may reply more than once; replies arrive in order. At most
  100 replies per showing.
- The promise resolves once the daemon has recorded the reply (a
  `ui_reply` event). It does not wait for the agent.
- It rejects with an `Error` whose `code` is stable:

| `code` | When |
|---|---|
| `invalid` | `JSON.stringify(value)` throws or returns `undefined` |
| `too_large` | The serialized value is over 16384 bytes |
| `limit` | This showing already sent 100 replies |
| `unavailable` | The daemon did not answer |

The agent's session receives one JSON line per reply, as a user message:
the daemon records the reply, then hands the line to the session of the
agent that opened the showing. A session that is not running does not get
it; the reply stays in the record and the pane says the agent is not
running.

```json
{"v":1,"kind":"ui_reply","ui":"pick-slot","version":3,"hash":"sha256:9f2c…","open":"pick-slot#12","value":{"slot":"Wed 14:00"}}
```

**Versioning.** Within bridge v1 changes only add: a new context field, a
new call. A UI ignores context fields it does not know. A UI may declare the
version it was written for with `<meta name="slinger-bridge" content="1">`;
without it, it is 1. The host shows a version it does not speak as a text
line, `bridge v<n> is not supported`, and runs none of its code.

## Alternative A: HTML in a sandboxed web view

### Example UI, in full

```html
<!doctype html>
<meta name="slinger-bridge" content="1">
<style>
  body { font: 13px -apple-system, sans-serif; margin: 8px; }
  button { margin: 2px; }
  button.on { font-weight: bold; }
</style>
<p id="q">Loading…</p>
<div id="slots"></div>
<textarea id="note" placeholder="Note" rows="2"></textarea>
<button id="send" disabled>Book</button>
<script>
  let pick = null;
  slinger.context().then(({ data }) => {
    document.getElementById("q").textContent = "Pick a slot";
    for (const s of data.slots) {
      const b = document.createElement("button");
      b.textContent = s;
      b.onclick = () => {
        pick = s;
        document.querySelectorAll("#slots button").forEach(x => x.classList.toggle("on", x === b));
        document.getElementById("send").disabled = false;
      };
      document.getElementById("slots").append(b);
    }
  });
  document.getElementById("send").onclick = () =>
    slinger.reply({ slot: pick, note: document.getElementById("note").value })
      .then(() => { document.body.textContent = "Sent."; })
      .catch(e => { document.getElementById("q").textContent = e.code + ": " + e.message; });
</script>
```

### How the sandbox holds (K4)

The web content runs in WebKit's own sandboxed process. The host adds four
checks:

1. **Only one origin.** The page is loaded from `slinger-ui://<hex>/`,
   where `<hex>` is the 64 hex characters of its blob id (`sha256:` cannot
   be part of a URL host), through a `WKURLSchemeHandler`. It answers that one URL with the blob and
   every other URL with 404. The app never calls `loadFileURL` or
   `loadHTMLString`, so there is no file or `about:` origin.
2. **A content security policy on the response**, not in the page, so the
   page cannot leave it out:
   `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; media-src data:; connect-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'`.
   This stops `fetch`, XHR, WebSocket, EventSource, beacons, frames, forms
   and remote images.
3. **A content rule list** that blocks every load but the scheme:
   `[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^slinger-ui:"},"action":{"type":"ignore-previous-rules"}}]`.
4. **No way out of the view.** The navigation delegate cancels every
   navigation that is not the first load. The UI delegate refuses new windows
   and `javaScriptCanOpenWindowsAutomatically` is false. The data store is
   non-persistent, so nothing survives a close.

The bridge is the only script message handler. The shim that defines
`window.slinger` is a user script injected at document start in the main
frame only, and its object is frozen. Over the wire, the shim posts the
JSON text of `{"v":1,"call":"context"}` or
`{"v":1,"call":"reply","value":"<JSON text>"}` to a
`WKScriptMessageHandlerWithReply` named `slinger`. It sends the value as
JSON text, so WebKit's object conversion cannot change a number into a
boolean. The handler answers with JSON text, or an error string
`<code>: <message>` that the shim turns into the `Error`. Any other message
gets `invalid`. A pane of a UI opened by name, not by `ui open`, has no
agent, so its `reply` rejects with `unavailable`; `slinger-app --preview`
writes each reply's `ui_reply` line to its stdout instead.

**The test** (acceptance 2 of the plan, `crates/app/tests/sandbox.rs`):
it runs the real app, `slinger-app --preview escape --data '{"port":N}'
--background`, against a daemon of its own. `--background` never shows the
window and keeps the app out of the Dock, so a test run takes no focus;
`WebKit` loads and runs the UI all the same. The fixture UI (`tests/fixtures/escape.html`)
shows its context, then tries each way out against a TCP listener the test
owns: `fetch`, XHR, WebSocket, EventSource, a beacon, an `<img>`, an
iframe, a form post, `location = "http://…"` and `window.open`; and against
`/etc/hosts`: `fetch`, XHR and an iframe of `file:///etc/hosts`. It replies
with what each attempt saw. The test passes when the reply came, no attempt
got out, `eval` threw (the policy on the response is in force), the page
saw policy violations, and the listener saw no connection.

**Each layer, measured** (macOS 26, by switching layers off in a local
build of that test; not in the committed code):

| Layers on | Connections | What got out |
|---|---|---|
| All four | 0 | nothing |
| Rule list only (no policy, navigations allowed) | 0 | nothing; WebSocket fails |
| Policy only (no rule list, navigations allowed) | 4 | `location = "http://…"` (top-level navigations are not the policy's) |
| Policy and navigation delegate (no rule list) | 0 | nothing |
| Navigation delegate only | 22 | fetch, XHR, WebSocket, EventSource, image, beacon |

So WebKit applies a policy sent on a custom-scheme response (`eval`
throws, and the page gets `securitypolicyviolation` events), and the
content rule list covers WebSocket loads: both were open questions. The
rule list alone stopped every attempt. The policy alone does not stop a
top-level navigation; the navigation delegate does. No layer is redundant
for every attempt, so all four stay.

### How it scales on a zoomed canvas

- The item's box is in canvas units, and one CSS pixel is one canvas unit.
  The view's frame is the box times the zoom, and its `pageZoom` is the zoom.
  The page lays out at the same CSS width at every zoom and WebKit draws it
  sharp.
- During a pinch the host scales the view's layer and sets `pageZoom` once
  the gesture ends, so a pinch never waits on a relayout.
- An item off screen, or narrower than 48 points on screen, shows the last
  snapshot (`takeSnapshot`) and gets no input. The web view behind it is
  closed once it has been off screen for 30 s and opened again on return,
  which loses the page's in-memory state. A UI that must keep state across
  that keeps it in the agent, through `reply`.
- A pane is the same with a zoom of 1.

As built (slinger-085117): the 48-point and 30 s rules are in
`scene/live.rs` and `scene/live/away.rs`. Not built yet: a still item
shows its live page, not a snapshot, without input; an off-screen item's
view is hidden, not replaced by a snapshot; and a pinch sets `pageZoom` at
each step instead of scaling the layer.

### Cost to build

- The core: names, versions, hashes and the two events. Small; shared with B.
- The app: one view built with `objc2-web-kit`, two small Objective-C classes
  (the scheme handler and the message handler) and the shim. About 400 to
  600 lines in `platform`, plus pure placement in `layout`.
- Run time: each live web view is a WebKit content process, tens of
  megabytes. The snapshot rule above keeps the count to what is on screen.
- The binary links WebKit dynamically, so it grows by the bindings only.

### What an agent must know

That it writes one HTML file of at most 1 MiB, with everything
inline and no network. The two calls above. That the page is drawn at the
box it was opened with (`ui open` gives the size), in CSS pixels. About
twenty lines of instructions.

## Alternative B: a closed set of blocks and actions

The model is Flick's cards (`docs/cards.md` in Flick). The code is not
copied (K6).

### Example UI, in full

```json
{
  "v": 1,
  "title": "Pick a slot",
  "blocks": [
    { "type": "choice", "id": "slot", "label": "Slot",
      "options": [{ "id": "tue-10", "label": "Tue 10:00" }, { "id": "wed-14", "label": "Wed 14:00" }] },
    { "type": "field", "id": "note", "label": "Note", "multiline": true }
  ],
  "actions": [
    { "id": "book", "label": "Book", "style": "primary", "do": "reply" },
    { "id": "close", "label": "Not now", "do": "dismiss" }
  ]
}
```

- Blocks: `text` (markdown-lite), `kv`, `list`, `progress`, `choice`,
  `field`, with Flick's keys and caps.
- Actions: `reply` (send the input values with the action id), `copy`
  (literal text to the clipboard) and `dismiss`. Nothing else: no URL, app,
  script or shell actions.
- Anything unknown degrades to text: an unknown block type becomes a
  `text` block `[<type>] <preview>`, an unknown action stays on screen,
  disabled. Only a document that is not a JSON object, or has no `title`, is
  refused.

### How the sandbox holds (K4)

There is no code. The blob is data, parsed by the core into a closed set
of types and drawn by native widgets. Nothing in it can reach the network or
a file, because there is no verb for it. This is the strongest sandbox.

### How it scales on a zoomed canvas

`layout` places each block in canvas units and `platform` draws text at the
font size times the zoom, so it is sharp at every zoom with no extra
process. Below a size threshold the item draws its title only.

### Cost to build

- The core: a parser with degrade rules and warnings. Flick's is about 1000
  lines plus 360 lines of tests.
- The app: six native widgets with focus, Tab order and keyboard, laid out
  at every zoom. About 1500 to 2500 lines, most of it in `platform`, where
  only a run of the app tests it.

### What an agent must know

The schema: six block types, their keys and caps, three actions and the
degrade rules. Flick's page for this is about 150 lines, and every UI is
limited to what those blocks can show.

### The bridge in B

There is no script, so the two calls are made by the host. Read the context:
`ui open --data` fills the inputs' initial `value` and `selected` by id.
Reply: a `reply` action sends `{"action":"book","values":{"slot":"wed-14","note":"…"}}`
as the `value` of the same `ui_reply` line.
