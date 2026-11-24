//! The one theme (SPEC P6): the window's colours, each a `[theme]` key.

use super::Rgb;

/// The one theme (SPEC P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub error: Rgb,
    /// The accent: the focus border (only with two or more panes), the caret,
    /// the selection, the sling field's edge and a working agent's mark.
    pub focus: Rgb,
    /// A done agent's mark: not to be read as working or blocked.
    pub done: Rgb,
    /// The agent colour (docs/design/figures.md, "Colour"): ink and fills
    /// stored in the agent colour (`scene::canvas::style::AGENT`) draw in
    /// this one. Nothing is written: another value recolours what shows.
    pub agent: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: rgb(0x1e, 0x1e, 0x1e),
            text: rgb(0xe6, 0xe6, 0xe6),
            muted: rgb(0x8c, 0x8c, 0x8c),
            error: rgb(0xff, 0x6b, 0x6b),
            focus: rgb(0x4c, 0x9a, 0xff),
            done: rgb(0x4c, 0xc3, 0x6b),
            agent: crate::scene::canvas::style::AGENT,
        }
    }
}

/// The `[theme]` keys one config file sets. Unset keys fall through to the
/// layer below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Colours {
    pub(super) background: Option<Rgb>,
    pub(super) text: Option<Rgb>,
    pub(super) muted: Option<Rgb>,
    pub(super) error: Option<Rgb>,
    pub(super) focus: Option<Rgb>,
    pub(super) done: Option<Rgb>,
    pub(super) agent: Option<Rgb>,
}

impl Colours {
    /// `top` merged over `self`: each key `top` sets wins.
    pub(super) fn over(self, top: Colours) -> Colours {
        Colours {
            background: top.background.or(self.background),
            text: top.text.or(self.text),
            muted: top.muted.or(self.muted),
            error: top.error.or(self.error),
            focus: top.focus.or(self.focus),
            done: top.done.or(self.done),
            agent: top.agent.or(self.agent),
        }
    }

    /// The theme: each key set here, and the default for the rest.
    pub(super) fn resolve(self) -> Theme {
        let d = Theme::default();
        Theme {
            background: self.background.unwrap_or(d.background),
            text: self.text.unwrap_or(d.text),
            muted: self.muted.unwrap_or(d.muted),
            error: self.error.unwrap_or(d.error),
            focus: self.focus.unwrap_or(d.focus),
            done: self.done.unwrap_or(d.done),
            agent: self.agent.unwrap_or(d.agent),
        }
    }

    /// The slot of `[theme]` key `key`; `None` for a key the theme has not.
    pub(super) fn slot(&mut self, key: &str) -> Option<&mut Option<Rgb>> {
        Some(match key {
            "background" => &mut self.background,
            "text" => &mut self.text,
            "muted" => &mut self.muted,
            "error" => &mut self.error,
            "focus" => &mut self.focus,
            "done" => &mut self.done,
            "agent" => &mut self.agent,
            _ => return None,
        })
    }
}

pub(super) const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}
