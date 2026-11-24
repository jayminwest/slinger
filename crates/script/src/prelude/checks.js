
// checks.js: once the script has run, every shape leaves the engine with
// its style resolved, then the warnings that come back beside the shapes:
// text over a line, texts overlapping, a box drawn again on one already
// there, a label taller than its box, a word wider than its text's frame,
// two parts that overlap, a shape far from the others.

const FAR = 5000;
const CELLS = 256;

// Canvas units to stored tenths, rounded half away from zero as the core does.
function T(v) {
  return Math.sign(v) * Math.round(Math.abs(v) * 10);
}

// A point list as the core stores it: the first pair absolute, each later
// pair the change from the one before, between rounded values.
function pts(points) {
  const out = [];
  let px = 0;
  let py = 0;
  for (const [x, y] of points) {
    const tx = T(x);
    const ty = T(y);
    out.push(tx - px, ty - py);
    px = tx;
    py = ty;
  }
  return out;
}

// The add item object for a shape, in tenths.
function item(l) {
  const color = l.get_('color') || l.base;
  const g = l.geo;
  let out;
  if (l.kind === 'box') out = { kind: 'box', color, x: T(g.x), y: T(g.y), w: T(g.w), h: T(g.h) };
  else if (l.kind === 'text') {
    out = { kind: 'text', color, x: T(g.x), y: T(g.y), w: T(g.w), h: T(g.h), text: l.text };
    if (T(l.size) !== 240) out.size = T(l.size);
  } else if (l.kind === 'arrow') {
    out = { kind: 'arrow', color, pts: pts(g.pts) };
    if (l.heads !== 'end') out.heads = l.heads;
  } else {
    out = { kind: 'stroke', color, width: Math.max(T(l.width), 1), pts: pts(g.pts) };
    if (l.own.closed) out.closed = true;
  }
  const fill = l.get_('fill');
  if ((l.kind === 'box' || l.own.closed) && fill !== undefined) out.fill = tint(l.call, fill, color);
  const dash = l.get_('dash');
  if (l.kind !== 'text' && dash !== undefined && dash !== 'solid') out.dash = dash;
  if (l.get_('locked') !== false) out.locked = true;
  if (input.name) out.fig = input.name;
  const part = l.get_('part');
  if (part !== undefined && part !== null) out.part = part;
  return out;
}

