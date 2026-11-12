//! The one-line entry (the sling's prompt, a note of feedback, a page's
//! new name): a native text field, so typing, the caret, input methods and
//! accents work as in any Mac app, in a rounded box with a border, its
//! label above and its keys beneath. It lives while the scene has an
//! entry, over every other view. Each edit goes to the app as
//! [`Event::Typed`] with the whole text. Return and escape never reach it:
//! the window's `sendEvent:` hands every key to the app first, which keeps
//! those two. The Edit menu's cut, copy, paste and select all reach the
//! field along the responder chain: Flick's dictation pastes what it heard
//! with cmd+V.
//!
//! Focus has one path, [`Kept::focus`], taken on every draw while the entry
//! is open and each time the window becomes key: the field takes the
//! keyboard back whenever it lost it (a press that went on to an agent
//! UI's page, the window losing the key), and the caret is the entry's
//! colour each time it starts editing ([`EntryField`]'s
//! `becomeFirstResponder`), not the system's dark default, which is
//! invisible on the dark box. An entry of another kind (feedback after the
//! sling's) gets a field of its own, with its own placeholder.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSControl, NSFocusRingType, NSFont, NSResponder, NSTextAlignment, NSTextField, NSView,
    NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{NSObject, NSString};

use super::chrome::border;
use super::text::sidebar_row;
use super::{Event, color, dispatch, flipped};
use crate::scene::sling::Entry;

define_class!(
    // SAFETY: `NSTextField` may be subclassed, and `EntryField` has no ivars
    // and no `Drop` impl.
    #[unsafe(super(NSTextField, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerEntryField"]
    pub struct EntryField;

    impl EntryField {
        /// The field takes the keyboard: its field editor's caret is the
        /// field's text colour, each time, whichever way the focus came.
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            // SAFETY: `NSTextField`'s own `becomeFirstResponder`.
            let took: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if let (true, Some(editor), Some(colour)) = (took, self.currentEditor(), self.textColor()) {
                // SAFETY: a text field's field editor is an `NSTextView`,
                // whose caret takes an `NSColor`.
                let () = unsafe { msg_send![&*editor, setInsertionPointColor: &*colour] };
            }
            took
        }

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
        let frame = flipped(height, entry.field);
        // SAFETY: `NSTextField`'s designated initializer, on a fresh
        // allocation of a subclass with no ivars.
        let field: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] };
        field.setEditable(true);
        // The box behind it is its bezel and its focus ring.
        field.setBezeled(false);
        field.setBordered(false);
        field.setDrawsBackground(false);
        field.setFocusRingType(NSFocusRingType::None);
        field.setTextColor(Some(&color(entry.colour)));
        field.setFont(Some(&NSFont::systemFontOfSize(entry.size)));
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

/// The entry on screen: its field, the views around it (the box, the
/// label and the keys, lowest first) and what they were made from (the
/// entry with no text), so they are made again only when that changes.
pub struct Kept {
    field: Retained<EntryField>,
    around: Vec<Retained<NSView>>,
    made_from: Entry,
}

impl Kept {
    /// The lowest of its views: the window's other chrome goes under it.
    pub fn bottom(&self) -> &NSView {
        self.around.first().map_or(&self.field, |box_| box_)
    }

    /// Gives the field `window`'s keyboard unless it is editing already:
    /// the one way an entry gets its focus and its caret.
    pub fn focus(&self, window: &NSWindow) {
        if self.field.currentEditor().is_none() {
            window.makeFirstResponder(Some(&self.field));
        }
    }

    /// Takes its views out of the window.
    fn remove(&self) {
        self.around.iter().for_each(|v| v.removeFromSuperview());
        self.field.removeFromSuperview();
    }

    /// Makes the views around the field from `entry`, under the field.
    fn surround(&mut self, view: &NSView, entry: &Entry, height: f64, mtm: MainThreadMarker) {
        for old in self.around.drain(..) {
            old.removeFromSuperview();
        }
        let edge = border(&entry.border, height, mtm);
        edge.setFillColor(&color(entry.fill));
        edge.setCornerRadius(entry.radius);
        self.around.push(Retained::into_super(edge));
        for (row, align) in
            [(&entry.title, NSTextAlignment::Left), (&entry.keys, NSTextAlignment::Left)]
                .into_iter()
                .chain(entry.dictate.as_ref().map(|row| (row, NSTextAlignment::Right)))
        {
            let line = sidebar_row(row, height, mtm);
            line.setAlignment(align);
            self.around.push(Retained::into_super(Retained::into_super(line)));
        }
        // Each right under the field, so the last made is the highest.
        for around in &self.around {
            view.addSubview_positioned_relativeTo(
                around,
                NSWindowOrderingMode::Below,
                Some(&self.field),
            );
        }
        self.made_from = Entry { text: String::new(), ..entry.clone() };
    }
}

/// Makes `view` (`height` tall) of `window` show `entry`: a field made
/// when the entry appears (or turns into an entry of another kind: its
/// title or placeholder differ), in its box with its label and keys; moved
/// and given the entry's text while it stays (its text only when it
/// differs, so the caret stays where it is), the views around it made
/// again when they change; all removed when the entry goes, the keyboard
/// back to the window. While open, the field has the keyboard
/// ([`Kept::focus`]).
pub fn place(
    window: &NSWindow,
    view: &NSView,
    kept: &mut Option<Kept>,
    entry: Option<&Entry>,
    height: f64,
    mtm: MainThreadMarker,
) {
    let same = |kept: &Kept, entry: &Entry| {
        kept.made_from.title.label.text == entry.title.label.text
            && kept.made_from.placeholder == entry.placeholder
    };
    if let Some(old) = kept.take_if(|kept| entry.is_none_or(|entry| !same(kept, entry))) {
        old.remove();
        window.makeFirstResponder(None);
    }
    let Some(entry) = entry else { return };
    if let Some(kept) = kept.as_mut() {
        kept.field.setFrame(flipped(height, entry.field));
        let text = NSString::from_str(&entry.text);
        if kept.field.stringValue() != text {
            kept.field.setStringValue(&text);
        }
        if kept.made_from != (Entry { text: String::new(), ..entry.clone() }) {
            kept.surround(view, entry, height, mtm);
        }
        kept.focus(window);
        return;
    }
    let field = EntryField::new(entry, height, mtm);
    view.addSubview(&field);
    let mut shown = Kept { field, around: Vec::new(), made_from: entry.clone() };
    shown.surround(view, entry, height, mtm);
    shown.focus(window);
    *kept = Some(shown);
}
