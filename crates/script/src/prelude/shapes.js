
// shapes.js: box, line, arrow, poly, circle, text and measure.

// Text measure, the core's rule (render::text_size and text_frame): a
// character cell is half the size wide and the size tall, lines wrap as
// nvim wraps them (raster::wrap) to the frame's width, and zero-width
// characters take no cell.
const CELL = 0.5;
const ZERO_WIDTH = /[\u200B\u200C\u200D\u2060\uFE0F\uFEFF]/gu;

// The cells of each row: a row breaks after the last space in its cells
// that a non-space follows, never in the line's leading spaces, the space
// ending its row; with none, it is cut at the cells.
function wrap(str, columns) {
  const rows = [];
  for (const paragraph of str.split('\n')) {
    const chars = Array.from(paragraph.replace(ZERO_WIDTH, ''));
    let lead = 0;
    while (lead < chars.length && chars[lead] === ' ') lead += 1;
    let first = 0;
    while (chars.length - first > columns) {
      const cut = first + columns;
      let next = cut;
      for (let i = cut - 1; i >= Math.max(first, lead + 1); i -= 1) {
        if (chars[i] === ' ' && chars[i + 1] !== ' ') {
          next = i + 1;
          break;
        }
      }
      rows.push(next - first);
      first = next;
    }
    rows.push(chars.length - first);
  }
  return rows;
}

function columns(w, size) {
  const cell = size * CELL;
  return w >= cell ? Math.floor(w / cell) : Infinity;
}

function textSize(str, cols, size) {
  const rows = wrap(str, Math.max(cols, 1));
  return { w: Math.max(...rows) * size * CELL, h: rows.length * size };
}

function textFrame(str, w, size) {
  if (w >= size * CELL) return { w, h: textSize(str, columns(w, size), size).h };
  const s = textSize(str, Infinity, size);
  return { w: Math.max(s.w, size * CELL), h: s.h };
}

function words(call, v) {
  if (typeof v === 'number' && Number.isFinite(v)) return String(v);
  if (typeof v !== 'string' || v === '') fail(call, `text must be a non-empty string, got ${show(v)}`);
  return v;
}

function sizeOf(call, opts) {
  const size = inherited(opts, 'size');
  return size === undefined ? 24 : checkSize(call, size);
}

function wrapWidth(call, opts) {
  return opts.w === undefined ? 0 : positive(call, 'w', opts.w);
}

const SHAPE = ['color', 'fill', 'dash', 'locked'];
const TEXT = ['color', 'size', 'w', 'locked'];

function measure(str, opts) {
  const call = 'measure';
  const o = options(call, opts, ['size', 'w']);
  const s = words(call, str);
  return textFrame(s, wrapWidth(call, o), sizeOf(call, o));
}

function text(x, y, str, opts) {
  const call = 'text';
  number(call, 'x', x);
  number(call, 'y', y);
  const s = words(call, str);
  const o = options(call, opts, TEXT);
  const size = sizeOf(call, o);
  const f = textFrame(s, wrapWidth(call, o), size);
  const style = styled(call, o, 'text');
  const item = { kind: 'text', color: style.color, x: T(x), y: T(y), w: T(f.w), h: T(f.h), text: s };
  if (T(size) !== 240) item.size = T(size);
  return put(call, item, style, { x, y, w: f.w, h: f.h }, []);
}

function box(x, y, w, h, opts) {
  const call = 'box';
  number(call, 'x', x);
  number(call, 'y', y);
  positive(call, 'w', w);
  positive(call, 'h', h);
  const style = styled(call, options(call, opts, SHAPE), 'box');
  const item = { kind: 'box', color: style.color, x: T(x), y: T(y), w: T(w), h: T(h) };
  const corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
  const edges = corners.map((p, i) => [p, corners[(i + 1) % 4]]);
  return put(call, item, style, { x, y, w, h }, edges);
}

function bounds(points) {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  const x = Math.min(...xs);
  const y = Math.min(...ys);
  return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
}

function point(call, name, p) {
  if (!Array.isArray(p) || p.length !== 2 || !p.every((v) => typeof v === 'number' && Number.isFinite(v))) {
    fail(call, `${name} must be [x, y] or a frame a shape returned, got ${show(p)}`);
  }
  return p;
}

function isFrame(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v)
    && ['x', 'y', 'w', 'h'].every((k) => typeof v[k] === 'number' && Number.isFinite(v[k]));
}

// Where a line from `c` towards `d` leaves the frame `f` centred on `c`.
function edge(f, c, d) {
  const dx = d[0] - c[0];
  const dy = d[1] - c[1];
  const t = Math.min(dx === 0 ? Infinity : f.w / 2 / Math.abs(dx), dy === 0 ? Infinity : f.h / 2 / Math.abs(dy), 1);
  return [c[0] + dx * t, c[1] + dy * t];
}

