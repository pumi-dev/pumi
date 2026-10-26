//! Web pages on Linux: WebKitGTK in a GTK window of wry's own, inside the app's X window (GTK has
//! no Wayland window to hang a child in yet, so pages need X11 or XWayland).
use super::Parent;
use gpui::{App, Bounds, Pixels, Size, Window};
use std::cell::Cell;
use std::time::Duration;
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::{WebView, WebViewBuilder};
/// What Windows and Linux pages share; macOS keeps its own whole surface in `macos.rs`.
#[path = "surface.rs"]
mod surface;
pub(super) use surface::Surface;
pub(super) fn parent(window: &Window) -> Result<Parent, String> {
    let raw = HasWindowHandle::window_handle(window).map(|h| h.as_raw()).map_err(|e| e.to_string())?;
    match raw {
        RawWindowHandle::Xlib(_) => Ok(Parent(raw)),
        RawWindowHandle::Xcb(h) => {
            let xlib = wry::raw_window_handle::XlibWindowHandle::new(std::ffi::c_ulong::from(h.window.get()));
            Ok(Parent(RawWindowHandle::Xlib(xlib)))
        }
        RawWindowHandle::Wayland(_) => {
            Err("web pages need X11: pumi starts on XWayland by default, and native Wayland (PUMI_WAYLAND=1) has no web pages yet".into())
        }
        _ => Err("this window has no X11 handle".into()),
    }
}
/// Starts GTK on X11 alone, and pumps it from GPUI's foreground: wry's pages are GTK widgets, and
/// GTK's own main loop never runs here.
pub(super) fn prepare(cx: &mut App) -> Result<(), String> {
    thread_local! {
        static STARTED: Cell<Option<bool>> = const { Cell::new(None) };
    }
    if let Some(ok) = STARTED.get() {
        return if ok { Ok(()) } else { Err("GTK did not start".into()) };
    }
    gtk::gdk::set_allowed_backends("x11");
    let ok = gtk::init().is_ok();
    STARTED.set(Some(ok));
    if !ok {
        return Err("GTK did not start (is DISPLAY set?)".into());
    }
    let timer = cx.background_executor().clone();
    cx.foreground_executor()
        .spawn(async move {
            loop {
                timer.timer(Duration::from_millis(8)).await;
                for _ in 0..64 {
                    if !gtk::events_pending() {
                        break;
                    }
                    gtk::main_iteration_do(false);
                }
            }
        })
        .detach();
    Ok(())
}
/// What a page's wry view is built over: nothing on GTK, where the builder is the page's own.
pub(super) struct Context;
impl Context {
    pub(super) fn new() -> Self {
        Self
    }
    pub(super) fn builder(&mut self) -> WebViewBuilder<'_> {
        WebViewBuilder::new()
    }
}
/// What the tab says when a page does not build: wry's error.
pub(super) fn build_failure(error: &wry::Error) -> String {
    format!("web page: {error}")
}
/// Cuts the page's window to its rounded box minus `holes` (page-local, each with its radius): an
/// X shape, and the same input shape, so a press in a hole reaches the app. X shapes are
/// rectangles: a rounded corner is stepped in a few bands (`bands`).
pub(super) fn clip(w: &WebView, size: Size<Pixels>, radius: Pixels, holes: &[(Bounds<Pixels>, Pixels)]) {
    use gtk::cairo::Region;
    use gtk::prelude::WidgetExt;
    use wry::WebViewExtUnix;
    // The page's widget sits in the window wry made for it in the parent's X window.
    let Some(window) = w.webview().toplevel() else { return };
    if radius <= Pixels::ZERO && holes.is_empty() {
        window.shape_combine_region(None);
        window.input_shape_combine_region(None);
        return;
    }
    let shape = Region::create_rectangles(&bands(Bounds::new(Default::default(), size), radius));
    for &(hole, r) in holes {
        for band in bands(hole, r) {
            let _ = shape.subtract_rectangle(&band);
        }
    }
    window.shape_combine_region(Some(&shape));
    window.input_shape_combine_region(Some(&shape));
}
/// `b` rounded by `r` as rectangles X can shape a window by: each corner stepped in up to four
/// bands, the curve's inset at each band's middle row.
fn bands(b: Bounds<Pixels>, r: Pixels) -> Vec<gtk::cairo::RectangleInt> {
    use gtk::cairo::RectangleInt;
    let round = |p: Pixels| f32::from(p).round() as i32;
    let (x, y, w, h) = (round(b.origin.x), round(b.origin.y), round(b.size.width), round(b.size.height));
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    // Whole pixels, at most half the box, so the middle band is never negative.
    let ri = (f32::from(r).max(0.) as i32).min(w / 2).min(h / 2);
    let r = ri as f32;
    let mut out = vec![RectangleInt::new(x, y + ri, w, h - 2 * ri)];
    let n = ri.min(4);
    for k in 0..n {
        let (top, bottom) = (ri * k / n, ri * (k + 1) / n);
        if bottom <= top {
            continue;
        }
        let d = r - (top + bottom) as f32 / 2.;
        let inset = (r - (r * r - d * d).max(0.).sqrt()).round() as i32;
        let width = w - 2 * inset;
        out.push(RectangleInt::new(x + inset, y + top, width, bottom - top));
        out.push(RectangleInt::new(x + inset, y + h - bottom, width, bottom - top));
    }
    out
}
