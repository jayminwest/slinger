
// shapes.js: fig.text, box, slot, line, path, circle, ellipse, arrow, and measure.

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

function words(call, key, v) {
  if (typeof v === 'number' && Number.isFinite(v)) return String(v);
  if (typeof v !== 'string' || v === '') fail(call, `${key} must be a non-empty string, got ${show(v)}`);
  return v;
}

// A text's size: its own, its part's, else `fallback`.
function sizeOf(call, o, fallback) {
  if (o.size !== undefined) return checkSize(call, o.size);
  if (scope !== null && scope.style.size !== undefined) return scope.style.size;
  return fallback;
}

function wrapWidth(call, o) {
  return o.w === undefined ? 0 : positive(call, 'w', o.w);
}

// What a shape's own options say, with fill and dash checked for its kind.
function own(call, o, kind) {
  const s = style(call, o);
  const fills = kind === 'box' || kind === 'closed';
  if (s.fill !== undefined && !fills) fail(call, 'fill takes a box, a circle, an ellipse and a path with closed: true');
  if (kind === 'closed') s.closed = true;
  return s;
}

const SHAPE = ['color', 'fill', 'dash', 'name', 'locked'];

G.measure = function measure(str, opts) {
  const call = 'measure';
  const o = options(call, opts, ['size', 'w']);
  return textFrame(words(call, 'text', str), wrapWidth(call, o), sizeOf(call, o, 24));
};

// A text from its top left, or with align its centre or right edge at x.
function text(call, str, x, y, o) {
  const s = words(call, 'text', str);
  number(call, 'x', x);
  number(call, 'y', y);
  const size = sizeOf(call, o, 24);
  const wrap = wrapWidth(call, o);
  const f = textFrame(s, wrap, size);
  if (o.align !== undefined && !['left', 'center', 'right'].includes(o.align)) {
    fail(call, `align must be "left", "center" or "right", got ${show(o.align)}`);
  }
  const left = o.align === 'center' ? x - f.w / 2 : o.align === 'right' ? x - f.w : x;
  const leaf = new Leaf(call, 'text', { x: left, y, w: f.w, h: f.h }, own(call, o, 'text'));
  Object.assign(leaf, { text: s, size, wrap });
  return leaf;
}

fig.text = entry(function (str, x, y, opts) {
  const call = 'fig.text';
  return text(call, str, x, y, options(call, opts, ['color', 'size', 'w', 'align', 'name', 'locked']));
});

// A box, with its label inside at the top left, wrapped to the box.
function box(call, x, y, w, h, o) {
  number(call, 'x', x);
  number(call, 'y', y);
  positive(call, 'w', w);
  positive(call, 'h', h);
  const leaf = new Leaf(call, 'box', { x, y, w, h }, own(call, o, 'box'));
  if (o.label !== undefined && o.label !== null && o.label !== '') {
    const size = o.size === undefined ? 20 : checkSize(call, o.size);
    const pad = size / 2;
    const s = words(call, 'label', o.label);
    const inside = w - 2 * pad >= size * CELL ? w - 2 * pad : 0;
    leaf.attach(text(call, s, x + pad, y + pad, { size, w: inside || undefined }));
  }
  return leaf;
}

fig.box = entry(function (x, y, w, h, opts) {
  const call = 'fig.box';
  return box(call, x, y, w, h, options(call, opts, [...SHAPE, 'label', 'size']));
});

// A box meant to be filled in: dashed for the user, dotted for the agent.
fig.slot = entry(function (x, y, w, h, opts) {
  const call = 'fig.slot';
  const o = options(call, opts, ['color', 'fill', 'name', 'locked', 'label', 'size', 'by']);
  if (o.by !== undefined && o.by !== 'user' && o.by !== 'agent') fail(call, `by must be "user" or "agent", got ${show(o.by)}`);
  return box(call, x, y, w, h, Object.assign({}, o, { by: undefined, dash: o.by === 'agent' ? 'dotted' : 'dashed' }));
});

