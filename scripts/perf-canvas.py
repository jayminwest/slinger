#!/usr/bin/env python3
"""Measure slinger on a synthetic 5 MB+ canvas (slinger-6c0f10).

    scripts/perf-canvas.py [--scale N] [--runs N] [--no-app] [--keep] [--dir DIR]

Builds release slingerd, slinger, slinger-app and the synthetic_canvas example,
writes a synthetic canvas (`big`, the default 120 sessions times --scale) and a
small one (`small`, 6 sessions) into a scratch data directory, starts a scratch
slingerd on its own socket (--scratch, with the daemon tests' stand-in for
Claude Code, so a sling spends no tokens), and times the control verbs over
the socket: canvas_show (cold and warm), canvas_read, sling_backdrops,
sling_send, canvas_apply. Then, unless --no-app, it starts a driven window
(`slinger-app --drive`, never on a screen) and times a workspace switch and
pen samples through the drive socket. It prints one Markdown table; the
method and the results are in docs/perf/canvas.md.

Nothing touches the user's daemon, data directory or config. Everything it
starts is stopped by pid on exit. Needs python3 (stdlib only) and cargo.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STANDIN = os.path.join(ROOT, "crates/daemon/src/tests/claude")
# A one-page PDF, written here so the canvas's PDF items have a blob.
PDF = b"""%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj
trailer<</Root 1 0 R>>
%%EOF
"""


def ask(path, request, timeout=600):
    """One request on a socket: (milliseconds, reply)."""
    start = time.perf_counter()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
        s.settimeout(timeout)
        s.connect(path)
        s.sendall((json.dumps(request) + "\n").encode())
        data = b""
        while not data.endswith(b"\n"):
            chunk = s.recv(1 << 20)
            if not chunk:
                break
            data += chunk
    ms = (time.perf_counter() - start) * 1000
    return ms, json.loads(data)


def daemon(sock, request):
    ms, reply = ask(sock, dict(v=1, **request))
    if not reply.get("ok", False):
        sys.exit(f"✗ {request['op']}: {reply}")
    return ms, reply


def summary(samples):
    samples = sorted(samples)
    p90 = samples[min(len(samples) - 1, int(len(samples) * 0.9))]
    return f"{statistics.median(samples):.1f} | {p90:.1f} | {len(samples)}"


def load():
    one, five, _ = os.getloadavg()
    builds = subprocess.run(["pgrep", "-c", "rustc"], capture_output=True, text=True).stdout.strip()
    return f"load {one:.1f} (1 min) {five:.1f} (5 min), {os.cpu_count()} cores, rustc processes: {builds or 0}"


def wait_for(path, seconds=30):
    end = time.time() + seconds
    while not os.path.exists(path):
        if time.time() > end:
            sys.exit(f"✗ {path} did not appear")
        time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--scale", type=int, default=1)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--no-app", action="store_true")
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--dir", help="where to make the scratch directory (default: $TMPDIR)")
    args = parser.parse_args()

    subprocess.run(
        ["cargo", "build", "--quiet", "--release", "--bin", "slingerd", "--bin", "slinger",
         "--bin", "slinger-app", "--example", "synthetic_canvas"],
        cwd=ROOT, check=True)
    target = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
    bins = os.path.join(target, "release")

    # Short relative socket names: a socket path over 104 bytes fails.
    tmp = tempfile.mkdtemp(prefix="slperf.", dir=args.dir)
    os.chdir(tmp)
    started = []
    rows = []

    def row(name, samples, note=""):
        rows.append(f"| {name} | {summary(samples)} | {note} |")

    try:
        os.makedirs("data/canvases")
        with open("paper.pdf", "wb") as f:
            f.write(PDF)
        pdf = "sha256:" + hashlib.sha256(PDF).hexdigest()
        gen = os.path.join(bins, "examples/synthetic_canvas")
        t = time.perf_counter()
        subprocess.run([gen, "data/canvases/big.canvas", "--id", "big", "--scale", str(args.scale),
                        "--pdf", pdf], check=True)
        gen_ms = (time.perf_counter() - t) * 1000
        subprocess.run([gen, "data/canvases/small.canvas", "--id", "small", "--sessions", "6",
                        "--seed", "2", "--pdf", pdf], check=True)
        big_bytes = os.path.getsize("data/canvases/big.canvas")
        small_bytes = os.path.getsize("data/canvases/small.canvas")
        for name in ("big", "small"):
            os.makedirs(f"ws-{name}")
            with open(f"ws-{name}/slinger-workspace.toml", "w") as f:
                f.write(f'version = 1\nname = "{name}"\ncanvas = "{name}"\n[agent]\nname = "bench-{name}"\n')

        log = open("slingerd.log", "w")
        proc = subprocess.Popen(
            [os.path.join(bins, "slingerd"), "--socket", "d.sock", "--data-dir", "data", "--scratch",
             "--claude", STANDIN, "--slinger", os.path.join(bins, "slinger")],
            stdout=log, stderr=log)
        started.append(proc)
        wait_for("d.sock")
        time.sleep(0.2)
        sock = "d.sock"
        daemon(sock, {"op": "blob_put", "path": os.path.abspath("paper.pdf")})
        for name in ("big", "small"):
            daemon(sock, {"op": "workspace_open", "dir": os.path.abspath(f"ws-{name}")})

        before = load()
        for name, size in (("big", big_bytes), ("small", small_bytes)):
            ms, reply = daemon(sock, {"op": "canvas_show", "canvas": name})
            rows.append(f"| canvas_show {name}, first (opens the file: {size / 1e6:.2f} MB) | {ms:.1f} | | 1 | {len(reply['items'])} items on page 1 |")
        for name in ("big", "small"):
            for page in ("p1", "p4"):
                samples = [daemon(sock, {"op": "canvas_show", "canvas": name, "page": page})[0]
                           for _ in range(args.runs)]
                row(f"canvas_show {name} {page}, warm", samples)
        for name in ("big", "small"):
            samples = [daemon(sock, {"op": "sling_backdrops", "canvas": name, "page": "p4"})[0]
                       for _ in range(args.runs)]
            row(f"sling_backdrops {name} p4 (ink on PDF pages)", samples)

        # A stroke the user writes, as the app sends it: one canvas_apply.
        def stroke(name, x):
            item = {"page": "p1", "kind": "stroke", "color": "black", "width": 20,
                    "pts": [x, 5000] + [12, 3] * 30}
            return daemon(sock, {"op": "canvas_apply", "canvas": name, "ops": {"op": "add", "item": item}})
        for name in ("big", "small"):
            made = []
            samples = []
            for i in range(args.runs):
                ms, reply = stroke(name, 1000 + i * 400)
                samples.append(ms)
                made += reply["made"]
            row(f"canvas_apply {name}: add one 31-point stroke", samples)
            samples = [daemon(sock, {"op": "canvas_apply", "canvas": name,
                                     "ops": {"op": "move", "ids": made, "dx": 10, "dy": 0}})[0]
                       for _ in range(args.runs)]
            row(f"canvas_apply {name}: lasso move of {len(made)} strokes", samples)

        for name in ("big", "small"):
            # The first read covers the whole canvas: no sling yet.
            ms, reply = daemon(sock, {"op": "canvas_read", "canvas": name})
            rows.append(f"| canvas_read {name} p1, everything new (no sling yet) | {ms:.1f} | | 1 | {len(reply['images'])} images |")
            nxt = daemon(sock, {"op": "canvas_show", "canvas": name})[1]["next"]
            samples = [daemon(sock, {"op": "canvas_read", "canvas": name, "after": nxt - 3})[0]
                       for _ in range(args.runs)]
            row(f"canvas_read {name} p1, last 2 lines new", samples)
        for name in ("big", "small"):
            ms, reply = daemon(sock, {"op": "sling_send", "canvas": name, "prompt": "bench"})
            rows.append(f"| sling_send {name} p1, first (everything new; starts the stand-in agent) | {ms:.1f} | | 1 | {reply.get('strokes')} strokes |")
            samples = []
            for i in range(max(3, args.runs // 4)):
                stroke(name, 30000 + i * 400)
                samples.append(daemon(sock, {"op": "sling_send", "canvas": name, "prompt": "bench"})[0])
            row(f"sling_send {name} p1, one new stroke", samples, "daemon side only: stand-in agent")
        after = load()

        app_rows = [] if args.no_app else drive(bins, sock, args.runs)
    finally:
        for proc in reversed(started):
            proc.terminate()
            try:
                proc.wait(10)
            except subprocess.TimeoutExpired:
                proc.kill()
        if args.keep:
            print(f"kept: {tmp}", file=sys.stderr)
        else:
            shutil.rmtree(tmp, ignore_errors=True)

    print(f"big.canvas {big_bytes} bytes (scale {args.scale}, written in {gen_ms:.0f} ms); small.canvas {small_bytes} bytes")
    print(f"before: {before}")
    print(f"after daemon runs: {after}")
    print()
    print("| what | median ms | p90 ms | runs | note |")
    print("|---|---|---|---|---|")
    for line in rows + app_rows:
        print(line)


def drive(bins, sock, runs):
    """Times a driven window: a workspace switch and pen samples."""
    with open("config.toml", "w") as f:
        f.write(f'[daemon]\nsocket = "{sock}"\n[workspace]\nname = "small"\n')
    env = dict(os.environ, SLINGER_CONFIG=os.path.abspath("config.toml"))
    log = open("app.log", "w")
    app = subprocess.Popen([os.path.join(bins, "slinger-app"), "--drive", "a.sock"],
                           env=env, stdout=log, stderr=log)
    a = "a.sock"
    rows = []
    try:
        wait_for("a.sock")

        def go(request):
            ms, reply = ask(a, request)
            if not reply.get("ok"):
                sys.exit(f"✗ drive {request}: {reply}")
            return ms, reply["said"]

        go({"op": "resize", "w": 1440, "h": 900})

        def scene():
            return go({"op": "scene"})[1]

        def settled(title):
            """Ticks until the window shows `title` and its marks stop changing."""
            last, same = None, 0
            end = time.time() + 60
            while time.time() < end:
                go({"op": "tick"})
                now = scene()
                if title in now.split("\n")[0] and now == last:
                    same += 1
                    if same >= 3:
                        return now
                else:
                    same = 0
                last = now
                time.sleep(0.02)
            sys.exit(f"✗ the window did not settle on {title}")

        settled('"small"')
        for x in (1000, 1001):
            # A driven window's first toggle leaves the sidebar shut, and a
            # command draws nothing until the next event: a pointer move draws it.
            go({"op": "command", "name": "toggle-sidebar"})
            go({"op": "move", "x": x, "y": 500})
            side = settled('"small"')
            if "WORKSPACES" in side:
                break
        rows_at = {}
        for line in side.split("\n"):
            m = re.match(r'sidebar ([\d.]+),([\d.]+) ([\d.]+)x([\d.]+) \S+(?: on \S+)? "(big|small)"', line)
            if m:
                x, y, w, h = map(float, m.groups()[:4])
                rows_at[m.group(5)] = (x + w / 2, y + h / 2)
        if set(rows_at) != {"big", "small"}:
            sys.exit(f"✗ no sidebar rows for both workspaces:\n{side}")

        def marks(text):
            return sum(1 for line in text.split("\n") if line.startswith("  i"))

        def switch(to):
            """Tap the row and poll (a tick, then the scene) until the outline
            has not changed for a second: (ms to the new title, ms to the
            final outline, the marks it shows)."""
            x, y = rows_at[to]
            start = time.perf_counter()
            go({"op": "press", "x": x, "y": y})
            go({"op": "lift", "x": x, "y": y})
            seen = []
            while True:
                go({"op": "tick"})
                now = scene()
                t = (time.perf_counter() - start) * 1000
                if not seen or seen[-1][1] != now:
                    seen.append((t, now))
                titled = [t for t, text in seen if f'"{to}"' in text.split("\n")[0]]
                if titled and t - seen[-1][0] > 1000:
                    break
                if t > 60000:
                    sys.exit(f"✗ the switch to {to} did not settle")
            return titled[0], seen[-1][0], marks(seen[-1][1])

        timed = {"big": ([], [], []), "small": ([], [], [])}
        count = {}
        for _ in range(max(3, runs // 2)):
            for to in ("big", "small"):
                titled, done, n = switch(to)
                timed[to][0].append(titled)
                timed[to][1].append(done)
                timed[to][2].append(done - titled)
                count[to] = n
        for to in ("big", "small"):
            rows.append(f"| app switch to {to}: tap to its title | {summary(timed[to][0])} | polled: a tick and a scene a round |")
            rows.append(f"| app switch to {to}: tap to the final outline ({count[to]} marks in view) | {summary(timed[to][1])} | |")
            rows.append(f"| app switch to {to}: its title to the final outline | {summary(timed[to][2])} | the canvas's part |")

        def pen(to, zoom, samples=200):
            """A press, drags, a lift: each drag one pen sample, timed alone."""
            in_view = marks(scene())
            per = []
            x0, y0 = 300.0, 450.0
            pressed = go({"op": "press", "x": x0, "y": y0, "tablet": True})[1]
            for i in range(samples):
                # A zigzag, so no two samples are one point.
                x, y = x0 + (i % 400) * 2.0, y0 + (i // 400) * 20.0 + (i % 7)
                ms, said = go({"op": "drag", "x": x, "y": y, "tablet": True})
                per.append(ms)
            go({"op": "lift", "x": x, "y": y, "tablet": True})
            drawn = marks(scene()) - in_view
            note = f"press {pressed}, drag {said}, {drawn} new mark"
            rows.append(f"| app pen sample on {to}, {zoom}, {in_view} marks in view, first 100 | {summary(per[:100])} | {note} |")
            rows.append(f"| app pen sample on {to}, {zoom}, samples {samples - 100}-{samples} | {summary(per[-100:])} | |")
            go({"op": "tick"})

        for to in ("big", "small"):
            switch(to)
            go({"op": "command", "name": "tool-pen"})
            go({"op": "move", "x": 600, "y": 300})
            pen(to, "zoom as opened")
            pen(to, "zoom as opened, a 2000-sample stroke", 2000)
            for _ in range(8):
                go({"op": "command", "name": "zoom-out"})
            go({"op": "move", "x": 601, "y": 300})
            zoom = re.search(r"\| (\d+%) \|", scene())
            pen(to, f"zoomed out to {zoom.group(1) if zoom else '?'}")
            for _ in range(8):
                go({"op": "command", "name": "zoom-in"})
            go({"op": "move", "x": 600, "y": 300})
        idle = [go({"op": "clip"})[0] for _ in range(runs)]
        rows.append(f"| drive request that does nothing (clip) | {summary(idle)} | the socket's own cost |")
    finally:
        app.terminate()
        try:
            app.wait(10)
        except subprocess.TimeoutExpired:
            app.kill()
    return rows


if __name__ == "__main__":
    main()
