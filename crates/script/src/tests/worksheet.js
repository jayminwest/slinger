const d = data;
const pct = Math.round(d.rate * 100);
const value = (t) => d.start * (1 + d.rate) ** t;

fig.part('title', () => {
  fig.text(d.title, 0, 0, { size: 40 });
  fig.text('Work in the dashed boxes. The dotted ones are mine.', 0, 56, { color: 'muted' });
});
fig.part('name', () => {
  fig.text('Name', 620, 8, { color: 'muted' });
  fig.line(690, 32, 900, 32);
});

// 1. Read a graph.
fig.text(`1. ${d.start} is saved at ${pct}% a year, added once a year.`, 0, 120, { name: 'q1' });
const p = fig.plot(60, 190, 480, 280, {
  xRange: [0, d.years], yRange: [0, 250], frame: 'axes',
  xticks: [0, 2, 4, 6, 8, 10], yticks: [0, 100, 200],
  xlabel: 'years', ylabel: 'balance', name: 'q1-plot',
});
p.fn(value, { steps: 40 });
fig.text('1a. About how much after 5 years?', 600, 190, { size: 20, w: 300 });
fig.slot(600, 250, 300, 90, { name: '1a-answer' });
fig.text('1b. When has it doubled?', 600, 360, { size: 20, w: 300 });
fig.slot(600, 400, 300, 90, { name: '1b-answer' });

// 2. Fill the table.
fig.text('2. Fill in the balance.', 0, 540, { name: 'q2' });
fig.table([
  ['year', ...d.table.map(String)],
  ['balance', value(d.table[0]).toFixed(0), '', '', ''],
], 0, 580, { widths: 150, size: 24, name: 'q2-table' });
fig.slot(0, 680, 900, 120, { name: '2-working' });

// 3. Predict, then compare.
fig.text(`3. Rank these by balance after ${d.years} years.`, 0, 840, { name: 'q3' });
fig.stack(d.accounts.map((a) => fig.text(`${a.name}: ${a.start} at ${a.rate}%`, 0, 880, { size: 20 })), { gap: 4 })
  .name('3-accounts');
fig.slot(0, 1010, 430, 200, { name: '3-predicted' });
fig.arrow(436, 1110, 464, 1110);
fig.text('Actual top 5', 470, 1010, { color: 'muted', size: 20 });
fig.part('actual-top-5', () => {
  fig.slot(470, 1040, 430, 170, { by: 'agent' });
  if (d.actual) fig.stack(d.actual.map((n, i) => fig.text(`${i + 1}. ${n}`, 486, 1052, { size: 20 })), { gap: 8 });
});

// Feedback, written on a later turn.
fig.part('feedback', () => {
  fig.slot(0, 1250, 900, 140, { by: 'agent' });
  if (d.feedback) fig.text(d.feedback, 16, 1266, { w: 868 });
});
fig.circle(880, 1230, 8, { fill: d.feedback ? 'series3' : undefined, name: 'check' });
