//! The window's file: the title bar that names the focused pane's canvas,
//! its file under it and that file's proxy icon, the Finder showing it,
//! the open panel that picks a workspace's folder, and files dropped on it.

use std::path::PathBuf;

use objc2::MainThreadMarker;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSDraggingInfo, NSModalResponseOK, NSOpenPanel, NSPasteboardType, NSPasteboardTypeFileURL,
    NSWindow, NSWorkspace,
};
use objc2_foundation::{NSArray, NSString, NSURL};

use crate::layout::Point;
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

/// Asks for a folder to open as a workspace; the panel can make a new one.
/// `None` when the panel is cancelled, or off the main thread.
pub fn choose_folder() -> Option<PathBuf> {
    let panel = NSOpenPanel::openPanel(MainThreadMarker::new()?);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setCanCreateDirectories(true);
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    panel.URLs().firstObject()?.to_file_path()
}

/// `AppKit`'s pasteboard type for a file's URL.
fn file_url() -> &'static NSPasteboardType {
    // SAFETY: an `AppKit` constant, set before any code runs and never changed.
    unsafe { NSPasteboardTypeFileURL }
}

/// Lets files be dragged onto `window` (`draggingEntered:`).
pub(super) fn take_drops(window: &NSWindow) {
    window.registerForDraggedTypes(&NSArray::from_slice(&[file_url()]));
}

/// Where the drag `info` is in `window`'s content, from the top left, and
/// the files it holds; the app says what to do with them.
pub(super) fn dropped(
    window: &NSWindow,
    info: &ProtocolObject<dyn NSDraggingInfo>,
) -> (Point, Vec<PathBuf>) {
    let at = info.draggingLocation();
    let height = window.contentView().map_or(0.0, |v| v.frame().size.height);
    let items = info.draggingPasteboard().pasteboardItems().unwrap_or_default();
    let url = |s: &NSString| NSURL::URLWithString(s)?.to_file_path();
    let item = |n| items.objectAtIndex(n).stringForType(file_url());
    let files = (0..items.count()).filter_map(|n| url(&*item(n)?)).collect();
    (Point { x: at.x, y: height - at.y }, files)
}
