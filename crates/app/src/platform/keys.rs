//! What the keyboard and the clipboard give the app: a key press as a
//! [`Chord`] and the text it types, and the clipboard's plain text for a
//! paste (cmd+V, or Edit > Paste, when no text field has the focus).

use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSPasteboard, NSPasteboardTypeString};

use crate::commands::Chord;

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

/// The clipboard's plain text, if it holds any.
pub(super) fn pasted(_mtm: MainThreadMarker) -> Option<String> {
    // SAFETY: `NSPasteboardTypeString` is an `AppKit` constant, set before
    // any code runs and never changed.
    let kind = unsafe { NSPasteboardTypeString };
    let text = NSPasteboard::generalPasteboard().stringForType(kind)?;
    Some(text.to_string()).filter(|t| !t.is_empty())
}
