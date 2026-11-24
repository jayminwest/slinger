const d = fig.data;
const pct = Math.round(d.rate * 100);
const value = (t) => d.start * (1 + d.rate) ** t;

part('title', () => {
  text(0, 0, d.title, { size: 40 });
  text(0, 56, 'Work in the dashed boxes. The dotted ones are mine.', { color: 'muted' });
});
part('name', () => {
  text(620, 8, 'Name', { color: 'muted' });
  line(690, 32, 900, 32);
});

// 1. Read a graph.
part('q1', () => text(0, 120, `1. ${d.start} is saved at ${pct}% a year, added once a year.`));
part('q1-plot', () => {
  const p = plot(60, 190, 480, 280, {
    x: [0, d.years], y: [0, 250],
    xticks: [0, 2, 4, 6, 8, 10], yticks: [0, 100, 200],
    xlabel: 'years', ylabel: 'balance',
  });
  p.line(value, { steps: 40 });
});
text(600, 190, '1a. About how much after 5 years?', { size: 20, w: 300 });
part('1a-answer', () => box(600, 250, 300, 90, { dash: 'dashed' }));
text(600, 360, '1b. When has it doubled?', { size: 20, w: 300 });
part('1b-answer', () => box(600, 400, 300, 90, { dash: 'dashed' }));

// 2. Fill the table.
part('q2', () => text(0, 540, '2. Fill in the balance.'));
part('q2-table', () => table(0, 580, [
  ['year', ...d.table.map(String)],
  ['balance', value(d.table[0]).toFixed(0), '', '', ''],
], { cols: 150, header: true }));
part('2-working', () => box(0, 680, 900, 120, { dash: 'dashed' }));

// 3. Predict, then compare.
part('q3', () => text(0, 840, `3. Rank these by balance after ${d.years} years.`));
part('3-accounts', () => stack(0, 880, { gap: 4 }, (s) => {
  for (const a of d.accounts) s.text(`${a.name}: ${a.start} at ${a.rate}%`, { size: 20 });
}));
part('3-predicted', () => box(0, 1010, 430, 200, { dash: 'dashed' }));
arrow(436, 1110, 464, 1110);
text(470, 1010, 'Actual top 5', { color: 'muted', size: 20 });
part('actual-top-5', () => {
  box(470, 1040, 430, 170, { dash: 'dotted' });
  if (d.actual) stack(486, 1052, {}, (s) => d.actual.forEach((n, i) => s.text(`${i + 1}. ${n}`, { size: 20 })));
});

// Feedback, written on a later turn.
part('feedback', () => {
  box(0, 1250, 900, 140, { dash: 'dotted' });
  if (d.feedback) text(16, 1266, d.feedback, { w: 868 });
});
part('check', () => circle(880, 1230, 8, { fill: d.feedback ? 'series3' : undefined }));
