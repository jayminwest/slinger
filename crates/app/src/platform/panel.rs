//! The open panel: asks for a folder (a workspace's, or one to export into)
//! or a file to open (a PDF or a tldraw file), and says which was chosen.
//!
//! The panel runs app-modal, with the app in front. While it is up, the menu
//! bar holds only an Edit menu whose items go to the panel's fields (undo,
//! cut, copy, paste, select all): the app's own commands cannot run then, and
//! their keys (⇧⌘D, ⇧⌘O, ⌘1 to ⌘8...) would shadow the panel's own (go to
//! the Desktop, to Documents, change the view). The app's menu bar comes
//! back as it was when the panel closes.
//!
//! What the panel may show is macOS's to say: the panel runs in a process of
//! its own, and reads the Desktop, Documents and Downloads folders as the
//! app's bundle (`Info.plist` says why it asks). A bare `slinger-app` run
//! from a shell has no bundle, so macOS asks on behalf of what started it
//! (a terminal, or `sshd` for a run over ssh), which may never be allowed in.

use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem, NSModalResponseOK, NSOpenPanel,
};
use objc2_foundation::{NSArray, NSString};

/// What the open panel asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A folder to open as a workspace; the panel can make a new one.
    Workspace,
    /// A folder to export the focused pane's UI into.
    ExportTo,
    /// One PDF or tldraw file to open as a canvas.
    Document,
}

impl Pick {
    /// The panel's message over its files, and its button's title.
    const fn words(self) -> (&'static str, &'static str) {
        match self {
            Self::Workspace => ("Choose a folder to open as a workspace.", "Open"),
            Self::ExportTo => ("Choose the folder to export the UI into.", "Export"),
            Self::Document => ("Choose a PDF or a tldraw file to open as a canvas.", "Open"),
        }
    }
}

/// The file extensions a [`Pick::Document`] may be.
const DOCUMENTS: [&str; 3] = ["pdf", "tldraw", "tldr"];

/// Asks for what `pick` says in the open panel. `None` when the panel is
/// cancelled, or off the main thread.
pub fn choose(pick: Pick) -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSOpenPanel::openPanel(mtm);
    let (message, prompt) = pick.words();
    panel.setMessage(Some(&NSString::from_str(message)));
    panel.setPrompt(Some(&NSString::from_str(prompt)));
    let folder = pick != Pick::Document;
    panel.setCanChooseFiles(!folder);
    panel.setCanChooseDirectories(folder);
    panel.setCanCreateDirectories(folder);
    panel.setAllowsMultipleSelection(false);
    if !folder {
        allow(&panel, &NSArray::from_retained_slice(&DOCUMENTS.map(NSString::from_str)));
    }
    let app = NSApplication::sharedApplication(mtm);
    app.activate();
    let menus = app.mainMenu();
    app.setMainMenu(Some(&editing(mtm)));
    let chosen = panel.runModal() == NSModalResponseOK;
    app.setMainMenu(menus.as_deref());
    if !chosen {
        return None;
    }
    panel.URLs().firstObject()?.to_file_path()
}

#[expect(
    deprecated,
    reason = "allowedContentTypes needs UniformTypeIdentifiers, a crate more for one type"
)]
fn allow(panel: &NSOpenPanel, types: &NSArray<NSString>) {
    panel.setAllowedFileTypes(Some(types));
}

/// The menu bar while the panel is up: the app's menu, empty, and an Edit
/// menu whose items go along the responder chain to the panel's fields.
fn editing(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let shift = NSEventModifierFlags::Command | NSEventModifierFlags::Shift;
    let items: [(&str, Sel, &str, NSEventModifierFlags); 6] = [
        ("Undo", sel!(undo:), "z", NSEventModifierFlags::Command),
        ("Redo", sel!(redo:), "z", shift),
        ("Cut", sel!(cut:), "x", NSEventModifierFlags::Command),
        ("Copy", sel!(copy:), "c", NSEventModifierFlags::Command),
        ("Paste", sel!(paste:), "v", NSEventModifierFlags::Command),
        ("Select All", sel!(selectAll:), "a", NSEventModifierFlags::Command),
    ];
    let edit = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Edit"));
    for (title, action, key, mods) in items {
        // SAFETY: a fresh allocation; each action is a one-argument
        // selector that a text field on the responder chain takes.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        item.setKeyEquivalentModifierMask(mods);
        edit.addItem(&item);
    }
    let bar = NSMenu::new(mtm);
    for (title, sub) in [("slinger", NSMenu::new(mtm)), ("Edit", edit)] {
        let head = NSMenuItem::new(mtm);
        head.setTitle(&NSString::from_str(title));
        head.setSubmenu(Some(&sub));
        bar.addItem(&head);
    }
    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundle's `Info.plist` (scripts/bundle.sh).
    const PLIST: &str = include_str!("../../Info.plist");

    #[test]
    fn each_panel_says_what_it_asks_for_and_what_its_button_does() {
        let picks = [Pick::Workspace, Pick::ExportTo, Pick::Document];
        let messages: Vec<_> = picks.iter().map(|p| p.words().0).collect();
        assert!(messages.iter().all(|m| m.ends_with('.')), "{messages:?}");
        assert_eq!(messages.iter().collect::<std::collections::HashSet<_>>().len(), 3);
        assert_eq!(Pick::ExportTo.words().1, "Export");
        assert!(Pick::Document.words().0.contains("PDF"));
    }

    #[test]
    fn the_bundle_says_why_it_reads_the_folders_macos_guards() {
        for key in [
            "NSDownloadsFolderUsageDescription",
            "NSDesktopFolderUsageDescription",
            "NSDocumentsFolderUsageDescription",
            "NSRemovableVolumesUsageDescription",
            "NSNetworkVolumesUsageDescription",
        ] {
            let at = PLIST.find(&format!("<key>{key}</key>")).unwrap_or_else(|| panic!("{key}"));
            let value = PLIST[at..].lines().next().unwrap_or_default();
            assert!(value.contains("<string>") && !value.contains("<string></string>"), "{value}");
        }
    }

    #[test]
    fn the_bundle_names_its_identifier_and_takes_its_version_from_the_workspace() {
        assert!(
            PLIST.contains("<key>CFBundleIdentifier</key><string>com.github.jayminwest.slinger")
        );
        assert_eq!(
            PLIST.matches("@VERSION@").count(),
            2,
            "CFBundleShortVersionString and CFBundleVersion"
        );
    }
}
