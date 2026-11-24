// The teacher's 106 worksheet, as one script.
const W = 1700, COL = 820, GAP = 60;
let y = 0;
const heading = (s, size = 32) => { const t = fig.text(s, 0, y, { size, w: W }); y = t.bottom + 12; return t; };
const para = (s) => { const t = fig.text(s, 0, y, { size: 20, w: W }); y = t.bottom + 24; return t; };
const yours = (x, w, h, label, name) => fig.box(x, y, w, h, { dash: 'dashed', label, name, locked: true });
const mine = (x, w, h, label, name) => fig.box(x, y, w, h, { dash: 'dotted', label, name });

heading('106: Logits, softmax, loss (GPT-2 small)', 48);
para('Predict first, in the dashed boxes. Then sling the page: I put GPT-2\'s actual output next to your prediction, and you write what surprised you.');

heading('Problem 1: what GPT-2 thinks comes next');
para('Prompt: "The Eiffel Tower is in the city of" ___\nGPT-2 gives every one of its 50,257 tokens a logit (a raw score). Softmax turns the logits into probabilities.');
yours(0, COL, 300, '(a) Top token and roughly its probability. Then your guesses for the next four.', 'p1a');
mine(COL + GAP, COL, 300, "GPT-2's actual top 5 (I fill this in after you predict)", 'p1a-actual');
y += 330;
yours(0, COL, 220, '(b) Surprised by the probability? Why might a model spread its bets like this?', 'p1b');
yours(COL + GAP, COL, 600, "(c) The top logit = residual vector · that token's row of W_U. In 101 terms: what is each row of W_U, and what is the logit?", 'p1c');
y += 640;

heading('Problem 2: how softmax reacts');
para('Each token gets a share proportional to e^logit; the shares add to 1. Below: three tokens with logits [2, 1, 0]. The dotted frame is 100%. Sketch the bars you predict for (a), (b), (c), then write a line on each.');
const softmax = (z) => { const e = z.map(Math.exp), s = e.reduce((a, b) => a + b); return e.map((v) => v / s); };
const cases = [
  { title: 'Given: [2, 1, 0]', z: [2, 1, 0], shown: true, name: 'given' },
  { title: '(a) add 5 to all: [7, 6, 5]', z: [7, 6, 5], name: 'p2a' },
  { title: '(b) double all: [4, 2, 0]', z: [4, 2, 0], name: 'p2b' },
  { title: '(c) top +1 only: [3, 1, 0]', z: [3, 1, 0], name: 'p2c' },
];
const PW = 380, PH = 400;
cases.forEach((c, k) => {
  const x = k * (PW + 50);
  fig.text(c.title, x, y, { size: 22 });
  const p = fig.plot(x, y + 60, PW, PH, { yRange: [0, 1] });
  if (c.shown) p.bars(softmax(c.z), { labels: ['A', 'B', 'C'] });
  else ['A', 'B', 'C'].forEach((l, j) => fig.text(l, x + (j + 0.5) * (PW / 3), y + 60 + PH + 6, { size: 18, align: 'center' }));
  p.bounds.name(c.name);
  if (!c.shown) fig.box(x, y + 60 + PH + 50, PW, 300, { dash: 'dashed', label: 'Why:', name: c.name + '-why' });
});
y += 60 + PH + 50 + 300 + 40;
para('Then: what does (a) tell you about the logits in Problem 1 all being around -92?');
yours(0, 1380, 400, '', 'p2-92'); y += 440;

heading('Problem 3: loss, or how wrong was the model?');
para('Loss for one prediction = -ln(probability the model gave the right token). Same prompt as Problem 1.');
yours(0, 1000, 600, '(a) Rank the loss if the right answer were " Paris", " London", " banana". Roughly how far apart?', 'p3a');
mine(1000 + GAP, 640, 600, 'Actual losses (I fill this in after you predict)', 'p3a-actual');
y += 640;
yours(0, W, 450, '(b) Why does the loss punish a confident wrong answer so hard?', 'p3b');
