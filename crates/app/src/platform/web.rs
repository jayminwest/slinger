//! Agent UIs in sandboxed web views (docs/design/live-items.md, "How the
//! sandbox holds"). Each view has one [`Host`]: the scheme handler that
//! serves the page, the navigation and UI delegate that keeps it in place,
//! and the bridge's message handler. Every answer comes from a
//! [`Bridge`]; this file only pipes `WebKit` to it.
//!
//! A live item's view is made the same way, so it has every layer of the
//! sandbox. It sits in a view of its own at its canvas pane's frame, which
//! cuts it off at the pane's edges, just over the pane's canvas, and its
//! page is zoomed with the canvas. One the scene says is away is kept,
//! hidden, with its page's state.
//!
//! The rule list compiles once, asynchronously: a view loads its page only
//! once the list is on it, so no load ever runs without it.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::ptr;
use std::sync::Arc;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSView, NSWindowOrderingMode};
use objc2_foundation::{
    NSData, NSDictionary, NSError, NSHTTPURLResponse, NSNumber, NSString, NSURL, NSURLRequest,
};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKContentWorld, WKNavigationAction,
    WKNavigationActionPolicy, WKNavigationDelegate, WKScriptMessage,
    WKScriptMessageHandlerWithReply, WKUIDelegate, WKURLSchemeHandler, WKURLSchemeTask,
    WKUserContentController, WKUserScript, WKUserScriptInjectionTime, WKWebView,
    WKWebViewConfiguration, WKWebsiteDataStore, WKWindowFeatures,
};

use super::flipped;
use crate::config::Theme;
use crate::layout::{PaneId, Rect};
use crate::scene::Web;
use crate::scene::canvas::ItemId;
use crate::scene::live::Shows;
use crate::ui::{self, Bridge, Page};

