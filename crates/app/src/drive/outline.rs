//! The scene as text an agent reads in place of the picture: one line for
//! each thing the window shows, with its place in the window's points from
//! the top left (a canvas pane's marks: in the pane's). Nothing here
//! decides what shows; it only says it.

use std::fmt::Write;

use crate::commands::context::Popup;
use crate::commands::menu::Action;
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::nvim::grid::Grid;
use crate::scene::canvas::{Dash, Draw, Fill, Mark};
use crate::scene::{Label, Scene};

/// The most of a text one line says.
const MOST: usize = 120;

/// What `scene` shows, a line each.
pub fn outline(scene: &Scene) -> String {
    let mut out = String::new();
    let mut say = |line: String| {
        // Writing to a string cannot fail.
        let _ = writeln!(out, "{line}");
    };
    let size = scene.size;
    let title = &scene.title;
    say(format!(
        "window {}x{} {} {}",
        n(size.width),
        n(size.height),
        quoted(&title.text),
        quoted(&title.subtitle)
    ));
    say(format!("keyboard {:?}", scene.keyboard));
    if scene.focus.width > 0.0 {
        say(format!("focus {}", rect(scene.focus.frame)));
    }
    for row in scene.sidebar.iter().filter(|row| !row.label.text.is_empty()) {
        say(label("sidebar", &row.label));
    }
    for pane in &scene.canvases {
        let zoom = pane.scene.viewport.zoom();
        let cursor = format!("{:?}", pane.cursor);
        let cursor = cursor.split([' ', '{']).next().unwrap_or_default();
        say(format!(
            "canvas {} {} zoom {} cursor {cursor}",
            pane.pane.number(),
            rect(pane.frame),
            n(zoom)
        ));
        for draw in &pane.scene.draws {
            say(format!("  {}", mark(draw)));
        }
    }
    for web in &scene.webs {
        let code = &web.page.code;
        let host = if web.live.is_some() { "live" } else { "pane" };
        say(format!("web {}@{} {host} {}", code.name, code.version, rect(web.frame)));
    }
    for footer in &scene.footers {
        let part = |p: &crate::scene::status::Part| {
            p.runs.iter().map(|run| run.text.as_str()).collect::<String>()
        };
        let parts: Vec<String> = footer.parts.iter().map(part).collect();
        say(format!("footer {} {}", rect(footer.frame), quoted(&parts.join(" | "))));
        if let Some(tip) = &footer.tip {
            say(label("  tip", &tip.label));
        }
    }
    for line in scene.labels.iter().filter(|line| !line.text.is_empty()) {
        say(label("label", line));
    }
    for pill in &scene.pills {
        say(label("pill", &pill.label));
    }
    if let Some(entry) = &scene.entry {
        say(format!(
            "entry {} {} placeholder {} title {} keys {}",
            rect(entry.frame),
            quoted(&entry.text),
            quoted(&entry.placeholder),
            quoted(&entry.title.label.text),
            quoted(&entry.keys.label.text)
        ));
    }
    if let Some(overlay) = &scene.overlay {
        say(format!("overlay {}", rect(overlay.frame)));
        for line in overlay.lines.iter().filter(|line| !line.label.text.is_empty()) {
            say(label("  line", &line.label));
        }
    }
    if let Some(flash) = &scene.flash {
        say(label("flash", &flash.label));
    }
    out
}

/// The context menu a driven window would pop ([`crate::commands::context`]),
/// a line each: `menu x,y`, then each item's title in quotes, its key and
/// `grey` where it cannot act; a separator is `---`. `drive pick` chooses
/// an item by its title.
pub fn menu(popup: &Popup) -> String {
    let mut out = format!("menu {}\n", point(popup.at));
    for item in &popup.items {
        let line = if item.action == Action::Separator {
            "---".to_string()
        } else {
            let key = item.key.map(|k| format!(" {}", k.glyphs())).unwrap_or_default();
            let grey = if item.enabled { "" } else { " grey" };
            format!("{}{key}{grey}", quoted(item.title))
        };
        // Writing to a string cannot fail.
        let _ = writeln!(out, "  {line}");
    }
    out
}

