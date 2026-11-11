//! One `AppKit` view per canvas pane. It keeps the pane's last
//! [`canvas::Scene`] and paints it: smoothed pen lines at a constant width,
//! text, boxes and arrows, each in its colour, on the pane's background.
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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAffineTransformNSAppKitAdditions, NSBezierPath, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSGraphicsContext, NSLayoutManager, NSLineCapStyle,
    NSLineJoinStyle, NSRectFill, NSResponder, NSTextContainer, NSTextStorage, NSView,
};
use objc2_foundation::{
    NSAffineTransform, NSAttributedString, NSAttributedStringKey, NSDictionary,
    NSMutableAttributedString, NSObject, NSPoint, NSProcessInfo, NSRect, NSSize, NSString,
};

use super::{Event, color, dispatch};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{ItemId, Mark, Scene};

/// How tall a text column may grow, in canvas points: text is never cut.
const TALL: f64 = 1e7;

/// The scene a view shows, its texts laid out, and the time of the oldest
/// pen event shown but not yet painted.
#[derive(Default)]
pub struct Ivars {
    scene: RefCell<Option<Rc<Scene>>>,
    texts: RefCell<HashMap<ItemId, Laid>>,
    unpainted: Cell<Option<f64>>,
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
    }
);

impl CanvasView {
    /// A view at `frame` in its superview, with nothing to show yet.
    pub fn new(frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: `NSView`'s designated initializer, on a fresh allocation.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// Shows `scene` at `frame`. A view that moved, or that showed nothing,
    /// repaints whole; otherwise only the box the scene says changed. Texts
    /// no longer drawn are let go.
    pub fn show(&self, frame: NSRect, scene: Rc<Scene>) {
        let moved = self.frame() != frame;
        if moved {
            self.setFrame(frame);
        }
        let changed = scene.changed;
        if let Some(when) = scene.pen_event {
            let unpainted = &self.ivars().unpainted;
            unpainted.set(Some(unpainted.get().map_or(when, |t| t.min(when))));
        }
        self.ivars().texts.borrow_mut().retain(|id, _| {
            scene.draws.iter().any(|d| d.id == *id && matches!(d.mark, Mark::Text { .. }))
        });
        let first = self.ivars().scene.replace(Some(scene)).is_none();
        if moved || first {
            self.setNeedsDisplay(true);
        } else if let Some(r) = changed {
            self.setNeedsDisplayInRect(ns_rect(r));
        }
    }
}

/// Paints the marks of `scene` that touch `dirty` over its background.
fn paint(scene: &Scene, dirty: NSRect, texts: &mut HashMap<ItemId, Laid>) {
    color(scene.background).setFill();
    NSRectFill(dirty);
    let dirty = Rect {
        x: dirty.origin.x,
        y: dirty.origin.y,
        width: dirty.size.width,
        height: dirty.size.height,
    };
    let mut ink: Option<Rgb> = None;
    for draw in &scene.draws {
        if draw.bounds.intersection(dirty).is_none() {
            continue;
        }
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
                let laid = match texts.get(&draw.id) {
                    Some(laid) if laid.from == from => laid,
                    _ => texts.entry(draw.id).insert_entry(lay_out(from)).into_mut(),
                };
                draw_text(laid, *at, *zoom);
            }
        }
    }
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