function point(call, name, p) {
  if (!Array.isArray(p) || p.length !== 2 || !p.every((v) => typeof v === 'number' && Number.isFinite(v))) {
    fail(call, `${name} must be [x, y] or a handle, got ${show(p)}`);
  }
  return p;
}

// Where a line from `c` towards `d` leaves the frame `f` centred on `c`.
function edge(f, c, d) {
  const dx = d[0] - c[0];
  const dy = d[1] - c[1];
  const t = Math.min(dx === 0 ? Infinity : f.w / 2 / Math.abs(dx), dy === 0 ? Infinity : f.h / 2 / Math.abs(dy), 1);
  return [c[0] + dx * t, c[1] + dy * t];
}

// The two ends of a line or arrow: four numbers, or two ends each a point
// or a handle (met at its edge where it is now).
function ends(call, args) {
  if (args.length >= 4 && typeof args[0] !== 'object') {
    const [x1, y1, x2, y2] = args;
    for (const [n, v] of [['x1', x1], ['y1', y1], ['x2', x2], ['y2', y2]]) number(call, n, v);
    return { a: [x1, y1], b: [x2, y2], opts: args[4] };
  }
  if (args.length < 2) fail(call, `takes (x1, y1, x2, y2, opts?) or (from, to, opts?), got ${args.length} arguments`);
  const frame = (v) => (v instanceof Handle ? v.bounds() : isFrame(v) ? v : null);
  const centre = (f) => [f.x + f.w / 2, f.y + f.h / 2];
  const [from, to] = args;
  const f = frame(from);
  const t = frame(to);
  const a = f ? centre(f) : point(call, 'from', from);
  const b = t ? centre(t) : point(call, 'to', to);
  return { a: f ? edge(f, a, b) : a, b: t ? edge(t, b, a) : b, opts: args[2] };
}

// A line or an arrow from a to b. Width 2 is the canvas's arrow; a line of
// another width is a two-point stroke.
function connect(call, a, b, o, heads) {
  if (o.heads !== undefined && !['end', 'both', 'none'].includes(o.heads)) {
    fail(call, `heads must be "end", "both" or "none", got ${show(o.heads)}`);
  }
  const h = o.heads === undefined ? heads : o.heads;
  const width = o.width === undefined ? 2 : positive(call, 'width', o.width);
  let leaf;
  if (width === 2) {
    leaf = new Leaf(call, 'arrow', { pts: [a, b] }, own(call, o, 'line'));
    leaf.heads = h;
  } else if (h === 'none') {
    leaf = new Leaf(call, 'stroke', { pts: [a, b] }, own(call, o, 'line'));
    leaf.width = width;
  } else {
    fail(call, `width must be 2 on an arrow with heads (the canvas draws every arrow 2 wide); heads: "none" draws a line of width ${width}`);
  }
  if (o.label !== undefined && o.label !== null && o.label !== '') label(call, leaf, a, b, words(call, 'label', o.label), o);
  return leaf;
}

fig.line = entry(function (...args) {
  const call = 'fig.line';
  const { a, b, opts } = ends(call, args);
  return connect(call, a, b, options(call, opts, ['color', 'dash', 'width', 'name', 'locked']), 'none');
});

fig.arrow = entry(function (...args) {
  const call = 'fig.arrow';
  const { a, b, opts } = ends(call, args);
  const o = options(call, opts, ['color', 'dash', 'width', 'heads', 'label', 'size', 'name', 'locked']);
  return connect(call, a, b, o, 'end');
});

