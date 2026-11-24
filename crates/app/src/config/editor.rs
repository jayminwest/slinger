//! Which Nvim edits a canvas text (`[editor] nvim`,
//! docs/design/writing-column.md). Pure, like the rest of `config`.

/// Which Nvim edits a canvas text (`[editor] nvim`). Without one, the
/// app's own editor types it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Nvim {
    /// The first `nvim` on the app's `PATH` or in the usual places a
    /// Finder-launched app does not have on its `PATH`.
    #[default]
    Auto,
    /// None: the app's own editor types every text.
    Off,
    /// This one, by its path (`~/` allowed).
    Path(String),
}

impl Nvim {
    /// Parses `"auto"`, `"off"`, or a path that starts with `/` or `~/`.
    pub(super) fn parse(value: &str) -> Option<Nvim> {
        match value {
            "auto" => Some(Nvim::Auto),
            "off" => Some(Nvim::Off),
            path if path.starts_with('/') || path.starts_with("~/") => {
                Some(Nvim::Path(path.to_string()))
            }
            _ => None,
        }
    }
}
