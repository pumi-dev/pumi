//! Web pages on Windows: WebView2, whose runtime is a program of its own that the app does not
//! ship, in a child window of the app's own window.
use super::Parent;
use gpui::{App, Bounds, Pixels, Size, Window};
use std::path::PathBuf;
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};
/// What Windows and Linux pages share; macOS keeps its own whole surface in `macos.rs`.
#[path = "surface.rs"]
mod surface;
pub(super) use surface::Surface;
/// Where to get what Windows 10 does not come with.
const RUNTIME: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
pub(super) fn parent(window: &Window) -> Result<Parent, String> {
    let raw = HasWindowHandle::window_handle(window).map(|h| h.as_raw()).map_err(|e| e.to_string())?;
    match raw {
        RawWindowHandle::Win32(_) => Ok(Parent(raw)),
        _ => Err("this window has no Win32 handle".into()),
    }
}
pub(super) fn prepare(_: &mut App) -> Result<(), String> {
    Ok(())
}
/// What the app's pages share in wry: WebView2 keeps its profiles in a folder it must be able to
/// write, which one inside Program Files may not be, so each user has one of their own.
pub(super) struct Context(WebContext);
impl Context {
    pub(super) fn new() -> Self {
        let app = pumi_proto::paths::appname();
        let data = pumi_sys::paths::dir(pumi_sys::paths::Dir::Server, &app).join("webview");
        Self(WebContext::new(Some(data)))
    }
    /// A page's builder over this context, serving the app's scheme as `https`: WebView2 has none
    /// of its own to serve it as.
    pub(super) fn builder(&mut self) -> WebViewBuilder<'_> {
        WebViewBuilder::new_with_web_context(&mut self.0).with_https_scheme(true)
    }
}
/// What the tab says when a page does not build. Without the WebView2 runtime (not part of
/// Windows 10, where Edge came before it) nothing builds, and wry's own error is a bare "file not
/// found": say where to get the runtime instead.
pub(super) fn build_failure(error: &wry::Error) -> String {
    match runtime() {
        true => format!("web page: {error}"),
        false => format!("Web pages need the Microsoft Edge WebView2 Runtime: {RUNTIME}"),
    }
}
/// Whether an Edge WebView2 runtime is installed for this machine or this user: a version folder
/// with the browser in it, under the program files the installer writes to.
fn runtime() -> bool {
    let exe = |version: std::path::PathBuf| version.join("msedgewebview2.exe").is_file();
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"].iter().filter_map(std::env::var_os).any(|base| {
        let app = PathBuf::from(base).join("Microsoft").join("EdgeWebView").join("Application");
        std::fs::read_dir(app).is_ok_and(|versions| versions.flatten().any(|version| exe(version.path())))
    })
}
/// Cuts the page's window to its rounded box minus `holes` (page-local, each with its radius): a
/// window region, which cuts its input too, so a press in a hole reaches the app.
pub(super) fn clip(w: &WebView, size: Size<Pixels>, radius: Pixels, holes: &[(Bounds<Pixels>, Pixels)]) {
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, HRGN, RGN_DIFF, SetWindowRgn,
    };
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    let hwnd = w.hwnd();
    unsafe {
        if radius <= Pixels::ZERO && holes.is_empty() {
            SetWindowRgn(hwnd, None, true);
            return;
        }
        let scale = match GetDpiForWindow(hwnd) {
            0 => 1.,
            dpi => dpi as f32 / 96.,
        };
        let px = |p: Pixels| (f32::from(p) * scale).round() as i32;
        let region = |b: Bounds<Pixels>, r: Pixels| -> HRGN {
            let (left, top, right, bottom) = (px(b.left()), px(b.top()), px(b.right()), px(b.bottom()));
            let d = px(r).min((right - left) / 2).min((bottom - top) / 2).max(0) * 2;
            // A round rect region leaves out its right and bottom edges.
            match d {
                0 => CreateRectRgn(left, top, right, bottom),
                d => CreateRoundRectRgn(left, top, right + 1, bottom + 1, d, d),
            }
        };
        let shape = region(Bounds::new(Default::default(), size), radius);
        for &(hole, r) in holes.iter().filter(|(hole, _)| !hole.is_empty()) {
            let cut = region(hole, r);
            CombineRgn(Some(shape), Some(shape), Some(cut), RGN_DIFF);
            let _ = DeleteObject(cut.into());
        }
        // The window owns the region once it takes it.
        if SetWindowRgn(hwnd, Some(shape), true) == 0 {
            let _ = DeleteObject(shape.into());
        }
    }
}
