use super::{Edit, Nav, Parent, State, Step};
use gpui::{App, Bounds, Pixels, Size, Window};
use objc2::encode::{Encoding, RefEncode};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::{WebView, WebViewBuilder, WebViewExtMacOS};
/// CoreGraphics' path, by reference only.
#[repr(C)]
struct CGPath([u8; 0]);
unsafe impl RefEncode for CGPath {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGPath", &[]));
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPathCreateMutable() -> *const CGPath;
    fn CGPathCreateWithRoundedRect(rect: NSRect, width: f64, height: f64, transform: *const c_void) -> *const CGPath;
    fn CGPathCreateCopyBySubtractingPath(path: *const CGPath, mask: *const CGPath, even_odd: bool) -> *const CGPath;
    fn CGPathContainsPoint(path: *const CGPath, transform: *const c_void, point: NSPoint, even_odd: bool) -> bool;
    fn CGPathRelease(path: *const CGPath);
}
/// The page's shape in its container's coordinates, while it is not the whole box.
struct Shape(Cell<*const CGPath>);
impl Drop for Shape {
    fn drop(&mut self) {
        unsafe { CGPathRelease(self.0.get()) }
    }
}
define_class!(
    /// A page's container: a press outside the page's shape (in a hole the app draws over, or a
    /// rounded-off corner) is not the page's, and falls to the app's view beneath.
    #[unsafe(super(NSView))]
    #[ivars = Shape]
    struct Container;
    impl Container {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let path = self.ivars().0.get();
            if !path.is_null() {
                let local: NSPoint = unsafe {
                    let superview: *mut NSView = msg_send![self, superview];
                    msg_send![self, convertPoint: point, fromView: superview]
                };
                if !unsafe { CGPathContainsPoint(path, null(), local, true) } {
                    return null_mut();
                }
            }
            unsafe { msg_send![super(self), hitTest: point] }
        }
    }
);
/// GPUI's views' own `performKeyEquivalent:`, by class, before `page_first` wrapped it.
static KEY_EQUIVALENT: std::sync::Mutex<Vec<(usize, Imp)>> = std::sync::Mutex::new(Vec::new());

/// A ⌘ or ⌃ chord goes to the page with the keyboard before the app sees it, as in a browser.
///
/// AppKit offers such a chord as a key equivalent to the window's views and then to the menu bar,
/// and only then as a key down to the first responder. GPUI's view answers the key equivalent
/// without asking the views inside it, so a page would hear such a chord only after the app's
/// bindings, its own (`Webview::page_key`: ⌘Z as the native undo, ⌘[ as back) and the menu bar, the
/// system's services included (⇧⌘L searches the selection with Google), had all had it: Monaco
/// would never see ⌘Z, and ⌘D would split the tab. The page hears it first instead: what it takes
/// is its own, the app's chords it does not keep it hands back (`__pumiKeys`), and what it leaves
/// WebKit sends through the app again, past the page, to GPUI and the menu bar.
fn page_first(view: *mut AnyObject) {
    let Some(view) = (unsafe { view.as_ref() }) else { return };
    let class = view.class();
    let mut wrapped = KEY_EQUIVALENT.lock().unwrap();
    if wrapped.iter().any(|(c, _)| *c == class as *const AnyClass as usize) {
        return;
    }
    let Some(method) = class.instance_method(sel!(performKeyEquivalent:)) else { return };
    let ours: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool = key_equivalent;
    // SAFETY: the same signature as the method it replaces (`-(BOOL)performKeyEquivalent:(NSEvent *)`).
    let original = unsafe { method.set_implementation(std::mem::transmute::<_, Imp>(ours)) };
    wrapped.push((class as *const AnyClass as usize, original));
}

