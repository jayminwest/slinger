//! What the window keeps from one launch to the next, in the app's user
//! defaults: its frame, which `AppKit` saves each time it moves or resizes,
//! and whether the sidebar was open.

use objc2_app_kit::NSWindow;
use objc2_foundation::{NSString, NSUserDefaults};

/// The name the window's frame is saved under.
const FRAME: &str = "slinger window";
/// The key for whether the sidebar was open.
const SIDEBAR: &str = "sidebar open";

/// Puts `window` where it was last left, if it was ever saved (`AppKit`
/// keeps it on a screen), and saves its frame from now on.
pub(super) fn frame(window: &NSWindow) {
    let name = NSString::from_str(FRAME);
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
