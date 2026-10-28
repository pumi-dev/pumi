//! What a page's wry view is to the app, where wry hangs it in a child window of its own and does
//! everything the app asks of it (Windows' WebView2, Linux's WebKitGTK): what is left to say here
//! is shared, and the OS file under `platform` says the rest (the window it hangs in, its shape).
//!
//! macOS is not here: its page is a view of the app's own, so `macos.rs` has its whole surface.
use crate::ui::webview::{Edit, Nav, Parent, State, Step};
use gpui::{Bounds, Pixels, Size};
use std::cell::Cell;
use wry::WebView;
use wry::dpi::{LogicalPosition, LogicalSize};
pub(crate) struct Surface {
    focused: Cell<bool>,
    /// Where the page is in its history, as its script last reported it (`Nav`).
    nav: Cell<Option<(bool, bool)>>,
}
impl Surface {
    pub fn new(_: &WebView, _: &Parent) -> Self {
        Self { focused: Cell::new(false), nav: Cell::new(None) }
    }
    /// The page's child window can't be hung in another window: it is built again there.
    pub fn moved(&mut self, _: &WebView, _: &Parent) -> bool {
        false
    }
    pub fn attached(&self, _: &WebView) -> bool {
        true
    }
    pub fn set_visible(&self, w: &WebView, visible: bool) {
        if !visible {
            self.release(w);
        }
        let _ = w.set_visible(visible);
    }
    pub fn frame(&self, w: &WebView, bounds: Bounds<Pixels>) {
        let (x, y) = (f64::from(f32::from(bounds.origin.x)), f64::from(f32::from(bounds.origin.y)));
        let (width, height) = (f64::from(f32::from(bounds.size.width)), f64::from(f32::from(bounds.size.height)));
        let _ = w.set_bounds(wry::Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(width, height).into(),
        });
    }
    pub fn clip(&self, w: &WebView, size: Size<Pixels>, radius: Pixels, holes: &[(Bounds<Pixels>, Pixels)]) {
        super::clip(w, size, radius, holes);
    }
    /// The page's history as its own script last told it: where the engine has no Navigation API
    /// to ask (WebKitGTK), the script says nothing and the buttons stay enabled.
    pub fn nav(&self, nav: Nav) {
        self.nav.set(Some((nav.back.unwrap_or(true), nav.forward.unwrap_or(true))));
    }
    /// What the app shows about the page. Its history is the script's word (`nav`); loading and
    /// progress are `None`: the page-load handler has already said them, and `Webview` keeps them.
    pub fn state(&self, _: &WebView) -> State {
        let (back, forward) = self.nav.get().unwrap_or((true, true));
        State { back, forward, loading: None, progress: None }
    }
    pub fn focus(&self, w: &WebView) {
        if !self.focused.replace(true) {
            let _ = w.focus();
        }
    }
    /// Gives the keyboard back to the app's own window, which the page's child window took it from.
    pub fn release(&self, w: &WebView) {
        if self.focused.replace(false) {
            let _ = w.focus_parent();
        }
    }
    pub fn step(&self, w: &WebView, step: Step) {
        let _ = match step {
            Step::Back => w.evaluate_script("history.back()"),
            Step::Forward => w.evaluate_script("history.forward()"),
            Step::Stop => w.evaluate_script("window.stop()"),
            Step::HardReload => w.reload(),
        };
    }
    pub fn edit(&self, w: &WebView, edit: Edit) {
        let command = match edit {
            Edit::Copy => "copy",
            Edit::Cut => "cut",
            Edit::SelectAll => "selectAll",
            Edit::Undo => "undo",
            Edit::Redo => "redo",
            Edit::Paste => return,
        };
        let _ = w.evaluate_script(&format!("document.execCommand('{command}')"));
    }
    pub fn dock_inspector(&self, _: &WebView) -> bool {
        true
    }
    /// A picture of the page: only macOS's WKWebView takes one for now.
    pub fn snapshot(&self, _: &WebView, _: &str, done: Box<dyn FnOnce(Result<(), String>)>) {
        done(Err("a picture of a web page needs macOS for now".into()));
    }
}
