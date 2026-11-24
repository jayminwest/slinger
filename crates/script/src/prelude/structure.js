
// structure.js: part, group, stack and table.

const NAME = /^[a-z0-9-]{1,40}$/;
const STYLE_OPTS = ['color', 'fill', 'dash', 'size', 'locked'];

function partName(call, name) {
  if (typeof name !== 'string' || !NAME.test(name)) {
    fail(call, `a part name is 1 to 40 of a-z, 0-9 and -, got ${show(name)}`);
  }
  return name;
}

G.part = function (name, opts, fn) {
  const call = 'part';
  partName(call, name);
  if (fn === undefined) [opts, fn] = [undefined, opts];
  callback(call, fn);
  const o = options(call, opts, STYLE_OPTS);
  if (part !== null) fail(call, `parts do not nest: ${name} is inside ${part}`);
  part = name;
  try {
    return within(call, o, fn);
  } finally {
    part = null;
  }
};

G.group = function (opts, fn) {
  const call = 'group';
  if (fn === undefined) [opts, fn] = [undefined, opts];
  callback(call, fn);
  return within(call, options(call, opts, STYLE_OPTS), fn);
};

function union(frames) {
  const x = Math.min(...frames.map((f) => f.x));
  const y = Math.min(...frames.map((f) => f.y));
  const w = Math.max(...frames.map((f) => f.x + f.w)) - x;
  const h = Math.max(...frames.map((f) => f.y + f.h)) - y;
  return { x, y, w, h };
}

G.stack = function (x, y, opts, fn) {
  const call = 'stack';
  number(call, 'x', x);
  number(call, 'y', y);
  if (fn === undefined) [opts, fn] = [undefined, opts];
  callback(call, fn);
  const o = options(call, opts, [...STYLE_OPTS, 'dir', 'gap']);
  if (o.dir !== undefined && o.dir !== 'down' && o.dir !== 'right') fail(call, `dir must be "down" or "right", got ${show(o.dir)}`);
  const gap = o.gap === undefined ? 8 : number(call, 'gap', o.gap);
  const down = o.dir !== 'right';
  const frames = [];
  let at = down ? y : x;
  const placed = (f) => {
    frames.push(f);
    at += (down ? f.h : f.w) + gap;
    return f;
  };
  const here = () => (down ? [x, at] : [at, y]);
  const s = {
    text(str, topts) {
      const [px, py] = here();
      return placed(text(px, py, str, topts));
    },
    box(w, h, bopts) {
      const [px, py] = here();
      return placed(box(px, py, w, h, bopts));
    },
    gap(n) {
      at += number('stack: gap', 'n', n);
    },
  };
  within(call, o, () => fn(s));
  return frames.length === 0 ? { x, y, w: 0, h: 0 } : union(frames);
};

G.table = function (x, y, rows, opts) {
  const call = 'table';
  number(call, 'x', x);
  number(call, 'y', y);
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every((r) => Array.isArray(r) && r.length > 0)) {
    fail(call, `rows must be an array of arrays of strings, got ${show(rows)}`);
  }
  const o = options(call, opts, [...STYLE_OPTS, 'cols', 'rowH', 'header', 'parts']);
  const cells = rows.map((r, i) => r.map((v, j) => {
    if (v === null || v === undefined || v === '') return '';
    if (typeof v !== 'string' && !(typeof v === 'number' && Number.isFinite(v))) {
      fail(call, `rows[${i}][${j}] must be a string, got ${show(v)}`);
    }
    return String(v);
  }));
  const size = sizeOf(call, o);
  const pad = size / 2;
  const count = Math.max(...cells.map((r) => r.length));
  let widths;
  if (o.cols === undefined) {
    widths = Array.from({ length: count }, (_, j) => Math.max(size, ...cells.map((r) => (r[j] ? textFrame(r[j], 0, size).w : 0))) + 2 * pad);
  } else if (Array.isArray(o.cols)) {
    if (o.cols.length !== count) fail(call, `cols has ${o.cols.length} widths for ${count} columns`);
    widths = o.cols.map((c, j) => positive(call, `cols[${j}]`, c));
  } else {
    widths = Array(count).fill(positive(call, 'cols', o.cols));
  }
  const rowH = o.rowH === undefined ? 1.6 * size : positive(call, 'rowH', o.rowH);
  if (o.parts !== undefined && (!Array.isArray(o.parts) || !o.parts.every((r) => r === null || Array.isArray(r)))) {
    fail(call, 'parts must be an array of arrays of part names, null for none');
  }
  if (o.parts !== undefined && part !== null) fail(call, `parts do not nest: the table is inside ${part}`);
  const left = (j) => x + widths.slice(0, j).reduce((a, b) => a + b, 0);
  const cell = (r, c) => ({ x: left(c), y: y + r * rowH, w: widths[c], h: rowH });
  from(() => within(call, o, () => cells.forEach((r, i) => r.forEach((v, j) => {
    const name = o.parts && o.parts[i] ? o.parts[i][j] : null;
    const draw = () => {
      const f = cell(i, j);
      box(f.x, f.y, f.w, f.h, i === 0 && o.header === true ? { fill: true } : {});
      if (v !== '') {
        const t = textFrame(v, 0, size);
        text(f.x + pad, f.y + (f.h - t.h) / 2, v);
      }
    };
    if (name === null || name === undefined) draw();
    else G.part(partName(call, name), draw);
  }))));
  const frame = { x, y, w: widths.reduce((a, b) => a + b, 0), h: rowH * cells.length };
  frame.cell = (r, c) => {
    if (!Number.isInteger(r) || !Number.isInteger(c) || r < 0 || c < 0 || r >= cells.length || c >= count) {
      fail('cell', `(${show(r)}, ${show(c)}) is not a cell of ${cells.length} rows and ${count} columns`);
    }
    return cell(r, c);
  };
  return frame;
};
