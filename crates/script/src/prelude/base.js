// base.js: the context, errors, colours, options and the one way a shape
// leaves the engine. Part of one function body (engine.rs PRELUDE): `input`
// and `emit` are its arguments, and nothing here is global unless set on G.

const G = globalThis;

// Purity: no eval of strings, a seeded Math.random. Date, timers, console,
// modules and IO are never added to the context.
delete G.eval;
for (const f of [function () {}, function* () {}]) {
  Object.defineProperty(Object.getPrototypeOf(f), 'constructor', { value: undefined });
}
delete G.Function;
let seed = 1;
Math.random = function random() {
  // mulberry32, seeded with 1 each run.
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

class DrawError extends Error {}
DrawError.prototype.name = 'DrawError';

function fail(call, message) {
  throw new DrawError(`${call}: ${message}`);
}

function show(v) {
  if (typeof v === 'string') return JSON.stringify(v);
  if (typeof v === 'function') return 'a function';
  if (Array.isArray(v)) return 'an array';
  if (v !== null && typeof v === 'object') return 'an object';
  return String(v);
}

// The script's line and column of the call being made: the first frame of
// the stack that is in the script.
function where() {
  const m = /script:(\d+):(\d+)/.exec(new Error().stack || '');
  return m ? [Number(m[1]), Number(m[2])] : [0, 0];
}

// Where a shape was drawn: `where`, or inside a call that draws many
// shapes (table) that call's own site, since the stack a trace keeps (10
// frames) can end before the script when a cell is drawn in a part.
let here = where;

function from(fn) {
  if (here !== where) return fn();
  const site = where();
  here = () => site;
  try {
    return fn();
  } finally {
    here = where;
  }
}

function number(call, name, v) {
  if (typeof v !== 'number' || !Number.isFinite(v)) fail(call, `${name} must be a number, got ${show(v)}`);
  return v;
}

function positive(call, name, v) {
  if (typeof v !== 'number' || !Number.isFinite(v) || v <= 0) {
    fail(call, `${name} must be a number above 0, got ${show(v)}`);
  }
  return v;
}

function callback(call, fn) {
  if (typeof fn !== 'function') fail(call, `the last argument must be a function, got ${show(fn)}`);
  return fn;
}

// Colours: a name, #rrggbb or #rrggbbaa, out as lower-case hex (#rrggbb when opaque).
const NAMES = input.colours;
const NAME_LIST = Object.keys(NAMES).join(', ');

function colour(call, key, v) {
  if (typeof v === 'string') {
    if (Object.prototype.hasOwnProperty.call(NAMES, v)) return NAMES[v];
    if (/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(v)) {
      const hex = v.toLowerCase();
      return hex.length === 9 && hex.endsWith('ff') ? hex.slice(0, 7) : hex;
    }
  }
  fail(call, `${key} must be "#rrggbb", "#rrggbbaa" or one of ${NAME_LIST}; got ${show(v)}`);
}

// A fill: true is a tint of the shape's colour, a name or #rrggbb a 20% tint, #rrggbbaa as given.
function tint(call, v, own) {
  if (v === true) return own.slice(0, 7) + '33';
  const hex = colour(call, 'fill', v);
  return hex.length === 7 ? hex + '33' : hex;
}

// Defaults that nest: each part, group, stack, table and plot pushes its
// options; the nearest that sets a key wins, then fig.color, then the built-in.
const STYLE = ['color', 'fill', 'dash', 'size', 'locked'];
const scopes = [];
let part = null;

function options(call, opts, allowed) {
  if (opts === undefined || opts === null) return {};
  if (typeof opts !== 'object' || Array.isArray(opts)) fail(call, `options must be an object, got ${show(opts)}`);
  for (const k of Object.keys(opts)) {
    if (!allowed.includes(k)) fail(call, `unknown option ${k}; ${call} takes ${allowed.join(', ')}`);
  }
  return opts;
}

function within(call, opts, fn) {
  const style = {};
  for (const k of STYLE) if (opts[k] !== undefined) style[k] = opts[k];
  if (style.color !== undefined) colour(call, 'color', style.color);
  if (style.dash !== undefined) dashed(call, style.dash);
  if (style.size !== undefined) checkSize(call, style.size);
  if (style.locked !== undefined && typeof style.locked !== 'boolean') {
    fail(call, `locked must be true or false, got ${show(style.locked)}`);
  }
  scopes.push(style);
  try {
    return fn();
  } finally {
    scopes.pop();
  }
}

// The value of `key` for a shape: its own option, else the nearest scope's.
function inherited(opts, key) {
  if (opts[key] !== undefined) return opts[key];
  for (let i = scopes.length - 1; i >= 0; i -= 1) if (scopes[i][key] !== undefined) return scopes[i][key];
  return undefined;
}

function dashed(call, v) {
  if (v !== 'dashed' && v !== 'dotted' && v !== 'solid') {
    fail(call, `dash must be "dashed", "dotted" or "solid", got ${show(v)}`);
  }
  return v;
}

function checkSize(call, v) {
  if (typeof v !== 'number' || !(v >= 4 && v <= 400)) fail(call, `size must be a number from 4 to 400, got ${show(v)}`);
  return v;
}

// The style a shape of `kind` gets. A shape's own fill or dash that its kind
// cannot take fails; an inherited one is left off.
function styled(call, opts, kind) {
  const own = opts.color !== undefined ? colour(call, 'color', opts.color) : undefined;
  const near = inherited(opts, 'color');
  const color = own || (near !== undefined ? colour(call, 'color', near) : colour(call, 'fig.color', fig.color));
  const style = { color, locked: true };
  const locked = inherited(opts, 'locked');
  if (locked !== undefined) {
    if (typeof locked !== 'boolean') fail(call, `locked must be true or false, got ${show(locked)}`);
    style.locked = locked;
  }
  const fills = kind === 'box' || kind === 'closed';
  if (opts.fill !== undefined && opts.fill !== false && opts.fill !== null && !fills) {
    fail(call, 'fill takes box, circle and poly with closed: true');
  }
  const fill = inherited(opts, 'fill');
  if (fills && fill !== undefined && fill !== false && fill !== null) style.fill = tint(call, fill, color);
  if (opts.dash !== undefined && kind === 'text') fail(call, 'dash takes box, line, arrow, poly and circle');
  const dash = inherited(opts, 'dash');
  if (kind !== 'text' && dash !== undefined && dashed(call, dash) !== 'solid') style.dash = dash;
  return style;
}

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

// Every shape the script drew, for the checks after it: kind, frame, segments, call site.
const drawn = [];

// Emits one shape: `fields` are its kind's, in tenths; `frame` and
// `segments` in units for the checks.
function put(call, item, style, frame, segments) {
  if (drawn.length >= input.items) fail(call, `a figure holds at most ${input.items} shapes`);
  if (style.fill !== undefined) item.fill = style.fill;
  if (style.dash !== undefined) item.dash = style.dash;
  if (style.locked) item.locked = true;
  if (input.name) item.fig = input.name;
  if (part !== null) item.part = part;
  const [line, col] = here();
  drawn.push({ kind: item.kind, frame, segments, line, col, text: item.text, fill: item.fill !== undefined });
  emit(JSON.stringify(item));
  return { x: frame.x, y: frame.y, w: frame.w, h: frame.h };
}

function warning(line, col, message) {
  emit(JSON.stringify({ warning: message, line, col }));
}

const fig = {
  name: input.name,
  data: input.data,
  view: Object.freeze({
    target: input.target === null ? null : Object.freeze({ x: 0, y: 0, w: input.target[2], h: input.target[3] }),
    page: input.page,
  }),
  color: input.agent,
};
Object.defineProperty(G, 'fig', { value: fig, enumerable: true });

G.warn = function warn(message) {
  const [line, col] = where();
  warning(line, col, String(message));
};
