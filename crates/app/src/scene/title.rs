//! Where a canvas is saved, said plainly (SPEC P6). The daemon is a
//! canvas's only writer (SPEC K5) and writes each change as it is made, so
//! there is nothing to save by hand: the window's title bar names the
//! focused pane's canvas, with its file under it (the title's proxy icon
//! is that file), and `save` says where it went. Pure: `platform` sets the
//! title bar and shows the file in the Finder.

use std::path::Path;

/// What the window's title bar says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title {
    /// The title: the focused pane's canvas, else "slinger".
    pub text: String,
    /// The line under it: the canvas's file, with `~` for the home
    /// directory; empty when there is none.
    pub subtitle: String,
    /// The file the title stands for, in full: its proxy icon.
    pub file: Option<String>,
}

impl Title {
    /// A title with no file under it.
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), subtitle: String::new(), file: None }
    }
}

/// What a file command flashes when the focused pane shows no canvas.
pub const NO_CANVAS: &str = "No canvas in this pane";

/// The title bar for the focused pane's `canvas` and its file's `path`
/// (`None` until the daemon has said, or from one too old to say), with
/// `home` as `~`. A pane that shows no canvas leaves "slinger".
pub fn window(canvas: Option<(&str, Option<&str>)>, home: Option<&Path>) -> Title {
    let Some((canvas, path)) = canvas else { return Title::plain("slinger") };
    let subtitle = path.map(|p| tilde(p, home)).unwrap_or_default();
    Title { text: canvas.to_string(), subtitle, file: path.map(str::to_string) }
}

/// What `save` flashes for the focused pane's canvas, whose file is at
/// `path` (if the daemon said): where every change goes, whether the last
/// ones are still on their way (`settled` false), or why one did not go.
pub fn saved(
    canvas: Option<(&str, Option<&str>)>,
    settled: bool,
    problem: Option<&str>,
    home: Option<&Path>,
) -> String {
    let Some((canvas, path)) = canvas else { return NO_CANVAS.to_string() };
    let file = path.map_or_else(|| canvas.to_string(), |p| tilde(p, home));
    match (problem, settled) {
        (Some(why), _) => format!("Not saved yet: {why}"),
        (None, false) => format!("Saving to {file}"),
        (None, true) => format!("Saved to {file}"),
    }
}

/// `path` with `home` written as `~`; as it is outside `home`.
pub fn tilde(path: &str, home: Option<&Path>) -> String {
    let under = home.and_then(|home| Path::new(path).strip_prefix(home).ok());
    match under {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "/Users/me/Library/Application Support/slinger/canvases/main.canvas";

    const ME: &str = "/Users/me";

    #[test]
    fn the_title_names_the_canvas_and_its_file_under_it() {
        let title = window(Some(("main", Some(FILE))), Some(Path::new(ME)));
        assert_eq!(title.text, "main");
        assert_eq!(title.subtitle, "~/Library/Application Support/slinger/canvases/main.canvas");
        assert_eq!(title.file.as_deref(), Some(FILE));
    }

    #[test]
    fn a_title_waits_for_the_file_and_leaves_slinger_without_a_canvas() {
        assert_eq!(window(Some(("main", None)), Some(Path::new(ME))), Title::plain("main"));
        assert_eq!(window(None, Some(Path::new(ME))), Title::plain("slinger"));
    }

    #[test]
    fn tilde_shortens_only_a_path_under_home() {
        assert_eq!(tilde("/Users/me", Some(Path::new(ME))), "~");
        assert_eq!(tilde("/Users/meg/x.canvas", Some(Path::new(ME))), "/Users/meg/x.canvas");
        assert_eq!(tilde("/data/x.canvas", Some(Path::new(ME))), "/data/x.canvas");
        assert_eq!(tilde("/data/x.canvas", None), "/data/x.canvas");
    }

    #[test]
    fn save_says_where_every_change_went_or_why_one_did_not() {
        let main = Some(("main", Some(FILE)));
        let gone = "~/Library/Application Support/slinger/canvases/main.canvas";
        assert_eq!(saved(main, true, None, Some(Path::new(ME))), format!("Saved to {gone}"));
        assert_eq!(saved(main, false, None, Some(Path::new(ME))), format!("Saving to {gone}"));
        let down = saved(main, true, Some("slingerd is not running"), Some(Path::new(ME)));
        assert_eq!(down, "Not saved yet: slingerd is not running");
        assert_eq!(saved(Some(("main", None)), true, None, Some(Path::new(ME))), "Saved to main");
        assert_eq!(saved(None, true, None, Some(Path::new(ME))), NO_CANVAS);
    }
}
