//! The window's file: the title bar that names the focused pane's canvas,
//! its file under it and that file's proxy icon, and the Finder showing it.

use objc2_app_kit::{NSWindow, NSWorkspace};
use objc2_foundation::NSString;

use crate::scene::title::Title;

/// Makes the title bar of `window` say `title`; each part set only when it
/// changed. No file leaves no proxy icon.
pub(super) fn title(window: &NSWindow, title: &Title) {
    let text = NSString::from_str(&title.text);
    if window.title() != text {
        window.setTitle(&text);
    }
    let subtitle = NSString::from_str(&title.subtitle);
    if window.subtitle() != subtitle {
        window.setSubtitle(&subtitle);
    }
    let file = NSString::from_str(title.file.as_deref().unwrap_or_default());
    if window.representedFilename() != file {
        window.setRepresentedFilename(&file);
    }
}

/// Shows the file at `path` selected in a Finder window; false when the
/// Finder cannot (the file is gone).
pub fn reveal(path: &str) -> bool {
    let (file, root) = (NSString::from_str(path), NSString::from_str(""));
    NSWorkspace::sharedWorkspace().selectFile_inFileViewerRootedAtPath(Some(&file), &root)
}
