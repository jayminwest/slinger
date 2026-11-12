//! One `AppKit` view per canvas pane. It keeps the pane's last
//! [`canvas::Scene`] and paints it: smoothed pen lines at a constant width,
//! text, boxes and arrows, each in its colour, on the pane's background,
//! and the chrome over them: panels and a text's caret.
//! A new scene repaints only the box the scene says changed; `AppKit` then
//! asks for that box alone, and only the marks that touch it are painted.
//! The view is flipped, so its points are the scene's: top-left origin.
//!
//! A scene made by a pen event is painted, then the delay from that event
//! to the paint goes to the app as [`Event::Painted`]: from the event's
//! time stamp to the end of `drawRect:` (the screen shows it a frame or so
//! later).
//!
//! Laying text out costs far more than drawing it, so each text is laid out
//! once, in canvas points, and kept while it is drawn the same; a pan or a
//! zoom only moves and scales the glyphs.
//!
//! Over the view the pointer is the scene's [`Cursor`]: a system cursor, or
//! a ring drawn once in its colour.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAffineTransformNSAppKitAdditions, NSBezierPath, NSCursor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSGraphicsContext, NSImage, NSLayoutManager, NSLineCapStyle,
    NSLineJoinStyle, NSRectClip, NSRectFill, NSResponder, NSTextContainer, NSTextStorage, NSView,
};
use objc2_foundation::{
    NSAffineTransform, NSAttributedString, NSAttributedStringKey, NSDictionary,
    NSMutableAttributedString, NSObject, NSPoint, NSProcessInfo, NSRange, NSRect, NSSize, NSString,
};

use super::{Event, color, dispatch};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Draw, ItemId, Mark, Scene};
use crate::scene::cursor::Cursor;

/// How tall a text column may grow, in canvas points: text is never cut.
const TALL: f64 = 1e7;
/// A dashed panel's dashes and gaps, in screen points.
const DASH: [f64; 2] = [4.0, 3.0];

/// The scene a view shows, its texts laid out, the time of the oldest
/// pen event shown but not yet painted, and the pointer over it.
#[derive(Default)]
pub struct Ivars {
    scene: RefCell<Option<Rc<Scene>>>,
    texts: RefCell<HashMap<ItemId, Laid>>,
    unpainted: Cell<Option<f64>>,
    cursor: RefCell<Option<(Cursor, Retained<NSCursor>)>>,
}

/// What a text is laid out from: its words, column width, size and colour.
type TextOf = (String, f64, f64, Rgb);

/// One text laid out, and what it was laid out from.
struct Laid {
    from: TextOf,
    manager: Retained<NSLayoutManager>,
    container: Retained<NSTextContainer>,
    // The layout manager does not keep its storage alive.
    _storage: Retained<NSTextStorage>,
}

define_class!(
    // SAFETY: `NSView` may be subclassed, and `CanvasView` has no `Drop` impl.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerCanvasView"]
    #[ivars = Ivars]
    pub struct CanvasView;

    impl CanvasView {
        /// Points run down from the top left, as in a scene.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// Every point is painted, background first.
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, dirty: NSRect) {
            if let Some(scene) = self.ivars().scene.borrow().as_deref() {
                paint(scene, dirty, &mut self.ivars().texts.borrow_mut());
            }
            if let Some(when) = self.ivars().unpainted.take() {
                let delay = NSProcessInfo::processInfo().systemUptime() - when;
                dispatch(Event::Painted(delay), self.mtm());
            }
        }

        /// The pointer is the scene's cursor over the whole view.
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            if let Some((_, cursor)) = self.ivars().cursor.borrow().as_ref() {
                self.addCursorRect_cursor(self.bounds(), cursor);
            }
        }
    }
);