unsafe extern "C-unwind" fn key_equivalent(this: *mut AnyObject, cmd: Sel, event: *mut AnyObject) -> Bool {
    unsafe {
        let window: *mut AnyObject = msg_send![this, window];
        let first: *mut AnyObject = if window.is_null() { null_mut() } else { msg_send![window, firstResponder] };
        if !first.is_null()
            && first != this
            && msg_send![first, isKindOfClass: objc2::class!(WKWebView)]
            && msg_send![first, isDescendantOf: this]
        {
            // The page's first hearing answers yes, and WebKit resends what it leaves; a resent
            // chord WebKit answers no to, and GPUI has it. WKWebView's own method, past wry's
            // `WryWebView`, which answers no for a child webview without asking WebKit.
            let taken: Bool = msg_send![super(&*first, objc2::class!(WKWebView)), performKeyEquivalent: event];
            if taken.as_bool() {
                return Bool::YES;
            }
        }
        let class = (*this).class() as *const AnyClass as usize;
        let original = KEY_EQUIVALENT.lock().unwrap().iter().find(|(c, _)| *c == class).map(|(_, imp)| *imp);
        match original {
            Some(imp) => {
                let imp: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool =
                    std::mem::transmute(imp);
                imp(this, cmd, event)
            }
            None => Bool::NO,
        }
    }
}

