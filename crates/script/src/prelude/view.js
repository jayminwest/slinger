// view.js: what the script may read of the canvas where its figure goes
// (docs/design/figures.md, "The script API (v1)", rule 5), as two
// read-only globals:
//   anchor  the item it goes beside or over: {id, kind, w, h, text}
//           (text is null but for a text), or null
//   around  with --over or --at, the items near where it goes, nearest
//           first, in figure coordinates: [{id, kind, x, y, w, h}]; else []
{
  const frozen = (v) => Object.freeze(v);
  for (const [key, value] of [
    ['anchor', input.anchor ? frozen({ ...input.anchor }) : null],
    ['around', frozen((input.around || []).map((n) => frozen({ ...n })))],
  ]) {
    Object.defineProperty(globalThis, key, { value, writable: false, enumerable: false, configurable: false });
  }
}