impl CanvasView {
    /// A view at `frame` in its superview, with nothing to show yet.
    pub fn new(frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: `NSView`'s designated initializer, on a fresh allocation.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// Makes the pointer this view's cursor, if it has one yet.
    pub fn set_cursor(&self) {
        if let Some((_, cursor)) = self.ivars().cursor.borrow().as_ref() {
            cursor.set();
        }
    }

    /// Shows `scene` at `frame`, with `cursor` over it. A view that moved,
    /// or that showed nothing, repaints whole; otherwise only the box the
    /// scene says changed. Texts no longer drawn are let go.
    pub fn show(&self, frame: NSRect, scene: Rc<Scene>, cursor: Cursor) {
        let moved = self.frame() != frame;
        if moved {
            self.setFrame(frame);
        }
        let held = &self.ivars().cursor;
        if held.borrow().as_ref().is_none_or(|(now, _)| *now != cursor) {
            held.replace(Some((cursor, ns_cursor(cursor))));
            if let Some(window) = self.window() {
                window.invalidateCursorRectsForView(self);
            }
        }
        let changed = scene.changed;
        if let Some(when) = scene.pen_event {
            let unpainted = &self.ivars().unpainted;
            unpainted.set(Some(unpainted.get().map_or(when, |t| t.min(when))));
        }
        self.ivars().texts.borrow_mut().retain(|id, _| {
            let laid = |d: &Draw| {
                matches!(
                    d.mark,
                    Mark::Text { .. }
                        | Mark::Live { .. }
                        | Mark::Button { .. }
                        | Mark::Caret { .. }
                )
            };
            scene.draws.iter().any(|d| d.id == *id && laid(d))
        });
        let first = self.ivars().scene.replace(Some(scene)).is_none();
        if moved || first {
            self.setNeedsDisplay(true);
        } else if let Some(r) = changed {
            self.setNeedsDisplayInRect(ns_rect(r));
        }
    }
}

/// Paints the marks of `scene` that touch `dirty` over its background, cut
/// off where the scene says ([`Scene::clip`]). `AppKit` does not cut a
/// view's drawing off at its bounds (`clipsToBounds` is off by default
/// since macOS 14, and `dirty` may run past the bounds), so ink past the
/// pane's edge would stay drawn over the sidebar or the pane beside it.
fn paint(scene: &Scene, dirty: NSRect, texts: &mut HashMap<ItemId, Laid>) {
    let dirty = Rect {
        x: dirty.origin.x,
        y: dirty.origin.y,
        width: dirty.size.width,
        height: dirty.size.height,
    };
    let Some(area) = scene.clip(dirty) else { return };
    // `AppKit` saves the graphics state around `drawRect:`, so the clip
    // ends with it.
    NSRectClip(ns_rect(area));
    color(scene.background).setFill();
    NSRectFill(ns_rect(area));
    let mut ink: Option<Rgb> = None;
    for draw in scene.touching(area) {
        if ink != Some(draw.colour) {
            color(draw.colour).set();
            ink = Some(draw.colour);
        }
        match &draw.mark {
            Mark::Stroke { start, curves, width } => {
                let path = line(*width);
                path.moveToPoint(ns(*start));
                for [c1, c2, end] in curves {
                    path.curveToPoint_controlPoint1_controlPoint2(ns(*end), ns(*c1), ns(*c2));
                }
                path.stroke();
            }
            Mark::Box { frame, width } => {
                let path = NSBezierPath::bezierPathWithRect(ns_rect(*frame));
                path.setLineWidth(*width);
                path.stroke();
            }
            Mark::Arrow { from, to, barbs, width } => {
                let path = line(*width);
                path.moveToPoint(ns(*from));
                path.lineToPoint(ns(*to));
                for barb in barbs {
                    path.moveToPoint(ns(*barb));
                    path.lineToPoint(ns(*to));
                }
                path.stroke();
            }
            Mark::Text { at, width, text, size, zoom } => {
                let from = (text.clone(), *width, *size, draw.colour);
                draw_text(laid(texts, draw.id, from), *at, *zoom);
            }
            Mark::Live { frame, name, at, size, zoom, .. } => {
                NSBezierPath::bezierPathWithRect(ns_rect(*frame)).stroke();
                let from = (name.clone(), frame.width / zoom, *size, draw.colour);
                draw_text(laid(texts, draw.id, from), *at, *zoom);
            }
            Mark::Button { frame, label, at, size, width, zoom } => {
                let corner = frame.height / 4.0;
                let r = ns_rect(*frame);
                let path =
                    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(r, corner, corner);
                path.setLineWidth(*width);
                path.stroke();
                let from = (label.clone(), frame.width / zoom, *size, draw.colour);
                draw_text(laid(texts, draw.id, from), *at, *zoom);
            }
            Mark::Panel { frame, fill, width, dashed } => {
                let path = NSBezierPath::bezierPathWithRect(ns_rect(*frame));
                if let Some(fill) = fill {
                    color(fill.colour).colorWithAlphaComponent(fill.alpha).setFill();
                    path.fill();
                    color(draw.colour).setFill();
                }
                if *width > 0.0 {
                    path.setLineWidth(*width);
                    if *dashed {
                        let count = DASH.len().cast_signed();
                        // SAFETY: the pattern is `count` floats that outlive the call.
                        unsafe { path.setLineDash_count_phase(DASH.as_ptr(), count, 0.0) };
                    }
                    path.stroke();
                }
            }
            Mark::Caret { at, column, text, size, zoom, width } => {
                let from = (text.clone(), *column, *size, draw.colour);
                let end = end_of(laid(texts, draw.id, from));
                let bar = Rect {
                    x: at.x + end.origin.x * zoom,
                    y: at.y + end.origin.y * zoom,
                    width: *width,
                    height: end.size.height * zoom,
                };
                NSBezierPath::bezierPathWithRect(ns_rect(bar)).fill();
            }
        }
    }
}

/// Where the next character of a laid-out text goes, in its own points: a
/// line's top left, as tall as the line. After a line break, or in an empty
/// text, that is the start of the line the layout keeps for it; otherwise
/// the right edge of the last character.
fn end_of(laid: &Laid) -> NSRect {
    let glyphs = laid.manager.glyphRangeForTextContainer(&laid.container);
    let extra = laid.manager.extraLineFragmentRect();
    if extra.size.height > 0.0 || glyphs.length == 0 {
        return NSRect::new(extra.origin, NSSize::new(0.0, extra.size.height));
    }
    let last = NSRange::new(glyphs.end() - 1, 1);
    let r = laid.manager.boundingRectForGlyphRange_inTextContainer(last, &laid.container);
    NSRect::new(
        NSPoint::new(r.origin.x + r.size.width, r.origin.y),
        NSSize::new(0.0, r.size.height),
    )
}

/// The system cursor `cursor` names, or its ring drawn.
fn ns_cursor(cursor: Cursor) -> Retained<NSCursor> {
    match cursor {
        Cursor::Arrow => NSCursor::arrowCursor(),
        Cursor::Crosshair => NSCursor::crosshairCursor(),
        Cursor::IBeam => NSCursor::IBeamCursor(),
        Cursor::Ring { colour, diameter } => ring(colour, diameter),
    }
}

/// A ring `diameter` points across in `colour`, pointing at its centre.
#[expect(deprecated, reason = "lockFocus draws a few points once, without a block")]
fn ring(colour: Rgb, diameter: f64) -> Retained<NSCursor> {
    let side = diameter + 2.0;
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(side, side));
    image.lockFocus();
    let path = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
        NSPoint::new(1.0, 1.0),
        NSSize::new(diameter, diameter),
    ));
    path.setLineWidth(1.5);
    color(colour).setStroke();
    path.stroke();
    image.unlockFocus();
    NSCursor::initWithImage_hotSpot(NSCursor::alloc(), &image, NSPoint::new(side / 2.0, side / 2.0))
}

