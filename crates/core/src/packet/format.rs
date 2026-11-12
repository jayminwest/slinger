//! A whole packet: the header, the three sections and the images. Writes the
//! text of `docs/design/packet.md` and reads it back; owns the header grammar
//! and the version check (B5).

use std::fmt::Write as _;

use super::line::{Entry, push_value, unvalue};
use crate::render::Image;
use crate::{Error, Result};

/// The packet format's version. Within it, changes only add.
pub const VERSION: u64 = 1;

/// The first word of every packet.
const MAGIC: &str = "slinger-packet";

/// The sections, in the order they are written.
const SECTIONS: [&str; 3] = ["new", "made", "page"];

/// Characters per text token, and pixels per image token, for [`Packet::tokens`].
const CHARS_PER_TOKEN: u64 = 4;
const PIXELS_PER_TOKEN: u64 = 750;
/// The most one image costs: a larger one is scaled down before it is read.
const MAX_IMAGE_TOKENS: u64 = 1600;

/// Line 1 of a packet, after the version.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The canvas id.
    pub canvas: String,
    /// The page's name.
    pub page: String,
    /// The canvas ops the packet covers: `from..=to`. Nothing new when `from > to`.
    pub from: u64,
    pub to: u64,
    /// Pixels per canvas unit in the images.
    pub scale: f64,
    /// About how many tokens the packet costs a model, its images included:
    /// [`Packet::tokens`], set by [`Packet::count`]. `None` writes no field.
    pub tokens: Option<u64>,
    /// The directory the images were written to, when they were.
    pub at: Option<String>,
}

/// What an agent reads from a canvas page instead of a screenshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub header: Header,
    /// Ink clusters with a stroke in `from..=to`, in reading order.
    pub new: Vec<Entry>,
    /// Items an agent made or changed in `from..=to`, in reading order.
    pub made: Vec<Entry>,
    /// The rest of the page, in reading order.
    pub page: Vec<Entry>,
    /// The images the `img` fields in `new` name, in the order named. Not
    /// part of [`Packet::text`]; a packet read back has none.
    pub images: Vec<Image>,
    /// The packet ids of the selection it was built with (`c42` for any
    /// stroke of that cluster), in the order selected, each once. Not part
    /// of [`Packet::text`]; a packet read back has none.
    pub selected: Vec<String>,
}

