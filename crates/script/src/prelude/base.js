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

// Throws at the call being made: its site leads the stack, so the error
// names the script's line even when the call ran deep in the prelude.
function fail(call, message) {
  const e = new DrawError(`${call}: ${message}`);
  const [line, col] = here();
  const site = `(script:${line}:${col})`;
  if (line > 0 && !(e.stack || '').includes(site)) e.stack = `    at ${call} ${site}\n${e.stack || ''}`;
  throw e;
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

// Where the call being made is: `where`, or, inside a call the script made
// to the API, that call's site, read once as it starts (the stack a trace
// keeps, 10 frames, can end before the script deep in a plot or a table).
let here = where;

// The API function `fn` as the script calls it: its site is read as it
// starts. The prelude calls `fn` itself, never the entry, so a site is the
// script's; a callback of the script's that calls the API reads its own.
function entry(fn) {
  return function (...args) {
    const before = here;
    const site = where();
    here = () => site;
    try {
      return fn.apply(this, args);
    } finally {
      here = before;
    }
  };
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

function bool(call, key, v) {
  if (typeof v !== 'boolean') fail(call, `${key} must be true or false, got ${show(v)}`);
  return v;
}

const NAME = /^[a-z0-9-]{1,40}$/;

function partName(call, name) {
  if (typeof name !== 'string' || !NAME.test(name)) {
    fail(call, `a part name is 1 to 40 of a-z, 0-9 and -, got ${show(name)}`);
  }
  return name;
}

function options(call, opts, allowed) {
  if (opts === undefined || opts === null) return {};
  if (typeof opts !== 'object' || Array.isArray(opts) || opts instanceof Handle) {
    fail(call, `options must be an object, got ${show(opts)}`);
  }
  for (const k of Object.keys(opts)) {
    if (!allowed.includes(k)) fail(call, `unknown option ${k}; ${call} takes ${allowed.join(', ')}`);
  }
  return opts;
}

// The style options a shape, a group or a part may set, checked as given;
// `name` comes back as `part`. Fill and dash are checked again for the
// kind when the shape is made.
const STYLE = ['color', 'fill', 'dash', 'locked'];

function style(call, o) {
  const s = {};
  if (o.color !== undefined) s.color = colour(call, 'color', o.color);
  if (o.fill !== undefined && o.fill !== null && o.fill !== false) {
    if (o.fill !== true) colour(call, 'fill', o.fill);
    s.fill = o.fill;
  }
  if (o.dash !== undefined) s.dash = dashed(call, o.dash);
  if (o.locked !== undefined) s.locked = bool(call, 'locked', o.locked);
  if (o.name !== undefined) s.part = partName(call, o.name);
  return s;
}

// The part a function passed to fig.part draws in: its name and defaults.
let scope = null;

const fig = {
  name: input.name,
  view: Object.freeze({
    target: input.target === null ? null : Object.freeze({ x: 0, y: 0, w: input.target[2], h: input.target[3] }),
    page: input.page,
  }),
  color: input.agent,
};
Object.defineProperty(G, 'fig', { value: fig, enumerable: true });
Object.defineProperty(G, 'data', { value: input.data, enumerable: true });

function warning(line, col, message) {
  emit(JSON.stringify({ warning: message, line, col }));
}

G.warn = function warn(message) {
  const [line, col] = where();
  warning(line, col, String(message));
};
