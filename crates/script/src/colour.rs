//! The colour names a script may use (docs/design/figures.md, "Colour").
//! The prelude resolves a name to hex before a shape leaves the engine, so
//! the canvas file holds hex only.

/// The agent colour: every shape's default. The core's
/// `canvas::style::AGENT` is the same value; a daemon test holds them equal.
pub const AGENT: &str = "#8a5cf5";

/// Every name a script may give for a colour, and its value.
pub const COLOURS: &[(&str, &str)] = &[
    ("agent", AGENT),
    ("ink", "#000000"),
    ("muted", "#8c8c8c"),
    ("accent", "#4c9aff"),
    ("error", "#e5484d"),
    ("series1", "#2f80ed"),
    ("series2", "#d55e00"),
    ("series3", "#009e73"),
    ("series4", "#cc79a7"),
    ("series5", "#a87a00"),
    ("series6", "#178a8a"),
    // The names the packet prints for ink, so a colour read can be given back.
    ("black", "#000000"),
    ("white", "#ffffff"),
    ("red", "#ff0000"),
    ("green", "#008000"),
    ("blue", "#0000ff"),
    ("yellow", "#ffff00"),
    ("orange", "#ffa500"),
    ("purple", "#800080"),
    ("grey", "#808080"),
];