impl Packet {
    /// The packet's text, in the designed format.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = Self::header_line(&self.header);
        for (name, entries) in SECTIONS.into_iter().zip([&self.new, &self.made, &self.page]) {
            let _ = writeln!(out, "# {name}");
            for e in entries {
                e.write(&mut out);
            }
        }
        out
    }

    /// Line 1, with its line end.
    pub(super) fn header_line(h: &Header) -> String {
        let mut out = format!("{MAGIC} {VERSION}");
        let mut field = |key: &str, value: &str| {
            let _ = write!(out, " {key}=");
            push_value(&mut out, value);
        };
        field("canvas", &h.canvas);
        field("page", &h.page);
        field("from", &h.from.to_string());
        field("to", &h.to.to_string());
        field("scale", &h.scale.to_string());
        if let Some(tokens) = h.tokens {
            field("tokens", &tokens.to_string());
        }
        if let Some(at) = &h.at {
            field("at", at);
        }
        out.push('\n');
        out
    }

    /// Reads a packet's text. Unknown header fields and unknown sections are
    /// skipped; unknown kinds and fields are kept on their entries.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedVersion`] when the version is not [`VERSION`].
    /// - [`Error::Invalid`] when the text is not a packet: a bad header, a
    ///   missing section, or a line that breaks the grammar.
    pub fn read(text: &str) -> Result<Self> {
        let text = text.strip_suffix('\n').unwrap_or(text);
        let mut lines = text.split('\n');
        let header = header(lines.next().unwrap_or_default())?;
        let mut sections: [Option<Vec<Entry>>; 3] = [None, None, None];
        let mut current: Option<usize> = None;
        let mut unknown = false;
        for (i, line) in lines.enumerate() {
            if let Some(name) = line.strip_prefix("# ") {
                current = SECTIONS.iter().position(|s| *s == name);
                unknown = current.is_none();
                if let Some(k) = current {
                    sections[k].get_or_insert_with(Vec::new);
                }
                continue;
            }
            let Some(k) = current else {
                if unknown {
                    continue;
                }
                return Err(invalid(format!("line {} is before any section", i + 2)));
            };
            let entry = Entry::parse(line)
                .ok_or_else(|| invalid(format!("line {} is not an item line", i + 2)))?;
            sections[k].get_or_insert_with(Vec::new).push(entry);
        }
        let [new, made, page] = sections;
        let missing = |name: &str| invalid(format!("the packet has no `# {name}` section"));
        Ok(Self {
            header,
            new: new.ok_or_else(|| missing("new"))?,
            made: made.ok_or_else(|| missing("made"))?,
            page: page.ok_or_else(|| missing("page"))?,
            images: Vec::new(),
            selected: Vec::new(),
        })
    }

    /// How many strokes of the user's ink are new: the `strokes` of each
    /// `new` line.
    #[must_use]
    pub fn new_strokes(&self) -> u64 {
        let strokes = self.new.iter().filter_map(|e| e.field("strokes")?.parse::<u64>().ok());
        strokes.sum()
    }

    /// About how many tokens a model spends reading the packet: a token per
    /// four characters of text, and a token per 750 pixels of each image, at
    /// most 1600 an image. A local estimate, not a tokenizer's count: no
    /// model is asked. Tests use it to hold the packet's cost down.
    #[must_use]
    pub fn tokens(&self) -> u64 {
        Self::estimate(self.text().chars().count() as u64, &self.images)
    }

    /// Writes [`Packet::tokens`] into the header's `tokens` field, counting
    /// the field itself. Call it again after changing the header or the
    /// sections.
    pub fn count(&mut self) {
        self.header.tokens = Some(0);
        let base = self.text().chars().count() as u64 - 1;
        // The field's own digits add to the text; more tokens never mean
        // fewer digits, so this climbs to a fixed point in a step or two.
        let mut tokens: u64 = 0;
        loop {
            let digits = tokens.to_string().len() as u64;
            let next = Self::estimate(base + digits, &self.images);
            if next == tokens {
                break;
            }
            tokens = next;
        }
        self.header.tokens = Some(tokens);
    }

    /// About how many tokens `chars` characters of text and `images` cost:
    /// the rule of [`Packet::tokens`], for whatever holds a packet.
    pub(crate) fn estimate<'a>(chars: u64, images: impl IntoIterator<Item = &'a Image>) -> u64 {
        let image = |i: &Image| {
            (u64::from(i.width) * u64::from(i.height))
                .div_ceil(PIXELS_PER_TOKEN)
                .min(MAX_IMAGE_TOKENS)
        };
        chars.div_ceil(CHARS_PER_TOKEN) + images.into_iter().map(image).sum::<u64>()
    }
}

/// Reads line 1.
fn header(line: &str) -> Result<Header> {
    let mut words = line.split(' ');
    if words.next() != Some(MAGIC) {
        return Err(invalid(format!("the text does not start with `{MAGIC}`")));
    }
    let version = words.next().unwrap_or_default();
    if version.parse::<u64>().ok() != Some(VERSION) {
        return Err(Error::UnsupportedVersion { version: version.to_string() });
    }
    let (mut canvas, mut page, mut from, mut to, mut scale, mut tokens, mut at) =
        (None, None, None, None, None, None, None);
    for token in words {
        let bad = || invalid(format!("the header field `{token}` cannot be read"));
        let (key, value) = token.split_once('=').ok_or_else(bad)?;
        let value = unvalue(value).ok_or_else(bad)?;
        match key {
            "canvas" => canvas = Some(value),
            "page" => page = Some(value),
            "from" => from = Some(value.parse().map_err(|_| bad())?),
            "to" => to = Some(value.parse().map_err(|_| bad())?),
            "scale" => scale = Some(value.parse().map_err(|_| bad())?),
            "tokens" => tokens = Some(value.parse().map_err(|_| bad())?),
            "at" => at = Some(value),
            _ => {}
        }
    }
    let missing = |key: &str| invalid(format!("the header has no `{key}`"));
    Ok(Header {
        canvas: canvas.ok_or_else(|| missing("canvas"))?,
        page: page.ok_or_else(|| missing("page"))?,
        from: from.ok_or_else(|| missing("from"))?,
        to: to.ok_or_else(|| missing("to"))?,
        scale: scale.ok_or_else(|| missing("scale"))?,
        tokens,
        at,
    })
}

fn invalid(message: String) -> Error {
    Error::Invalid {
        message,
        hint: Some("the format is docs/design/packet.md; read the packet again".into()),
    }
}
