//! What the window keeps from one launch to the next, in the app's user
//! defaults: its frame, which `AppKit` saves each time it moves or resizes,
//! and whether the sidebar was open.

use objc2_app_kit::NSWindow;
use objc2_foundation::{NSString, NSUserDefaults};

/// Whose frame a run's window opens at and keeps: the app's window, or a
/// preview's (`slinger-app --preview`, which the tests run), saved apart so
/// a preview never moves or sizes the app's window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    Window,
    Preview,
}

impl Kept {
    /// The name the frame is saved under.
    const fn name(self) -> &'static str {
        match self {
            Self::Window => "slinger window",
            Self::Preview => "slinger preview",
        }
    }
}

/// The key for whether the sidebar was open.
const SIDEBAR: &str = "sidebar open";

/// Puts `window` where the `kept` frame was last left, if it was ever saved
/// (`AppKit` keeps it on a screen), and saves its frame there from now on.
pub(super) fn frame(window: &NSWindow, kept: Kept) {
    let name = NSString::from_str(kept.name());
    window.setFrameUsingName(&name);
    window.setFrameAutosaveName(&name);
}

/// Whether the sidebar was open when last left; `None` before it ever was.
pub fn sidebar() -> Option<bool> {
    let (defaults, key) = (NSUserDefaults::standardUserDefaults(), NSString::from_str(SIDEBAR));
    defaults.objectForKey(&key).is_some().then(|| defaults.boolForKey(&key))
}

/// Keeps whether the sidebar is `open`, for the next launch.
pub fn remember_sidebar(open: bool) {
    NSUserDefaults::standardUserDefaults().setBool_forKey(open, &NSString::from_str(SIDEBAR));
}

#[cfg(test)]
mod tests {
    use super::Kept;

    #[test]
    fn a_preview_keeps_its_frame_apart_from_the_window() {
        assert_ne!(Kept::Preview.name(), Kept::Window.name());
        assert_eq!(Kept::Window.name(), "slinger window", "the app's saved frame keeps its name");
    }
}