/// One mark of a canvas pane: its item, its kind, the box its ink covers
/// and what it holds.
fn mark(draw: &Draw) -> String {
    let what = match &draw.mark {
        Mark::Stroke { curves, width, dash, fill, .. } => {
            format!("stroke {} curves width {}{}", curves.len(), n(*width), styled(*dash, *fill))
        }
        Mark::Text { text, size, width, .. } => {
            format!("text size {} column {} {}", n(*size), n(*width), quoted(text))
        }
        Mark::Box { width, dash, fill, .. } => {
            format!("box width {}{}", n(*width), styled(*dash, *fill))
        }
        Mark::Arrow { from, to, heads, dash, .. } => {
            let heads = match heads.len() {
                0 => " heads none",
                1 => "",
                _ => " heads both",
            };
            format!("arrow {} to {}{heads}{}", point(*from), point(*to), styled(*dash, None))
        }
        Mark::Live { name, open, .. } => format!("live {name} {open}"),
        Mark::Button { label, .. } => format!("button {}", quoted(label)),
        Mark::Pdf { page, file, .. } => {
            format!("pdf page {page}{}", if file.is_some() { "" } else { " (not come)" })
        }
        Mark::Panel { fill, dashed, .. } => {
            let edge = if *dashed { " dashed" } else { "" };
            format!("panel{edge}{}", if fill.is_some() { " filled" } else { "" })
        }
        Mark::Caret { .. } => "caret".to_string(),
        Mark::Cells { grid, .. } => cells(grid),
    };
    // A mark that is no item of the page (a caret, a frame, a handle) has an id past any item's.
    let id = if draw.id.0 > u64::MAX / 2 { "~".to_string() } else { format!("i{}", draw.id.0) };
    format!("{id} {} {} {what}", rect(draw.bounds), colour(draw.colour))
}

/// A line's dash and its fill as drawn, each only when it has one:
/// ` dashed` or ` dotted`, then ` filled #rrggbbaa` (the alpha painted).
fn styled(dash: Dash, fill: Option<Fill>) -> String {
    let dash = match dash {
        Dash::Solid => "",
        Dash::Dashed => " dashed",
        Dash::Dotted => " dotted",
    };
    let fill = fill.map(|f| {
        // The alpha is from 0 to 1, from a byte.
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "0 to 255")]
        let alpha = (f.alpha * 255.0).round() as u8;
        format!(" filled {}{alpha:02x}", colour(f.colour))
    });
    format!("{dash}{}", fill.unwrap_or_default())
}

/// Nvim's grid over the text it edits: its size, mode and cursor (row,
/// column, from 0), then each row as it shows, after a `|`, its blanks cut.
fn cells(grid: &Grid) -> String {
    let (cols, rows, (row, col)) = (grid.cols, grid.rows, grid.cursor);
    let mut said = format!("cells {cols}x{rows} mode {} cursor {row},{col}", grid.mode_name);
    for row in 0..rows {
        // Writing to a string cannot fail.
        let _ = write!(said, "\n      |{}", grid.row_text(row));
    }
    said
}

/// A line of text at its place: `kind x,y wxh #colour "text"`.
fn label(kind: &str, label: &Label) -> String {
    let fill = label.fill.map(|fill| format!(" on {}", colour(fill))).unwrap_or_default();
    format!("{kind} {} {}{fill} {}", rect(label.frame), colour(label.colour), quoted(&label.text))
}

/// `text` in quotes on one line, cut to [`MOST`] characters.
fn quoted(text: &str) -> String {
    let cut: String = text.chars().take(MOST).collect();
    if cut.len() < text.len() { format!("{cut:?}…") } else { format!("{cut:?}") }
}

fn colour(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn rect(r: Rect) -> String {
    format!("{},{} {}x{}", n(r.x), n(r.y), n(r.width), n(r.height))
}

fn point(p: Point) -> String {
    format!("{},{}", n(p.x), n(p.y))
}

/// A number with at most one decimal, and none when it is whole.
fn n(value: f64) -> String {
    let tenths = (value * 10.0).round() / 10.0;
    if tenths.fract() == 0.0 { format!("{tenths:.0}") } else { format!("{tenths:.1}") }
}
