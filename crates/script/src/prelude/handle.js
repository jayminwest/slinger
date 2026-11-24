
// handle.js: what every draw call returns. A handle is one shape (a Leaf)
// or a Group of handles. It has bounds and moves until the script ends:
// nothing is on the canvas before then, when finish (checks.js) emits every
// shape in the order it was made, its style resolved.

// Every shape, in the order made.
const leaves = [];

function isFrame(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v)
    && ['x', 'y', 'w', 'h'].every((k) => typeof v[k] === 'number' && Number.isFinite(v[k]));
}

// The frame of a handle, or of a plain {x, y, w, h}.
function frameOf(call, v) {
  if (v instanceof Handle) return v.bounds();
  if (isFrame(v)) return { x: v.x, y: v.y, w: v.w, h: v.h };
  fail(call, `other must be a handle or {x, y, w, h}, got ${show(v)}`);
}

function union(frames) {
  if (frames.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
  const x = Math.min(...frames.map((f) => f.x));
  const y = Math.min(...frames.map((f) => f.y));
  const w = Math.max(...frames.map((f) => f.x + f.w)) - x;
  const h = Math.max(...frames.map((f) => f.y + f.h)) - y;
  return { x, y, w, h };
}

class Handle {
  get x() { return this.bounds().x; }
  get y() { return this.bounds().y; }
  get w() { return this.bounds().w; }
  get h() { return this.bounds().h; }
  get right() { const b = this.bounds(); return b.x + b.w; }
  get bottom() { const b = this.bounds(); return b.y + b.h; }
  get cx() { const b = this.bounds(); return b.x + b.w / 2; }
  get cy() { const b = this.bounds(); return b.y + b.h / 2; }

  moveTo(x, y) {
    number('moveTo', 'x', x);
    number('moveTo', 'y', y);
    const b = this.bounds();
    return this.moveBy(x - b.x, y - b.y);
  }

  moveBy(dx, dy) {
    number('moveBy', 'dx', dx);
    number('moveBy', 'dy', dy);
    for (const leaf of this.leaves()) leaf.shift(dx, dy);
    return this;
  }

  below(other, gap = 24) {
    const o = frameOf('below', other);
    return this.moveBy(0, o.y + o.h + number('below', 'gap', gap) - this.y);
  }

  above(other, gap = 24) {
    const o = frameOf('above', other);
    return this.moveBy(0, o.y - number('above', 'gap', gap) - this.bottom);
  }

  rightOf(other, gap = 24) {
    const o = frameOf('rightOf', other);
    return this.moveBy(o.x + o.w + number('rightOf', 'gap', gap) - this.x, 0);
  }

  leftOf(other, gap = 24) {
    const o = frameOf('leftOf', other);
    return this.moveBy(o.x - number('leftOf', 'gap', gap) - this.right, 0);
  }

  centerIn(other) {
    const o = frameOf('centerIn', other);
    return this.moveBy(o.x + o.w / 2 - this.cx, o.y + o.h / 2 - this.cy);
  }

  // Names the part every shape in this handle is in, unless a shape names its own.
  name(part) {
    this.own.part = partName('name', part);
    return this;
  }

  toJSON() {
    return this.bounds();
  }
}

// One shape. `geo` is {x, y, w, h} for a box or a text, {pts} for an arrow
// or a stroke, in figure units. `own` is what its call set (color, fill,
// dash, locked, part); the rest comes from the handles that hold it, then
// the part it was drawn in, then fig.color as it was when it was drawn.
class Leaf extends Handle {
  constructor(call, kind, geo, own) {
    super();
    if (leaves.length >= input.items) fail(call, `a figure holds at most ${input.items} shapes`);
    this.call = call;
    this.kind = kind;
    this.geo = geo;
    this.own = own;
    this.up = null;
    this.scope = scope;
    this.base = colour(call, 'fig.color', fig.color);
    [this.line, this.col] = here();
    this.extra = [];
    this.owner = null;
    leaves.push(this);
  }

  bounds() {
    const g = this.geo;
    if (g.pts === undefined) return { x: g.x, y: g.y, w: g.w, h: g.h };
    const xs = g.pts.map((p) => p[0]);
    const ys = g.pts.map((p) => p[1]);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return { x, y, w: Math.max(...xs) - x, h: Math.max(...ys) - y };
  }

  leaves() {
    return [this];
  }

  // Moves this shape and what belongs to it (a box's label, an arrow's).
  shift(dx, dy) {
    const g = this.geo;
    if (g.pts === undefined) {
      g.x += dx;
      g.y += dy;
    } else {
      g.pts = g.pts.map(([x, y]) => [x + dx, y + dy]);
    }
    for (const e of this.extra) e.shift(dx, dy);
  }

  // A shape that belongs to this one: it moves with it and takes its style.
  attach(leaf) {
    leaf.up = this;
    leaf.owner = this;
    this.extra.push(leaf);
    return leaf;
  }

  // The value of a style key: its own, the nearest holder's, its part's.
  get_(key) {
    for (let n = this; n !== null; n = n.up) if (n.own[key] !== undefined) return n.own[key];
    if (this.scope !== null) return key === 'part' ? this.scope.name : this.scope.style[key];
    return undefined;
  }

  // The line segments it draws, for the checks.
  segments() {
    if (this.kind === 'text') return [];
    if (this.kind === 'box') {
      const { x, y, w, h } = this.geo;
      const c = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
      return c.map((p, i) => [p, c[(i + 1) % 4]]);
    }
    const p = this.geo.pts;
    const s = p.slice(1).map((q, i) => [p[i], q]);
    if (this.own.closed) s.push([p[p.length - 1], p[0]]);
    return s;
  }
}

// The root of the holders above a handle.
function root(h) {
  let r = h;
  while (r.up !== null) r = r.up;
  return r;
}

// Several handles as one: its bounds are theirs together, it moves them
// all, and its style and name reach every shape in it that sets none.
class Group extends Handle {
  constructor(members, own) {
    super();
    this.members = [];
    this.own = own;
    this.up = null;
    for (const m of members) this.add(m);
  }

  add(m) {
    this.members.push(m);
    // The outermost holder of m comes under this group, unless this group
    // is already under it.
    const r = root(m);
    if (r !== this && root(this) !== r) r.up = this;
    return m;
  }

  bounds() {
    return union(this.members.map((m) => m.bounds()));
  }

  // Every shape in it once, however many of its groups hold it.
  leaves() {
    const seen = new Set();
    for (const m of this.members) for (const l of m.leaves()) seen.add(l);
    return [...seen];
  }
}

// The handles a call was given: handles or arrays of them, flattened.
function handles(call, list) {
  const out = [];
  const take = (v) => {
    if (Array.isArray(v)) v.forEach(take);
    else if (v instanceof Handle) out.push(v);
    else if (typeof v === 'function') {
      fail(call, 'takes handles, not a function; to set defaults for what a function draws, use fig.part(name, opts, fn)');
    } else fail(call, `each argument must be a handle a draw call returned, got ${show(v)}`);
  };
  list.forEach(take);
  return out;
}
