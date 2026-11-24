
// plot.js: axes, ticks and labels; lines and dots in the series colours.

G.plot = function (x, y, w, h, opts) {
  const call = 'plot';
  number(call, 'x', x);
  number(call, 'y', y);
  positive(call, 'w', w);
  positive(call, 'h', h);
  const o = options(call, opts, [...STYLE_OPTS, 'x', 'y', 'xticks', 'yticks', 'xlabel', 'ylabel']);
  const range = (k) => {
    const r = o[k];
    if (!Array.isArray(r) || r.length !== 2 || !r.every((v) => typeof v === 'number' && Number.isFinite(v)) || r[0] === r[1]) {
      fail(call, `${k} must be [min, max] of two different numbers, got ${show(r)}`);
    }
    return r;
  };
  const [x0, x1] = range('x');
  const [y0, y1] = range('y');
  const at = (vx, vy) => [x + ((vx - x0) / (x1 - x0)) * w, y + h - ((vy - y0) / (y1 - y0)) * h];
  const ticks = (k) => {
    const t = o[k] === undefined ? [] : o[k];
    if (!Array.isArray(t) || !t.every((v) => typeof v === 'number' && Number.isFinite(v))) {
      fail(call, `${k} must be an array of numbers, got ${show(t)}`);
    }
    return t;
  };
  const scope = Object.assign({}, o);
  if (scope.size === undefined) scope.size = 16;
  const size = checkSize(call, scope.size);
  within(call, scope, () => {
    line(x, y, x, y + h);
    line(x, y + h, x + w, y + h);
    const label = { color: 'muted' };
    for (const t of ticks('xticks')) {
      const s = String(t);
      const f = textFrame(s, 0, size);
      text(at(t, y0)[0] - f.w / 2, y + h + 4, s, label);
    }
    for (const t of ticks('yticks')) {
      const s = String(t);
      const f = textFrame(s, 0, size);
      text(x - 6 - f.w, at(x0, t)[1] - f.h / 2, s, label);
    }
    if (o.xlabel !== undefined) {
      const s = words(call, o.xlabel);
      const f = textFrame(s, 0, size);
      text(x + (w - f.w) / 2, y + h + 8 + size, s);
    }
    if (o.ylabel !== undefined) text(x + 6, y - 4 - size, words(call, o.ylabel));
  });
  let series = 0;
  const next = (lopts) => {
    const own = {};
    if (lopts.color === undefined) {
      series += 1;
      own.color = `series${((series - 1) % 6) + 1}`;
    }
    return Object.assign(own, lopts);
  };
  return {
    x, y, w, h,
    at(vx, vy) {
      number('at', 'x', vx);
      number('at', 'y', vy);
      return at(vx, vy);
    },
    line(f, lopts) {
      const lcall = 'line';
      const lo = options(lcall, lopts, ['color', 'dash', 'locked', 'width', 'steps']);
      let points;
      if (typeof f === 'function') {
        const steps = lo.steps === undefined ? 50 : lo.steps;
        if (!Number.isInteger(steps) || steps < 1) fail(lcall, `steps must be a whole number above 0, got ${show(steps)}`);
        points = [];
        for (let i = 0; i <= steps; i += 1) {
          const vx = x0 + ((x1 - x0) * i) / steps;
          points.push(at(vx, number(lcall, `f(${vx})`, f(vx))));
        }
      } else if (Array.isArray(f) && f.length >= 2) {
        points = f.map((p, i) => at(...point(lcall, `points[${i}]`, p)));
      } else {
        fail(lcall, `takes a function or an array of two or more [x, y], got ${show(f)}`);
      }
      const { steps, ...rest } = lo;
      return within(call, scope, () => poly(points, next(rest)));
    },
    dots(points, dopts) {
      const dcall = 'dots';
      if (!Array.isArray(points)) fail(dcall, `points must be an array of [x, y], got ${show(points)}`);
      const own = next(options(dcall, dopts, ['color', 'locked']));
      const hex = colour(dcall, 'color', own.color);
      return within(call, scope, () => points.map((p, i) => {
        const [px, py] = at(...point(dcall, `points[${i}]`, p));
        return circle(px, py, 4, Object.assign({}, own, { fill: hex.slice(0, 7) + '66' }));
      }));
    },
  };
};