pub(super) fn parent(window: &Window) -> Result<Parent, String> {
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(raw @ RawWindowHandle::AppKit(_)) => Ok(Parent(raw)),
        _ => Err("this window has no AppKit view".into()),
    }
}
pub(super) fn prepare(_: &mut App) -> Result<(), String> {
    Ok(())
}
/// What a window's pages share in wry: nothing on macOS, where a page's builder is its own.
pub(super) struct Context;
impl Context {
    pub(super) fn new() -> Self {
        Self
    }
    pub(super) fn builder(&mut self) -> WebViewBuilder<'_> {
        WebViewBuilder::new().with_user_agent(user_agent())
    }
}
/// What a page says it is: Safari's own user agent. A bare WKWebView's lacks Safari's
/// `Version/… Safari/…` tail, and sites read that as an old or unknown browser (Google serves its
/// legacy, no-JavaScript home page to it).
fn user_agent() -> &'static str {
    static AGENT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        // WebKit's prefix, frozen by Safari itself (the OS version and the build stay the same).
        format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{} Safari/605.1.15",
            safari_version().unwrap_or_else(|| "26.0".into())
        )
    })
}
/// The installed Safari's version, as its user agent has it: major.minor (`26.0`, not `26.0.1`).
fn safari_version() -> Option<String> {
    unsafe {
        let path = NSString::from_str("/Applications/Safari.app");
        let bundle: *mut AnyObject = msg_send![objc2::class!(NSBundle), bundleWithPath: &*path];
        let bundle = bundle.as_ref()?;
        let key = NSString::from_str("CFBundleShortVersionString");
        let version: *mut NSString = msg_send![bundle, objectForInfoDictionaryKey: &*key];
        let version = version.as_ref()?.to_string();
        let mut parts = version.split('.');
        let major = parts.next().filter(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit()))?;
        let minor = parts.next().filter(|m| !m.is_empty() && m.bytes().all(|b| b.is_ascii_digit())).unwrap_or("0");
        Some(format!("{major}.{minor}"))
    }
}
/// What the tab says when a page does not build: wry's error.
pub(super) fn build_failure(error: &wry::Error) -> String {
    format!("web page: {error}")
}
fn ns_view(parent: &Parent) -> *mut AnyObject {
    match parent.0 {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr() as *mut AnyObject,
        _ => std::ptr::null_mut(),
    }
}
fn wk(w: &WebView) -> Retained<wry::WryWebView> {
    w.webview()
}
pub(super) struct Surface {
    container: Retained<Container>,
    /// The container's layer's mask while the page is cut (`clip`).
    mask: Retained<AnyObject>,
}
impl Surface {
    pub fn new(w: &WebView, parent: &Parent) -> Self {
        let wk = wk(w);
        let parent = ns_view(parent);
        page_first(parent);
        // GPUI's windows, and so their pages, live on the main thread.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let container = Container::alloc(mtm).set_ivars(Shape(Cell::new(null())));
        let container: Retained<Container> = unsafe { msg_send![super(container), init] };
        unsafe {
            let mask: Retained<AnyObject> = msg_send![objc2::class!(CAShapeLayer), new];
            let _: () = msg_send![&*mask, setFillRule: &*NSString::from_str("even-odd")];
            let _: () = msg_send![&*container, setWantsLayer: true];
            let _: () = msg_send![&*container, setHidden: true];
            let _: () = msg_send![parent, addSubview: &*container];
            let _: () = msg_send![&*wk, removeFromSuperview];
            let _: () = msg_send![&*container, addSubview: &*wk];
            let _: () = msg_send![&*wk, setAutoresizingMask: 2usize | 16usize];
            let _: () = msg_send![&*wk, setHidden: false];
            Self { container, mask }
        }
    }
    pub fn moved(&mut self, _: &WebView, parent: &Parent) -> bool {
        self.hand_back();
        let parent = ns_view(parent);
        page_first(parent);
        unsafe {
            let _: () = msg_send![&*self.container, removeFromSuperview];
            let _: () = msg_send![parent, addSubview: &*self.container];
        }
        true
    }
    pub fn attached(&self, w: &WebView) -> bool {
        let window: *mut AnyObject = unsafe { msg_send![&*wk(w), window] };
        !window.is_null()
    }
    pub fn set_visible(&self, _: &WebView, visible: bool) {
        if !visible {
            self.hand_back();
        }
        unsafe {
            let _: () = msg_send![&*self.container, setHidden: !visible];
        }
    }
    pub fn frame(&self, w: &WebView, bounds: Bounds<Pixels>) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let wk = wk(w);
        let (x, y) = (f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)));
        let (width, h) = (f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height)));
        let docked = self.inspector_docked(w);
        unsafe {
            let parent: *mut AnyObject = msg_send![&*self.container, superview];
            if parent.is_null() {
                return;
            }
            let flipped: bool = msg_send![parent, isFlipped];
            let height = {
                let frame: NSRect = msg_send![parent, frame];
                frame.size.height
            };
            let y = if flipped { y } else { height - y - h };
            let _: () = msg_send![&*self.container, setFrame: NSRect::new(NSPoint::new(x, y), NSSize::new(width, h))];
            if !docked {
                let _: () = msg_send![&*wk, setFrame: NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, h))];
            }
        }
    }
    /// Masks the container to the page's rounded box minus `holes` (page-local, each with its
    /// radius), and hit-tests by the same shape. The holes are subtracted one by one rather than
    /// filled even-odd as they are, since they overlap (a hovered item inside its menu).
    pub fn clip(&self, _: &WebView, size: Size<Pixels>, radius: Pixels, holes: &[(Bounds<Pixels>, Pixels)]) {
        let f = |p: Pixels| f64::from(f32::from(p));
        let (width, height) = (f(size.width), f(size.height));
        // The container is not flipped: its y runs up from the bottom.
        let rounded = |b: Bounds<Pixels>, r: Pixels| {
            let (w, h) = (f(b.size.width), f(b.size.height));
            let r = f(r).min(w / 2.).min(h / 2.).max(0.);
            let rect = NSRect::new(NSPoint::new(f(b.origin.x), height - f(b.origin.y) - h), NSSize::new(w, h));
            unsafe { CGPathCreateWithRoundedRect(rect, r, r, null()) }
        };
        let path = match radius <= Pixels::ZERO && holes.is_empty() {
            true => null(),
            false => {
                let mut path = rounded(Bounds::new(Default::default(), size), radius);
                for &(hole, r) in holes.iter().filter(|(hole, _)| !hole.is_empty()) {
                    let cut = rounded(hole, r);
                    unsafe {
                        let rest = CGPathCreateCopyBySubtractingPath(path, cut, false);
                        CGPathRelease(cut);
                        CGPathRelease(path);
                        path = if rest.is_null() { CGPathCreateMutable() } else { rest };
                    }
                }
                path
            }
        };
        unsafe {
            let transaction = objc2::class!(CATransaction);
            let _: () = msg_send![transaction, begin];
            let _: () = msg_send![transaction, setDisableActions: true];
            let layer: *mut AnyObject = msg_send![&*self.container, layer];
            if path.is_null() {
                let _: () = msg_send![layer, setMask: null_mut::<AnyObject>()];
            } else {
                let window: *mut AnyObject = msg_send![&*self.container, window];
                if !window.is_null() {
                    let scale: f64 = msg_send![window, backingScaleFactor];
                    let _: () = msg_send![&*self.mask, setContentsScale: scale];
                }
                let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, height));
                let _: () = msg_send![&*self.mask, setFrame: frame];
                let _: () = msg_send![&*self.mask, setPath: path];
                let _: () = msg_send![layer, setMask: &*self.mask];
            }
            let _: () = msg_send![transaction, commit];
            CGPathRelease(self.container.ivars().0.replace(path));
        }
    }
    pub fn state(&self, w: &WebView) -> State {
        let wk = wk(w);
        unsafe {
            State {
                back: msg_send![&*wk, canGoBack],
                forward: msg_send![&*wk, canGoForward],
                loading: Some(msg_send![&*wk, isLoading]),
                progress: Some(msg_send![&*wk, estimatedProgress]),
            }
        }
    }
    /// WKWebView answers its own history (`state`): the page's script tells it nothing new.
    pub fn nav(&self, _: Nav) {}
    pub fn focus(&self, w: &WebView) {
        let wk = wk(w);
        let window: *mut AnyObject = unsafe { msg_send![&*wk, window] };
        let first: *mut AnyObject =
            if window.is_null() { window } else { unsafe { msg_send![window, firstResponder] } };
        if !window.is_null() && first != Retained::as_ptr(&wk) as *mut AnyObject {
            let _ = w.focus();
        }
    }
    pub fn release(&self, _: &WebView) {
        self.hand_back();
    }
    fn hand_back(&self) {
        let container = &self.container;
        unsafe {
            let window: *mut AnyObject = msg_send![&**container, window];
            if window.is_null() {
                return;
            }
            let first: *mut AnyObject = msg_send![window, firstResponder];
            if first.is_null() || !msg_send![first, isKindOfClass: objc2::class!(NSView)] {
                return;
            }
            if !msg_send![first, isDescendantOf: &**container] {
                return;
            }
            let gpui_view: *mut AnyObject = msg_send![&**container, superview];
            if !gpui_view.is_null() {
                let _: bool = msg_send![window, makeFirstResponder: gpui_view];
            }
        }
    }
    pub fn step(&self, w: &WebView, step: Step) {
        let selector = match step {
            Step::Back => sel!(goBack),
            Step::Forward => sel!(goForward),
            Step::Stop => sel!(stopLoading),
            Step::HardReload => sel!(reloadFromOrigin),
        };
        let _: *mut AnyObject = unsafe { msg_send![&*wk(w), performSelector: selector] };
    }
    pub fn edit(&self, w: &WebView, edit: Edit) {
        let selector: Sel = match edit {
            Edit::Copy => sel!(copy:),
            Edit::Cut => sel!(cut:),
            Edit::Paste => sel!(paste:),
            Edit::SelectAll => sel!(selectAll:),
            Edit::Undo => sel!(undo:),
            Edit::Redo => sel!(redo:),
        };
        let wk = wk(w);
        let _: bool = unsafe { msg_send![&*wk, tryToPerform: selector, with: &*wk] };
    }
    fn inspector(w: &WebView) -> Option<*mut AnyObject> {
        let inspector: *mut AnyObject = unsafe { msg_send![&*wk(w), _inspector] };
        (!inspector.is_null()).then_some(inspector)
    }
    fn inspector_docked(&self, w: &WebView) -> bool {
        let Some(inspector) = Self::inspector(w) else { return false };
        unsafe {
            if !msg_send![inspector, isVisible] {
                return false;
            }
            let view: *mut AnyObject = msg_send![inspector, inspectorWebView];
            if view.is_null() {
                return false;
            }
            let parent: *mut AnyObject = msg_send![view, superview];
            std::ptr::eq(parent.cast_const(), Retained::as_ptr(&self.container).cast())
        }
    }
    pub fn dock_inspector(&self, w: &WebView) -> bool {
        let Some(inspector) = Self::inspector(w) else { return false };
        unsafe {
            if !msg_send![inspector, isVisible] {
                return false;
            }
            if !self.inspector_docked(w) {
                let _: () = msg_send![inspector, attach];
            }
        }
        true
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        self.hand_back();
        unsafe {
            let _: () = msg_send![&*self.container, removeFromSuperview];
        }
    }
}
