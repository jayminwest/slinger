
// checks.js: the warnings that come back beside the shapes once the script
// has run: text over a line, texts overlapping, a shape far from the others.

const FAR = 5000;
const CELLS = 256;

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

function finish() {
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
      } else if (over === null && o.segments.some(([p, q]) => crosses(s.frame, p, q))) {
        over = o;
      }
    }
    const said = (o) => (o.line > 0 ? ` from line ${o.line}` : '');
    if (over !== null) warning(s.line, s.col, `text: ${JSON.stringify(s.text)} is over the ${over.kind === 'arrow' ? 'line' : over.kind}${said(over)}`);
    if (text !== null) warning(s.line, s.col, `text: ${JSON.stringify(s.text)} overlaps the text${said(text)}`);
  });
  if (drawn.length > 1) {
    const mid = (a) => a.sort((p, q) => p - q)[Math.floor(a.length / 2)];
    const cx = mid(drawn.map((s) => s.frame.x + s.frame.w / 2));
    const cy = mid(drawn.map((s) => s.frame.y + s.frame.h / 2));
    for (const s of drawn) {
      const f = s.frame;
      const dx = Math.max(f.x - cx, cx - (f.x + f.w), 0);
      const dy = Math.max(f.y - cy, cy - (f.y + f.h), 0);
      if (Math.hypot(dx, dy) > FAR) warning(s.line, s.col, `${s.kind === 'arrow' ? 'line' : s.kind}: is more than ${FAR} units from the other shapes`);
    }
  }
}
