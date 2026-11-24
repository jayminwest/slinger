
// plot.js: a plot's frame, ticks and labels in figure units, and bars,
// curves, vectors and dots in data units, in the series colours.

// A tick's label: the value, rounded as people write it.
function tickLabel(v) {
  return String(Number(v.toPrecision(6)));
}

fig.plot = entry(function (x, y, w, h, opts) {
  const call = 'fig.plot';
  number(call, 'x', x);
  number(call, 'y', y);
  positive(call, 'w', w);
  positive(call, 'h', h);
  const o = options(call, opts, ['xRange', 'yRange', 'ticks', 'frame', 'xticks', 'yticks', 'xlabel', 'ylabel', 'title', 'size', 'color', 'name', 'locked']);
  const range = (k) => {
    const r = o[k] === undefined ? [0, 1] : o[k];
    if (!Array.isArray(r) || r.length !== 2 || !r.every((v) => typeof v === 'number' && Number.isFinite(v)) || r[0] === r[1]) {
      fail(call, `${k} must be [min, max] of two different numbers, got ${show(r)}`);
    }
    return r;
  };
  const [x0, x1] = range('xRange');
  const [y0, y1] = range('yRange');
  const frame = o.frame === undefined ? 'dotted' : o.frame;
  if (!['dotted', 'dashed', 'solid', 'axes', 'none'].includes(frame)) {
    fail(call, `frame must be "dotted", "dashed", "solid", "axes" or "none", got ${show(frame)}`);
  }
  const ticks = o.ticks === undefined ? 0 : o.ticks;
  if (!Number.isInteger(ticks) || ticks < 0) fail(call, `ticks must be a whole number, 0 for none, got ${show(ticks)}`);
  const size = sizeOf(call, o, 18);
  // Where the plot's area is now: it moves with its bounds.
  const area = { x, y };
  const anchor = {
    up: null,
    own: {},
    bounds: () => ({ x: area.x, y: area.y, w, h }),
    leaves() { return [this]; },
    shift(dx, dy) { area.x += dx; area.y += dy; },
  };
  const bounds = new Group([anchor], style(call, o));
  const px = (v) => area.x + ((v - x0) / (x1 - x0)) * w;
  const py = (v) => area.y + h - ((v - y0) / (y1 - y0)) * h;
  const add = (leaf) => bounds.add(leaf);

  if (frame === 'axes') {
    add(connect(call, [x, y], [x, y + h], {}, 'none'));
    add(connect(call, [x, y + h], [x + w, y + h], {}, 'none'));
  } else if (frame !== 'none') {
    add(box(call, x, y, w, h, frame === 'solid' ? {} : { dash: frame }));
  }
  // Each tick is a value, labelled with itself, or [value, label]; `ticks`
  // spaces that many steps along both axes.
  const tickList = (k, lo, hi) => {
    if (o[k] === undefined) return ticks === 0 ? [] : Array.from({ length: ticks + 1 }, (_, i) => lo + ((hi - lo) * i) / ticks).map((v) => [v, tickLabel(v)]);
    const t = o[k];
    const finite = (v) => typeof v === 'number' && Number.isFinite(v);
    const tick = (v) => finite(v) || (Array.isArray(v) && v.length === 2 && finite(v[0]) && (typeof v[1] === 'string' || finite(v[1])));
    if (!Array.isArray(t) || !t.every(tick)) {
      fail(call, `${k} must be an array of numbers or [value, "label"] pairs, such as [[0, "Jan"], [1, "Feb"]]; got ${show(t)}`);
    }
    return t.map((v) => (Array.isArray(v) ? [v[0], String(v[1])] : [v, tickLabel(v)]));
  };
  const muted = { size, color: 'muted' };
  for (const [t, s] of tickList('xticks', x0, x1)) add(text(call, s, px(t), y + h + 4, Object.assign({ align: 'center' }, muted)));
  for (const [t, s] of tickList('yticks', y0, y1)) {
    add(text(call, s, x - 6, py(t) - textFrame(s, 0, size).h / 2, Object.assign({ align: 'right' }, muted)));
  }
  if (o.xlabel !== undefined) add(text(call, words(call, 'xlabel', o.xlabel), x + w / 2, y + h + 8 + size, { size, align: 'center' }));
  if (o.ylabel !== undefined) add(text(call, words(call, 'ylabel', o.ylabel), x + 6, y - 4 - size, { size }));
  // The title stands above the y label, half as large again as the labels.
  if (o.title !== undefined) {
    const big = Math.min(size * 1.5, 400);
    add(text(call, words(call, 'title', o.title), x, y - 4 - (o.ylabel !== undefined ? size + 4 : 0) - big, { size: big }));
  }

  // Each bars, fn and dots takes the next series colour unless given one.
  let series = 0;
  const next = (lo) => {
    if (lo.color !== undefined) return lo;
    series += 1;
    return Object.assign({ color: `series${((series - 1) % 6) + 1}` }, lo);
  };
  const data = (c, name, p) => [px(point(c, name, p)[0]), py(p[1])];
  return {
    bounds,
    x: (v) => px(number('p.x', 'v', v)),
    y: (v) => py(number('p.y', 'v', v)),
    // One bar per value across the plot, from 0 (or the range's nearer end), labels under them.
    bars: entry(function (values, bopts) {
      const c = 'p.bars';
      if (!Array.isArray(values) || values.length === 0) fail(c, `values must be an array of numbers, got ${show(values)}`);
      values.forEach((v, i) => number(c, `values[${i}]`, v));
      const bo = next(options(c, bopts, ['labels', 'color', 'fill', 'locked']));
      if (bo.labels !== undefined && (!Array.isArray(bo.labels) || bo.labels.length !== values.length)) {
        fail(c, `labels must be an array of ${values.length} strings, one per value, got ${show(bo.labels)}`);
      }
      const slot = w / values.length;
      const zero = Math.min(Math.max(0, Math.min(y0, y1)), Math.max(y0, y1));
      const own = { color: bo.color, fill: bo.fill === undefined ? true : bo.fill, locked: bo.locked };
      return values.map((v, i) => {
        const top = Math.min(py(v), py(zero));
        const tall = Math.abs(py(v) - py(zero));
        const bar = tall > 0 ? add(box(c, area.x + slot * (i + 0.2), top, slot * 0.6, tall, own)) : null;
        if (bo.labels !== undefined) add(text(c, words(c, `labels[${i}]`, bo.labels[i]), area.x + slot * (i + 0.5), area.y + h + 6, { size, align: 'center' }));
        return bar;
      });
    }),
    // A curve: f sampled across the x range (steps, default 50), or [[x, y], ...].
    fn: entry(function (f, fopts) {
      const c = 'p.fn';
      const fo = next(options(c, fopts, ['color', 'dash', 'width', 'steps', 'locked']));
      let points;
      if (typeof f === 'function') {
        const steps = fo.steps === undefined ? 50 : fo.steps;
        if (!Number.isInteger(steps) || steps < 1) fail(c, `steps must be a whole number above 0, got ${show(steps)}`);
        points = [];
        for (let i = 0; i <= steps; i += 1) {
          const vx = x0 + ((x1 - x0) * i) / steps;
          points.push([px(vx), py(number(c, `f(${vx})`, f(vx)))]);
        }
      } else if (Array.isArray(f) && f.length >= 2) {
        points = f.map((p, i) => data(c, `points[${i}]`, p));
      } else {
        fail(c, `takes a function or an array of two or more [x, y], got ${show(f)}`);
      }
      const { steps, ...rest } = fo;
      return add(stroke(c, points, rest, false));
    }),
    // An arrow in data coordinates.
    vector: entry(function (a, b, vopts) {
      const c = 'p.vector';
      const vo = options(c, vopts, ['color', 'dash', 'heads', 'label', 'size', 'locked']);
      return add(connect(c, data(c, 'from', a), data(c, 'to', b), vo, 'end'));
    }),
    // Filled dots of radius 4 at [[x, y], ...].
    dots: entry(function (points, dopts) {
      const c = 'p.dots';
      if (!Array.isArray(points)) fail(c, `points must be an array of [x, y], got ${show(points)}`);
      const own = next(options(c, dopts, ['color', 'locked']));
      const hex = colour(c, 'color', own.color);
      return points.map((p, i) => {
        const [cx, cy] = data(c, `points[${i}]`, p);
        return add(ellipse(c, cx, cy, 4, 4, Object.assign({}, own, { fill: hex.slice(0, 7) + '66' })));
      });
    }),
  };
});
