//! The sling's one-line entry: a native text field, so typing, the caret,
//! input methods and accents work as in any Mac app. It lives while the
//! scene has an entry, over every other view, and takes the keyboard when
//! it appears. Each edit goes to the app as [`Event::Typed`] with the
//! whole text. Return and escape never reach it: the window's
//! `sendEvent:` hands every key to the app first, which keeps those two.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSControl, NSResponder, NSTextField, NSView, NSWindow};
use objc2_foundation::{NSObject, NSString};

use super::{Event, dispatch, flipped};
use crate::scene::sling::Entry;

define_class!(
    // SAFETY: `NSTextField` may be subclassed, and `EntryField` has no ivars
    // and no `Drop` impl.
    #[unsafe(super(NSTextField, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerEntryField"]
    pub struct EntryField;

    impl EntryField {
        /// The field editor changed the text: the field's own work, then
        /// the whole text to the app.
        #[unsafe(method(textDidChange:))]
        fn text_did_change(&self, note: &AnyObject) {
            // SAFETY: `NSTextField`'s own `textDidChange:`, with its notification.
            let () = unsafe { msg_send![super(self), textDidChange: note] };
            dispatch(Event::Typed(self.stringValue().to_string()), self.mtm());
        }
    }
);

impl EntryField {
    fn new(entry: &Entry, height: f64, mtm: MainThreadMarker) -> Retained<Self> {
        let frame = flipped(height, entry.frame);
        // SAFETY: `NSTextField`'s designated initializer, on a fresh
        // allocation of a subclass with no ivars.
        let field: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] };
        field.setEditable(true);
        field.setBezeled(true);
        field.setUsesSingleLineMode(true);
        // SAFETY: a text field's cell is an `NSTextFieldCell`, which scrolls
        // its text sideways when told to, instead of wrapping it.
        unsafe {
            let cell: Retained<AnyObject> = msg_send![&*field, cell];
            let () = msg_send![&*cell, setScrollable: true];
        }
        field.setPlaceholderString(Some(&NSString::from_str(&entry.placeholder)));
        field.setStringValue(&NSString::from_str(&entry.text));
        field
    }
}

/// Makes `view` (`height` tall) of `window` show `entry`: a field made
/// when the entry appears, which then takes the keyboard; moved and given
/// the entry's text while it stays (its text only when it differs, so the
/// caret stays where it is); removed when the entry goes, the keyboard
/// back to the window.
pub fn place(
    window: &NSWindow,
    view: &NSView,
    kept: &mut Option<Retained<EntryField>>,
    entry: Option<&Entry>,
    height: f64,
    mtm: MainThreadMarker,
) {
    let Some(entry) = entry else {
        if let Some(field) = kept.take() {
            field.removeFromSuperview();
            window.makeFirstResponder(None);
        }
        return;
    };
    if let Some(field) = kept.as_ref() {
        field.setFrame(flipped(height, entry.frame));
        let text = NSString::from_str(&entry.text);
        if field.stringValue() != text {
            field.setStringValue(&text);
        }
        return;
    }
    let field = EntryField::new(entry, height, mtm);
    view.addSubview(&field);
    window.makeFirstResponder(Some(&field));
    *kept = Some(field);
}