/// The text `from` laid out, kept for item `id`: laid out again only when
/// it changed.
fn laid(texts: &mut HashMap<ItemId, Laid>, id: ItemId, from: TextOf) -> &Laid {
    if texts.get(&id).is_none_or(|laid| laid.from != from) {
        texts.insert(id, lay_out(from));
    }
    &texts[&id]
}

/// An empty path for a line `width` points wide, with round ends and corners.
fn line(width: f64) -> Retained<NSBezierPath> {
    let path = NSBezierPath::new();
    path.setLineWidth(width);
    path.setLineCapStyle(NSLineCapStyle::Round);
    path.setLineJoinStyle(NSLineJoinStyle::Round);
    path
}

/// Lays a text out in the system font, wrapped to its column, in canvas points.
fn lay_out(from: TextOf) -> Laid {
    let (text, width, size, colour) = &from;
    let font = NSFont::systemFontOfSize(*size);
    let ink = color(*colour);
    // SAFETY: the two keys are AppKit's own statics.
    let keys: [&NSAttributedStringKey; 2] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [&font, &ink];
    let attributes = NSDictionary::from_slices(&keys, &values);
    let string = NSString::from_str(text);
    // SAFETY: each value is the type its key takes: an `NSFont` and an `NSColor`.
    let styled = unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &string,
            Some(&attributes),
        )
    };
    let storage = NSTextStorage::new();
    let editable: &NSMutableAttributedString = &storage;
    editable.setAttributedString(&styled);
    let container =
        NSTextContainer::initWithSize(NSTextContainer::alloc(), NSSize::new(*width, TALL));
    container.setLineFragmentPadding(0.0);
    let manager = NSLayoutManager::new();
    manager.addTextContainer(&container);
    storage.addLayoutManager(&manager);
    Laid { from, manager, container, _storage: storage }
}

/// Draws laid-out text from `at`, `zoom` times its laid-out size.
fn draw_text(laid: &Laid, at: Point, zoom: f64) {
    NSGraphicsContext::saveGraphicsState_class();
    let place = NSAffineTransform::transform();
    place.translateXBy_yBy(at.x, at.y);
    place.scaleBy(zoom);
    place.concat();
    let glyphs = laid.manager.glyphRangeForTextContainer(&laid.container);
    laid.manager.drawGlyphsForGlyphRange_atPoint(glyphs, NSPoint::new(0.0, 0.0));
    NSGraphicsContext::restoreGraphicsState_class();
}

fn ns(p: Point) -> NSPoint {
    NSPoint::new(p.x, p.y)
}

fn ns_rect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.width, r.height))
}