/// What a [`Host`] holds: the bridge of its one showing.
pub struct Ivars {
    bridge: RefCell<Bridge>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Host` has no
    // `Drop` impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerUiHost"]
    #[ivars = Ivars]
    pub struct Host;

    unsafe impl NSObjectProtocol for Host {}

    unsafe impl WKURLSchemeHandler for Host {
        /// Answers a load: the page for its own URL, 404 for any other.
        #[unsafe(method(webView:startURLSchemeTask:))]
        fn start(&self, _web: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            serve(&self.ivars().bridge.borrow(), task);
        }

        #[unsafe(method(webView:stopURLSchemeTask:))]
        fn stop(&self, _web: &WKWebView, _task: &ProtocolObject<dyn WKURLSchemeTask>) {}
    }

    unsafe impl WKNavigationDelegate for Host {
        /// Lets the page's own first load through and cancels every other
        /// navigation, in any frame.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(
            &self,
            _web: &WKWebView,
            action: &WKNavigationAction,
            decide: &DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: plain getters on the action WebKit hands us.
            let (request, frame) = unsafe { (action.request(), action.targetFrame()) };
            // SAFETY: a getter on a frame WebKit made.
            let main = frame.is_some_and(|f| unsafe { f.isMainFrame() });
            let url = text(request.URL().and_then(|u| u.absoluteString()));
            let go = self.ivars().bridge.borrow_mut().navigate(&url, main);
            let policy =
                if go { WKNavigationActionPolicy::Allow } else { WKNavigationActionPolicy::Cancel };
            decide.call((policy,));
        }
    }

    unsafe impl WKUIDelegate for Host {
        /// Refuses every new window.
        #[unsafe(method(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create(
            &self,
            _web: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            _action: &WKNavigationAction,
            _features: &WKWindowFeatures,
        ) -> *mut WKWebView {
            ptr::null_mut()
        }
    }

    unsafe impl WKScriptMessageHandlerWithReply for Host {
        /// A call from the shim: the bridge answers it, and a reply goes to
        /// the app's answer as [`super::Event::Reply`].
        #[unsafe(method(userContentController:didReceiveScriptMessage:replyHandler:))]
        fn message(
            &self,
            _content: &WKUserContentController,
            message: &WKScriptMessage,
            answer: &DynBlock<dyn Fn(*mut AnyObject, *mut NSString)>,
        ) {
            // SAFETY: a getter on the message WebKit hands us.
            let body = unsafe { message.body() };
            let body = body.downcast_ref::<NSString>().map(NSString::to_string).unwrap_or_default();
            let mtm = self.mtm();
            let said = self.ivars().bridge.borrow_mut().call(&body, |line| super::reply(line, mtm));
            match said {
                Ok(json) => {
                    let json = NSString::from_str(&json);
                    answer.call((Retained::as_ptr(&json).cast_mut().cast(), ptr::null_mut()));
                }
                Err(refusal) => {
                    let why = NSString::from_str(&refusal.to_string());
                    answer.call((ptr::null_mut(), Retained::as_ptr(&why).cast_mut()));
                }
            }
        }
    }
);

impl Host {
    fn new(page: Arc<Page>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { bridge: RefCell::new(Bridge::new(page)) });
        // SAFETY: `NSObject`'s `init` on a fresh allocation.
        unsafe { msg_send![super(this), init] }
    }
}

fn text(s: Option<Retained<NSString>>) -> String {
    s.map(|s| s.to_string()).unwrap_or_default()
}

/// Answers `task` with what the bridge's page says for its URL.
fn serve(bridge: &Bridge, task: &ProtocolObject<dyn WKURLSchemeTask>) {
    // SAFETY: the task's own request.
    let Some(url) = (unsafe { task.request() }).URL() else { return };
    let said = bridge.page().respond(&text(url.absoluteString()));
    let keys: Vec<_> = said.headers.iter().map(|(k, _)| NSString::from_str(k)).collect();
    let values: Vec<_> = said.headers.iter().map(|(_, v)| NSString::from_str(v)).collect();
    let keys: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let values: Vec<&NSString> = values.iter().map(|v| &**v).collect();
    let headers = NSDictionary::from_slices(&keys, &values);
    let response = NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(
        NSHTTPURLResponse::alloc(),
        &url,
        said.status.into(),
        Some(&NSString::from_str("HTTP/1.1")),
        Some(&headers),
    );
    let Some(response) = response else { return };
    // SAFETY: a task gets one response, its data and then the end, in that
    // order, once, on the main thread, as these calls do.
    unsafe {
        task.didReceiveResponse(&response);
        task.didReceiveData(&NSData::with_bytes(said.body));
        task.didFinish();
    }
}

/// Which view a [`Web`] is: a live item's by its pane and item, and any by
/// its page's URL.
type Key = (Option<(Option<PaneId>, ItemId)>, String);

fn key(web: &Web) -> Key {
    (web.live.map(|l| (l.pane, l.item)), web.page.url.clone())
}

/// A view on screen and what it shows; a live item's in its clip, at its zoom.
struct Shown {
    key: Key,
    view: Retained<WKWebView>,
    _host: Retained<Host>,
    clip: Option<Retained<NSView>>,
    zoom: f64,
}

/// Where the rule list is.
enum Rules {
    Unasked,
    /// Compiling; these views wait to load.
    Pending(Vec<(Retained<WKWebView>, Retained<WKUserContentController>, String)>),
    Ready(Retained<WKContentRuleList>),
    /// It did not compile: no view loads.
    Failed,
}

thread_local! {
    static SHOWN: RefCell<Vec<Shown>> = const { RefCell::new(Vec::new()) };
    static RULES: RefCell<Rules> = const { RefCell::new(Rules::Unasked) };
    /// The theme the views were last given.
    static THEME: Cell<Option<Theme>> = const { Cell::new(None) };
}

/// Shows `webs` in `into`, a view `height` tall: each one already showing
/// that page (for a live item, that item in that pane) keeps its view (and
/// its state; a new `theme` changes its page's look in place), each new
/// one gets a new view below the chrome, a live item's just over the view
/// `canvas` gives for its pane, and the rest close.
pub fn place(
    into: &NSView,
    webs: &[Web],
    theme: &Theme,
    height: f64,
    canvas: impl Fn(PaneId) -> Option<Retained<NSView>>,
    mtm: MainThreadMarker,
) {
    let mut old = SHOWN.take();
    // A new theme reaches the open pages too: their look changes in place.
    let look = (THEME.replace(Some(*theme)) != Some(*theme)).then(|| ui::script(theme, ""));
    let mut now = Vec::new();
    for web in webs {
        let key = key(web);
        let kept = old.iter().position(|s| s.key == key).map(|at| old.remove(at));
        let mut shown = kept.unwrap_or_else(|| {
            let mut shown = open(&web.page, theme, key, mtm);
            let Some(live) = web.live else {
                into.addSubview_positioned_relativeTo(
                    &shown.view,
                    NSWindowOrderingMode::Below,
                    None,
                );
                return shown;
            };
            let clip = NSView::initWithFrame(NSView::alloc(mtm), super::NSRect::ZERO);
            clip.setClipsToBounds(true);
            clip.addSubview(&shown.view);
            let under = live.pane.and_then(&canvas);
            into.addSubview_positioned_relativeTo(
                &clip,
                NSWindowOrderingMode::Above,
                under.as_deref(),
            );
            shown.clip = Some(clip);
            shown
        });
        match (&shown.clip, web.live) {
            (Some(clip), Some(live)) => {
                clip.setHidden(live.shows == Shows::Away);
                clip.setFrame(flipped(height, live.clip));
                let (x, y) = (web.frame.x - live.clip.x, web.frame.y - live.clip.y);
                shown.view.setFrame(flipped(live.clip.height, Rect { x, y, ..web.frame }));
                if (shown.zoom - live.zoom).abs() > f64::EPSILON {
                    // SAFETY: a setter on a view of ours, on the main thread.
                    unsafe { shown.view.setPageZoom(live.zoom) };
                    shown.zoom = live.zoom;
                }
            }
            _ => shown.view.setFrame(flipped(height, web.frame)),
        }
        if let Some(look) = &look {
            // SAFETY: a script run in a view of ours, on the main thread.
            unsafe {
                shown.view.evaluateJavaScript_completionHandler(&NSString::from_str(look), None);
            }
        }
        now.push(shown);
    }
    for gone in old {
        match gone.clip {
            Some(clip) => clip.removeFromSuperview(),
            None => gone.view.removeFromSuperview(),
        }
    }
    SHOWN.set(now);
}

/// A new view for `page`, sandboxed, with `theme`'s look ([`ui::script`])
/// and no background of its own, so no white box shows before the page
/// paints; it loads once the rule list is on it.
fn open(page: &Arc<Page>, theme: &Theme, key: Key, mtm: MainThreadMarker) -> Shown {
    let host = Host::new(Arc::clone(page), mtm);
    let me = ProtocolObject::from_ref(&*host);
    // SAFETY: setting up a fresh configuration on the main thread, with the
    // host alive as long as the view (`Shown` keeps both).
    let view = unsafe {
        let config = WKWebViewConfiguration::new(mtm);
        config.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
        config.setURLSchemeHandler_forURLScheme(Some(me), &NSString::from_str(ui::SCHEME));
        config.preferences().setJavaScriptCanOpenWindowsAutomatically(false);
        let content = config.userContentController();
        let shim = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
            WKUserScript::alloc(mtm),
            &NSString::from_str(&ui::script(theme, ui::SHIM)),
            WKUserScriptInjectionTime::AtDocumentStart,
            true,
        );
        content.addUserScript(&shim);
        content.addScriptMessageHandlerWithReply_contentWorld_name(
            ProtocolObject::from_ref(&*host),
            &WKContentWorld::pageWorld(mtm),
            &NSString::from_str(ui::HANDLER),
        );
        let view = WKWebView::initWithFrame_configuration(
            WKWebView::alloc(mtm),
            super::NSRect::ZERO,
            &config,
        );
        view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*host)));
        view.setUIDelegate(Some(ProtocolObject::from_ref(&*host)));
        let clear = NSNumber::new_bool(false);
        let draws = NSString::from_str("drawsBackground");
        let _: () = msg_send![&view, setValue: &*clear, forKey: &*draws];
        view
    };
    // SAFETY: the view's own controller, which it shares with its configuration.
    let content = unsafe { view.configuration().userContentController() };
    when_ruled(view.clone(), content, page.url.clone(), mtm);
    Shown { key, view, _host: host, clip: None, zoom: 1.0 }
}

/// Loads `url` in `view` once the rule list is on its `content`.
fn when_ruled(
    view: Retained<WKWebView>,
    content: Retained<WKUserContentController>,
    url: String,
    mtm: MainThreadMarker,
) {
    let compile = RULES.with_borrow_mut(|rules| match rules {
        Rules::Ready(list) => {
            load(&view, &content, list, &url);
            false
        }
        Rules::Pending(waiting) => {
            waiting.push((view, content, url));
            false
        }
        Rules::Failed => false,
        Rules::Unasked => {
            *rules = Rules::Pending(vec![(view, content, url)]);
            true
        }
    });
    if compile {
        start_compile(mtm);
    }
}

/// Compiles the rule list, then loads every view that waits for it.
fn start_compile(mtm: MainThreadMarker) {
    let done = RcBlock::new(|list: *mut WKContentRuleList, error: *mut NSError| {
        // SAFETY: WebKit hands a valid list or a valid error, or null.
        let (list, error) = unsafe { (Retained::retain(list), Retained::retain(error)) };
        let waiting = RULES.with_borrow_mut(|rules| {
            let waiting = match std::mem::replace(rules, Rules::Failed) {
                Rules::Pending(waiting) => waiting,
                _ => Vec::new(),
            };
            if let Some(list) = list {
                for (view, content, url) in &waiting {
                    load(view, content, &list, url);
                }
                *rules = Rules::Ready(list);
            }
            waiting
        });
        if let Some(error) = error.filter(|_| !waiting.is_empty()) {
            // The views stay blank: nothing loads without the rules.
            cannot_start(&error.localizedDescription().to_string());
        }
    });
    // SAFETY: the store and its compile, on the main thread; the block is
    // copied by WebKit and called once on the main thread.
    unsafe {
        let Some(store) = WKContentRuleListStore::defaultStore(mtm) else {
            RULES.set(Rules::Failed);
            cannot_start("no rule list store");
            return;
        };
        store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
            Some(&NSString::from_str(ui::RULES_ID)),
            Some(&NSString::from_str(ui::RULES)),
            Some(&done),
        );
    }
}

fn load(view: &WKWebView, content: &WKUserContentController, list: &WKContentRuleList, url: &str) {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else { return };
    // SAFETY: adding the list and starting the load, on the main thread.
    unsafe {
        content.addContentRuleList(list);
        let _ = view.loadRequest(&NSURLRequest::requestWithURL(&url));
    }
}

/// Says on stderr why no agent UI can load.
fn cannot_start(why: &str) {
    // Nothing can be done if stderr is closed too.
    let _ = writeln!(std::io::stderr(), "✗ agent UIs cannot start: {why}");
}