// The two ends of a line or arrow: four numbers, or two ends each a point or a frame (met at its edge).
function ends(call, args) {
  if (args.length >= 4 && typeof args[0] !== 'object') {
    const [x1, y1, x2, y2] = args;
    for (const [n, v] of [['x1', x1], ['y1', y1], ['x2', x2], ['y2', y2]]) number(call, n, v);
    return { a: [x1, y1], b: [x2, y2], opts: args[4] };
  }
  if (args.length < 2) fail(call, `takes (x1, y1, x2, y2, opts?) or (from, to, opts?), got ${args.length} arguments`);
  const centre = (f) => [f.x + f.w / 2, f.y + f.h / 2];
  const [from, to] = args;
  const a = isFrame(from) ? centre(from) : point(call, 'from', from);
  const b = isFrame(to) ? centre(to) : point(call, 'to', to);
  return { a: isFrame(from) ? edge(from, a, b) : a, b: isFrame(to) ? edge(to, b, a) : b, opts: args[2] };
}

function connect(call, args, heads, allowed) {
  const { a, b, opts } = ends(call, args);
  const o = options(call, opts, allowed);
  if (o.heads !== undefined && !['end', 'both', 'none'].includes(o.heads)) {
    fail(call, `heads must be "end", "both" or "none", got ${show(o.heads)}`);
  }
  const style = styled(call, o, 'line');
  const item = { kind: 'arrow', color: style.color, pts: pts([a, b]) };
  const h = o.heads === undefined ? heads : o.heads;
  if (h !== 'end') item.heads = h;
  const frame = put(call, item, style, bounds([a, b]), [[a, b]]);
  if (o.label !== undefined) label(call, a, b, words(call, o.label), o);
  return frame;
}

// A label beside the middle of the line from a to b, never on it: above a
// line that runs across, to the right of one that runs straight down.
function label(call, a, b, s, o) {
  const size = sizeOf(call, o);
  const f = textFrame(s, 0, size);
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
  let nx = (b[1] - a[1]) / len;
  let ny = -(b[0] - a[0]) / len;
  if (ny > 0 || (ny === 0 && nx < 0)) {
    nx = -nx;
    ny = -ny;
  }
  const away = 6 + Math.abs(nx) * f.w / 2 + Math.abs(ny) * f.h / 2;
  const cx = (a[0] + b[0]) / 2 + nx * away;
  const cy = (a[1] + b[1]) / 2 + ny * away;
  const opts = { size };
  if (o.color !== undefined) opts.color = o.color;
  if (o.locked !== undefined) opts.locked = o.locked;
  text(cx - f.w / 2, cy - f.h / 2, s, opts);
}

function line(...args) {
  return connect('line', args, 'none', ['color', 'dash', 'locked']);
}

function arrow(...args) {
  return connect('arrow', args, 'end', ['color', 'dash', 'locked', 'heads', 'label', 'size']);
}

function stroke(call, points, closed, o) {
  const width = o.width === undefined ? 2 : positive(call, 'width', o.width);
  const style = styled(call, o, closed ? 'closed' : 'stroke');
  const item = { kind: 'stroke', color: style.color, width: Math.max(T(width), 1), pts: pts(points) };
  if (closed) item.closed = true;
  const segments = points.slice(1).map((p, i) => [points[i], p]);
  if (closed) segments.push([points[points.length - 1], points[0]]);
  return put(call, item, style, bounds(points), segments);
}

function poly(points, opts) {
  const call = 'poly';
  if (!Array.isArray(points) || points.length < 2) fail(call, `points must be an array of two or more [x, y], got ${show(points)}`);
  points.forEach((p, i) => point(call, `points[${i}]`, p));
  const o = options(call, opts, [...SHAPE, 'closed', 'width']);
  if (o.closed !== undefined && typeof o.closed !== 'boolean') fail(call, `closed must be true or false, got ${show(o.closed)}`);
  return stroke(call, points, o.closed === true, o);
}

function circle(cx, cy, r, opts) {
  const call = 'circle';
  number(call, 'cx', cx);
  number(call, 'cy', cy);
  positive(call, 'r', r);
  const o = options(call, opts, [...SHAPE, 'width']);
  const points = [];
  for (let i = 0; i < 48; i += 1) {
    const a = (i * 2 * Math.PI) / 48;
    points.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
  }
  stroke(call, points, true, o);
  return { x: cx - r, y: cy - r, w: 2 * r, h: 2 * r };
}

Object.assign(G, { box, line, arrow, poly, circle, text, measure });
