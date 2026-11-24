
// structure.js: fig.part, group, stack, grid and table.

const PART = ['color', 'fill', 'dash', 'locked', 'size'];

// Every shape fn draws is in part `name`, with these defaults. Returns a
// handle over what it drew.
fig.part = entry(function (name, opts, fn) {
  const call = 'fig.part';
  partName(call, name);
  if (fn === undefined) [opts, fn] = [undefined, opts];
  callback(call, fn);
  const o = options(call, opts, PART);
  if (scope !== null) fail(call, `parts do not nest: ${name} is inside ${scope.name}`);
  const s = style(call, o);
  if (o.size !== undefined) s.size = checkSize(call, o.size);
  const first = leaves.length;
  scope = { name, style: s };
  try {
    fn();
  } finally {
    scope = null;
  }
  return new Group(leaves.slice(first).filter((l) => l.owner === null), {});
});

const GROUP = ['color', 'fill', 'dash', 'locked', 'name'];

// The handles and the options of a call that takes handles, then options last.
function split(call, args, allowed) {
  const last = args[args.length - 1];
  const has = args.length > 0 && last !== null && typeof last === 'object' && !Array.isArray(last) && !(last instanceof Handle);
  const o = has ? options(call, last, allowed) : {};
  return { list: handles(call, has ? args.slice(0, -1) : args), o };
}

fig.group = entry(function (...args) {
  const call = 'fig.group';
  if (args.some((a) => typeof a === 'function')) handles(call, args.filter((a) => typeof a === 'function'));
  const { list, o } = split(call, args, GROUP);
  return new Group(list, style(call, o));
});

// The handles of a stack or a grid: one array.
function placed(call, list) {
  if (!Array.isArray(list)) fail(call, `takes an array of handles first, got ${show(list)}`);
  return handles(call, list);
}

// Moves each handle after the one before, down or right; the other way
// each stays where it is.
fig.stack = entry(function (list, opts) {
  const call = 'fig.stack';
  const hs = placed(call, list);
  const o = options(call, opts, [...GROUP, 'dir', 'gap']);
  if (o.dir !== undefined && o.dir !== 'down' && o.dir !== 'right') fail(call, `dir must be "down" or "right", got ${show(o.dir)}`);
  const gap = o.gap === undefined ? 24 : number(call, 'gap', o.gap);
  for (let i = 1; i < hs.length; i += 1) {
    if (o.dir === 'right') hs[i].moveBy(hs[i - 1].right + gap - hs[i].x, 0);
    else hs[i].moveBy(0, hs[i - 1].bottom + gap - hs[i].y);
  }
  return new Group(hs, style(call, o));
});

// Places the handles in rows of `cols` from (x, y): each column as wide as
// its widest, each row as tall as its tallest.
fig.grid = entry(function (list, opts) {
  const call = 'fig.grid';
  const hs = placed(call, list);
  const o = options(call, opts, [...GROUP, 'cols', 'gap', 'x', 'y']);
  const cols = o.cols === undefined ? 3 : o.cols;
  if (!Number.isInteger(cols) || cols < 1) fail(call, `cols must be a whole number above 0, got ${show(cols)}`);
  const gap = o.gap === undefined ? 24 : number(call, 'gap', o.gap);
  const x = o.x === undefined ? 0 : number(call, 'x', o.x);
  const y = o.y === undefined ? 0 : number(call, 'y', o.y);
  const widths = [];
  const heights = [];
  hs.forEach((h, i) => {
    const [r, c] = [Math.floor(i / cols), i % cols];
    widths[c] = Math.max(widths[c] || 0, h.w);
    heights[r] = Math.max(heights[r] || 0, h.h);
  });
  const sum = (a, n) => a.slice(0, n).reduce((s, v) => s + v + gap, 0);
  hs.forEach((h, i) => h.moveTo(x + sum(widths, i % cols), y + sum(heights, Math.floor(i / cols))));
  return new Group(hs, style(call, o));
});

// A grid of boxed cells, one text per non-empty cell; columns fit their
// longest cell unless `widths` says. The first row is tinted unless
// header: false. `parts` names cells: an array of rows of part names.
fig.table = entry(function (rows, x, y, opts) {
  const call = 'fig.table';
  if (!Array.isArray(rows) || rows.length === 0 || !rows.every((r) => Array.isArray(r) && r.length > 0)) {
    fail(call, `rows must be an array of arrays of strings, got ${show(rows)}`);
  }
  number(call, 'x', x);
  number(call, 'y', y);
  const o = options(call, opts, [...GROUP, 'size', 'header', 'widths', 'rowH', 'parts']);
  const cells = rows.map((r, i) => r.map((v, j) => {
    if (v === null || v === undefined || v === '') return '';
    if (typeof v !== 'string' && !(typeof v === 'number' && Number.isFinite(v))) {
      fail(call, `rows[${i}][${j}] must be a string, got ${show(v)}`);
    }
    return String(v);
  }));
  const size = sizeOf(call, o, 20);
  const pad = size / 2;
  const count = Math.max(...cells.map((r) => r.length));
  let widths;
  if (o.widths === undefined) {
    widths = Array.from({ length: count }, (_, j) => Math.max(size, ...cells.map((r) => (r[j] ? textFrame(r[j], 0, size).w : 0))) + 2 * pad);
  } else if (Array.isArray(o.widths)) {
    if (o.widths.length !== count) fail(call, `widths has ${o.widths.length} widths for ${count} columns`);
    widths = o.widths.map((c, j) => positive(call, `widths[${j}]`, c));
  } else {
    widths = Array(count).fill(positive(call, 'widths', o.widths));
  }
  const rowH = o.rowH === undefined ? 1.6 * size : positive(call, 'rowH', o.rowH);
  if (o.parts !== undefined && (!Array.isArray(o.parts) || !o.parts.every((r) => r === null || Array.isArray(r)))) {
    fail(call, 'parts must be an array of arrays of part names, null for none');
  }
  const header = o.header === undefined ? true : bool(call, 'header', o.header);
  const left = (j) => x + widths.slice(0, j).reduce((a, b) => a + b, 0);
  const group = new Group([], style(call, o));
  const grid = cells.map((r, i) => r.map((v, j) => {
    const name = o.parts && o.parts[i] ? o.parts[i][j] : null;
    const cell = { fill: i === 0 && header ? true : undefined };
    if (name !== null && name !== undefined) cell.name = partName(call, name);
    const b = group.add(box(call, left(j), y + i * rowH, widths[j], rowH, cell));
    if (v !== '') {
      const t = textFrame(v, 0, size);
      const own = { size };
      if (cell.name !== undefined) own.name = cell.name;
      group.add(text(call, v, b.x + pad, b.y + (rowH - t.h) / 2, own));
    }
    return b;
  }));
  group.cell = (r, c) => {
    if (!Number.isInteger(r) || !Number.isInteger(c) || r < 0 || c < 0 || r >= grid.length || c >= grid[r].length) {
      fail('t.cell', `(${show(r)}, ${show(c)}) is not a cell of ${cells.length} rows and ${count} columns`);
    }
    return grid[r][c].bounds();
  };
  return group;
});