// A label beside the line from a to b, never on it: above a line that runs
// across, to the right of one that runs straight down, at its middle. When
// that spot covers a text or a line already drawn (two arrows into one
// box), the first free one of the other side, then a third of the way from
// either end, on either side; with none free, the middle. A label longer
// than its arrow (along it) warns: it reads as beside nothing.
function label(call, leaf, a, b, s, o) {
  const size = sizeOf(call, o, 24);
  const f = textFrame(s, 0, size);
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
  let nx = (b[1] - a[1]) / len;
  let ny = -(b[0] - a[0]) / len;
  if (ny > 0 || (ny === 0 && nx < 0)) {
    nx = -nx;
    ny = -ny;
  }
  const away = 6 + Math.abs(nx) * f.w / 2 + Math.abs(ny) * f.h / 2;
  const spot = ([t, side]) => ({
    x: a[0] + (b[0] - a[0]) * t + side * nx * away - f.w / 2,
    y: a[1] + (b[1] - a[1]) * t + side * ny * away - f.h / 2,
    w: f.w,
    h: f.h,
  });
  const along = Math.abs(b[0] - a[0]) / len * f.w + Math.abs(b[1] - a[1]) / len * f.h;
  if (along > len) {
    warning(leaf.line, leaf.col, `${call}: the label ${JSON.stringify(s)} is ${Math.round(along)} long beside an arrow ${Math.round(len)} long; lengthen the arrow or shorten the label`);
  }
  const spots = [[0.5, 1], [0.5, -1], [0.35, 1], [0.65, 1], [0.35, -1], [0.65, -1]].map(spot);
  const at = spots.find((p) => free(p, leaf)) || spots[0];
  leaf.attach(text(call, s, at.x, at.y, { size }));
}

// Whether frame f covers no text and crosses no line drawn so far, but `self`.
function free(f, self) {
  return leaves.every((d) => d === self || !overlaps(f, grown(d.bounds()))
    || (d.kind !== 'text' && !d.segments().some(([p, q]) => crosses(f, p, q))));
}

// A frame one unit larger each way, so a straight line's empty frame still meets.
function grown(f) {
  return { x: f.x - 1, y: f.y - 1, w: f.w + 2, h: f.h + 2 };
}

// A polyline; closed joins the last point to the first.
function stroke(call, points, o, closed) {
  const width = o.width === undefined ? 2 : positive(call, 'width', o.width);
  const leaf = new Leaf(call, 'stroke', { pts: points.map((p) => [p[0], p[1]]) }, own(call, o, closed ? 'closed' : 'stroke'));
  leaf.width = width;
  return leaf;
}

function path(call, points, o) {
  if (!Array.isArray(points) || points.length < 2) fail(call, `points must be an array of two or more [x, y], got ${show(points)}`);
  points.forEach((p, i) => point(call, `points[${i}]`, p));
  if (o.closed !== undefined) bool(call, 'closed', o.closed);
  return stroke(call, points, o, o.closed === true);
}

fig.path = entry(function (points, opts) {
  const call = 'fig.path';
  return path(call, points, options(call, opts, [...SHAPE, 'closed', 'width']));
});

// An ellipse as a closed stroke of 48 points, so its bounds are exact.
function ellipse(call, cx, cy, rx, ry, o) {
  const points = [];
  for (let i = 0; i < 48; i += 1) {
    const a = (i * 2 * Math.PI) / 48;
    points.push([cx + rx * Math.cos(a), cy + ry * Math.sin(a)]);
  }
  return stroke(call, points, o, true);
}

fig.circle = entry(function (cx, cy, r, opts) {
  const call = 'fig.circle';
  number(call, 'cx', cx);
  number(call, 'cy', cy);
  positive(call, 'r', r);
  return ellipse(call, cx, cy, r, r, options(call, opts, [...SHAPE, 'width']));
});

fig.ellipse = entry(function (cx, cy, rx, ry, opts) {
  const call = 'fig.ellipse';
  number(call, 'cx', cx);
  number(call, 'cy', cy);
  positive(call, 'rx', rx);
  positive(call, 'ry', ry);
  return ellipse(call, cx, cy, rx, ry, options(call, opts, [...SHAPE, 'width']));
});
