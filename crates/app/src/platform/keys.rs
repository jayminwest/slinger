//! What the keyboard and the clipboard give the app: a key press as a
//! [`Chord`] and the text it types; and for a cut, copy or paste no text
//! field takes, the clipboard as a [`Clip`]: its plain text, and a
//! canvas's items under a type of the app's own, which other apps leave be.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSPasteboard, NSPasteboardType, NSPasteboardTypeString,
};
use objc2_foundation::NSString;

use crate::commands::Chord;
use crate::scene::paste::Clip;

/// The chord of a key press: the device-independent modifier flags, whose
/// shift, control, option and command bits (17 to 20) are the [`Chord`]
/// bits in order, and the key's character with no modifier but shift.
pub(super) fn chord(event: &NSEvent) -> Option<Chord> {
    let chars = event.charactersIgnoringModifiers()?;
    if chars.length() == 0 {
        return None;
    }
    let key = char::from_u32(chars.characterAtIndex(0).into())?;
    #[expect(clippy::cast_possible_truncation, reason = "masked to four bits")]
    let mods = ((event.modifierFlags().0 >> 17) & 0xf) as u8;
    Some(Chord::new(mods, key))
}

/// The characters a key press types, if any. A key with command held types
/// nothing: cmd+C is not a "c".
pub(super) fn typed(event: &NSEvent) -> Option<String> {
    if event.modifierFlags().contains(NSEventModifierFlags::Command) {
        return None;
    }
    let chars = event.characters()?;
    (chars.length() > 0).then(|| chars.to_string())
}

/// The clipboard's type for a canvas's items: their file's objects, as JSON.
const ITEMS: &str = "com.github.jayminwest.slinger.items";

/// The clipboard's plain text, under `AppKit`'s type for it.
fn text_type() -> &'static NSPasteboardType {
    // SAFETY: `NSPasteboardTypeString` is an `AppKit` constant, set before
    // any code runs and never changed.
    unsafe { NSPasteboardTypeString }
}

/// What the clipboard holds for the app.
pub(super) fn clip(_mtm: MainThreadMarker) -> Clip {
    let board = NSPasteboard::generalPasteboard();
    let read = |kind: &NSPasteboardType| board.stringForType(kind).map(|s| s.to_string());
    Clip::new(read(text_type()), read(&NSString::from_str(ITEMS)))
}

/// Puts `clip` on the clipboard in place of what it held.
pub(super) fn copy(clip: &Clip, _mtm: MainThreadMarker) {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    let items = NSString::from_str(ITEMS);
    let kinds = [(&clip.text, text_type()), (&clip.items, &*items)];
    for (value, kind) in kinds {
        if let Some(value) = value {
            board.setString_forType(&NSString::from_str(value), kind);
        }
    }
}
