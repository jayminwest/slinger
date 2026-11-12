//! `slinger canvas schema`: the item kinds and operations `canvas apply` takes,
//! for an agent that cannot read docs/service.md. Static text, so it needs no
//! daemon; `canvas apply --help` shows the same table.

use serde_json::{Value, json};

use crate::output::Output;

/// The schema, one entry per line: section, name and fields, by tabs. The
/// source of truth is docs/service.md ("Canvas operations") and
/// docs/design/canvas-file.md ("Item objects"); keep this in step with them.
pub const SCHEMA: &str = "\
unit\tlength\tpositions and lengths are integers in tenths of a unit
unit\tpts\ta flat list of x,y pairs: the first pair is the start, each next pair the step from the one before
item\tany\tid (i<n>), page (p<n>), kind, color (#rrggbb or #rrggbbaa, default #000000; not on live or button), then the kind's fields; 'canvas show' adds by and t
kind\ttext\tx, y, text; optional w, h (the frame; missing, it fits the words); optional size (one character, 40 to 4000; missing or null, 240; as 'canvas put' and 'canvas edit' --size take it)
kind\tbox\tx, y, w, h
kind\tarrow\tpts (two or more pairs); optional from, to (the item ids it points at)
kind\tstroke\twidth, pts (one or more pairs)
kind\tbutton\tx, y, w, h (above 0): the send-to-agent button
kind\tlive\tx, y, w, h, ui, hash, open: an agent UI, made by 'slinger ui open --as item'
op\tadd\titem: an item without id; an arrow's from or to may be $n: the item made[n] names, from an add before it in this apply
op\tremove\tids: a list of item ids; ids already gone are fine
op\tmove\tids, dx, dy
op\tset\tid, fields: item fields laid over the item's current ones
op\tpage.add\tname; the page goes last
op\tpage.rename\tpage, name
op\tpage.remove\tpage; its items go with it, the last page stays";

/// The schema as text, and for `--json` as `units`, `item`, `kinds` and `ops`.
pub fn schema() -> Output {
    let mut data = json!({ "units": {}, "item": "", "kinds": {}, "ops": {} });
    for line in SCHEMA.lines() {
        let mut parts = line.splitn(3, '\t');
        let (section, name, fields) = (parts.next(), parts.next(), parts.next());
        let (Some(section), Some(name), Some(fields)) = (section, name, fields) else { continue };
        let fields = Value::from(fields);
        match section {
            "unit" => data["units"][name] = fields,
            "item" => data["item"] = fields,
            "kind" => data["kinds"][name] = fields,
            _ => data["ops"][name] = fields,
        }
    }
    Output::new(data, SCHEMA.to_string())
}