// The open inside of a frame, so frames that only touch do not meet.
function overlaps(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

// Whether the segment p-q passes through the open inside of frame f (Liang-Barsky).
function crosses(f, p, q) {
  let t0 = 0;
  let t1 = 1;
  const dx = q[0] - p[0];
  const dy = q[1] - p[1];
  const sides = [[-dx, p[0] - f.x], [dx, f.x + f.w - p[0]], [-dy, p[1] - f.y], [dy, f.y + f.h - p[1]]];
  for (const [pp, qq] of sides) {
    if (pp === 0) {
      if (qq <= 0) return false;
    } else {
      const t = qq / pp;
      if (pp < 0) t0 = Math.max(t0, t);
      else t1 = Math.min(t1, t);
    }
  }
  return t0 < t1;
}

// The grid cells a frame touches, as keys.
function keys(f) {
  const out = [];
  for (let i = Math.floor(f.x / CELLS); i <= Math.floor((f.x + f.w) / CELLS); i += 1) {
    for (let j = Math.floor(f.y / CELLS); j <= Math.floor((f.y + f.h) / CELLS); j += 1) out.push(`${i},${j}`);
  }
  return out;
}

// What the call that drew a shape is called in a message: "box", "line".
function what(s) {
  return s.call.replace(/^(fig|p)\./, '');
}

const from = (s) => (s.line > 0 ? ` from line ${s.line}` : '');

// A box with no fill on the frame of a box drawn before it (a table's cell,
// then the part's own box in it) adds nothing but a doubled edge.
function twice(drawn) {
  const boxes = new Map();
  for (const s of drawn) {
    if (s.kind !== 'box') continue;
    const key = [s.frame.x, s.frame.y, s.frame.w, s.frame.h].map(T).join(',');
    const first = boxes.get(key);
    if (first === undefined) boxes.set(key, s);
    else if (!s.fill) {
      warning(s.line, s.col, `${s.call}: is drawn again on the box${from(first)}; a table's cell is boxed already, draw in it with t.cell(r, c)`);
    }
  }
}

// A label that runs out of the bottom of its box; a word wider than the
// width its text wraps to, which is cut.
function fits(drawn) {
  for (const s of drawn) {
    if (s.kind !== 'text') continue;
    const o = s.leaf.owner;
    if (o !== null && o.kind === 'box' && T(s.frame.y + s.frame.h) > T(o.geo.y + o.geo.h)) {
      warning(o.line, o.col, `${o.call}: the label ${JSON.stringify(s.text)} is ${Math.round(s.frame.h)} tall in a box ${Math.round(o.geo.h)} tall; make the box taller or the label shorter`);
    }
    const wrap = s.leaf.wrap;
    if (wrap > 0) {
      const cell = s.leaf.size * CELL;
      const long = s.text.split(/\s+/).find((word) => Array.from(word.replace(ZERO_WIDTH, '')).length * cell > wrap);
      if (long !== undefined) {
        const said = o !== null ? `the label of the ${what(o)}` : 'the text';
        warning(s.line, s.col, `${s.call}: the word ${JSON.stringify(long)} is ${Array.from(long).length * cell} wide, wider than ${said} (${wrap}); it is cut`);
      }
    }
  }
}

// Two parts whose shapes overlap: an agent finding one by its name gets
// the other's area too.
function parts(drawn) {
  const all = new Map();
  for (const s of drawn) {
    if (s.part === undefined) continue;
    const p = all.get(s.part);
    if (p === undefined) all.set(s.part, { first: s, frames: [s.frame] });
    else p.frames.push(s.frame);
  }
  const list = [...all].map(([name, p]) => {
    const f = union(p.frames);
    return { name, first: p.first, f: { x: T(f.x), y: T(f.y), w: T(f.w), h: T(f.h) } };
  });
  list.forEach((b, j) => list.slice(0, j).forEach((a) => {
    if (overlaps(a.f, b.f)) {
      warning(b.first.line, b.first.col, `name: the part ${JSON.stringify(b.name)} overlaps the part ${JSON.stringify(a.name)}${from(a.first)}: ink where their frames meet may be said to be over either`);
    }
  }));
}

function finish() {
  if (overflow) throw overflow;
  const drawn = leaves.map((leaf) => {
    const out = item(leaf);
    emit(JSON.stringify(out));
    const frame = leaf.bounds();
    return { leaf, kind: leaf.kind, call: leaf.call, frame, segments: leaf.segments(), line: leaf.line, col: leaf.col, text: leaf.text, fill: out.fill !== undefined, part: out.part };
  });
  const grid = new Map();
  drawn.forEach((s, i) => {
    for (const k of keys(s.frame)) {
      if (!grid.has(k)) grid.set(k, []);
      grid.get(k).push(i);
    }
  });
  drawn.forEach((s, i) => {
    if (s.kind !== 'text') return;
    const near = new Set();
    for (const k of keys(s.frame)) for (const j of grid.get(k)) if (j !== i) near.add(j);
    let over = null;
    let text = null;
    for (const j of [...near].sort((a, b) => a - b)) {
      const o = drawn[j];
      if (o.kind === 'text') {
        if (j < i && text === null && overlaps(s.frame, o.frame)) text = o;
      } else if (over === null && o.leaf !== s.leaf.owner && o.segments.some(([p, q]) => crosses(s.frame, p, q))) {
        over = o;
      }
    }
    if (over !== null) warning(s.line, s.col, `${s.call}: ${JSON.stringify(s.text)} is over the ${what(over)}${from(over)}`);
    if (text !== null) warning(s.line, s.col, `${s.call}: ${JSON.stringify(s.text)} overlaps the text ${JSON.stringify(text.text)}${from(text)}`);
  });
  twice(drawn);
  fits(drawn);
  parts(drawn);
  if (drawn.length > 1) {
    const mid = (a) => a.sort((p, q) => p - q)[Math.floor(a.length / 2)];
    const cx = mid(drawn.map((s) => s.frame.x + s.frame.w / 2));
    const cy = mid(drawn.map((s) => s.frame.y + s.frame.h / 2));
    for (const s of drawn) {
      const f = s.frame;
      const dx = Math.max(f.x - cx, cx - (f.x + f.w), 0);
      const dy = Math.max(f.y - cy, cy - (f.y + f.h), 0);
      if (Math.hypot(dx, dy) > FAR) warning(s.line, s.col, `${s.call}: is more than ${FAR} units from the other shapes`);
    }
  }
}
