//! The process's one GPUI application, pumped from the JS thread.
//!
//! Every window (`GpuixRenderer`) opens in it. The app starts with the first window and runs until
//! JS quits it: closing the last window does not end it. Crates linked into the addon add their
//! own setup with [`on_start`] (keymaps, elements, menus) before the first window opens.
//!
//! The platform is the OS file's (`os::Platform`): GPUI's, made to return from `run` and pumped
//! once per [`tick`] without blocking, since the JS thread owns the main loop.
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;

type Start = Box<dyn Fn(&mut gpui::App) + Send + Sync>;

static STARTS: Mutex<Vec<Start>> = Mutex::new(Vec::new());

#[cfg(target_os = "macos")]
#[path = "app/macos.rs"]
mod os;
#[cfg(target_os = "windows")]
#[path = "app/windows.rs"]
mod os;
#[cfg(target_os = "linux")]
#[path = "app/linux.rs"]
mod os;

struct Running {
    platform: os::Platform,
    handle: gpui::ApplicationHandle,
}

thread_local! {
    // Never dropped: the app lives until the process ends, and dropping GPUI's app as the main
    // thread's locals are torn down reads other thread-locals already gone (an abort on exit).
    static APP: std::mem::ManuallyDrop<RefCell<Option<Running>>> = const { std::mem::ManuallyDrop::new(RefCell::new(None)) };
}

/// Runs `f` once the app starts (or now, inside the next [`update`], if it already has).
pub fn on_start(f: impl Fn(&mut gpui::App) + Send + Sync + 'static) {
    if running() {
        let _ = update(|cx| f(cx));
        return;
    }
    STARTS.lock().unwrap().push(Box::new(f));
}

pub fn running() -> bool {
    APP.with(|app| app.borrow().is_some())
}

/// What `svg().path("icons/…")` loads from: the host's embedded assets. Set before `ensure`.
static ASSETS: Mutex<Option<Arc<dyn gpui::AssetSource>>> = Mutex::new(None);

pub fn set_assets(source: impl gpui::AssetSource) {
    *ASSETS.lock().unwrap() = Some(Arc::new(source));
}

struct Assets(Arc<dyn gpui::AssetSource>);
impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        self.0.load(path)
    }
    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        self.0.list(path)
    }
}

/// What runs when the app is opened again while it runs (its Dock icon clicked).
static REOPEN: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);
pub fn on_reopen(hook: impl Fn() + Send + Sync + 'static) {
    *REOPEN.lock().unwrap() = Some(Arc::new(hook));
}

/// What runs when macOS asks the app to open URLs: `file://` ones for files and folders dropped
/// on its Dock icon or opened with it ("Open With", `open -a`), the app's own schemes' others.
type OpenUrls = Arc<dyn Fn(Vec<String>) + Send + Sync>;
static OPEN_URLS: Mutex<Option<OpenUrls>> = Mutex::new(None);
pub fn on_open_urls(hook: impl Fn(Vec<String>) + Send + Sync + 'static) {
    *OPEN_URLS.lock().unwrap() = Some(Arc::new(hook));
}

/// The name the app was started with: the first menu of the bar, and the About panel's title.
static NAME: Mutex<Option<String>> = Mutex::new(None);

pub fn app_name() -> String {
    NAME.lock().unwrap().clone().unwrap_or_else(|| "pumi".to_string())
}

/// Starts the app if it isn't running. `app_name` names the menu bar's first menu.
pub fn ensure(app_name: &str) -> Result<()> {
    if running() {
        return Ok(());
    }
    *NAME.lock().unwrap() = Some(app_name.to_string());
    let platform = os::Platform::new()?;
    let name = app_name.to_string();
    let mut application = gpui::Application::with_platform(platform.gpui()).with_quit_mode(gpui::QuitMode::Explicit);
    if let Some(assets) = ASSETS.lock().unwrap().clone() {
        application = application.with_assets(Assets(assets));
    }
    application.on_reopen(|_| {
        let hook = REOPEN.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook();
        }
    });
    application.on_open_urls(|urls| {
        let hook = OPEN_URLS.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook(urls);
        }
    });
    let handle = application.run_embedded(move |cx: &mut gpui::App| {
        crate::custom_elements::input::init(cx);
        // Before the addon's own setup, which may replace this bar with the app's full one
        // (`App::set_menus` again, after the addon's `on_start` has run).
        crate::app_menu::init(&name, cx);
        for start in STARTS.lock().unwrap().iter() {
            start(cx);
        }
    });
    APP.with(|app| *app.borrow_mut() = Some(Running { platform, handle }));
    Ok(())
}

/// Runs `f` with the app. Errors when it isn't running.
pub fn update<R>(f: impl FnOnce(&mut gpui::App) -> R) -> Result<R> {
    APP.with(|app| {
        let app = app.borrow();
        let app = app.as_ref().ok_or_else(|| Error::from_reason("The GPUI application is not running"))?;
        Ok(app.handle.update(f))
    })
}

/// Pumps the platform once without blocking. False once the app has quit.
pub(crate) fn tick() -> bool {
    crate::frame_loop::pump(|| APP.with(|app| app.borrow().as_ref().map(|app| app.platform.pump()).unwrap_or(false)))
}

/// Whether a wake reaches JS when a frame is due (macOS's display link). Elsewhere
/// `setFrameWakeCallback` says no and JS pumps on its own timer.
pub(crate) const FRAME_WAKE: bool = os::FRAME_WAKE;

/// Sets (or clears) how a wake reaches JS: the display link's each frame, input's, and
/// [`wake`]'s all go through it, coalesced and counted ([`crate::frame_loop`]).
pub(crate) fn set_wake_hook(sender: Option<crate::frame_loop::Sender>) {
    let display = sender.is_some().then(|| {
        Arc::new(|| crate::frame_loop::request(crate::frame_loop::Source::Display)) as Arc<dyn Fn() + Send + Sync>
    });
    crate::frame_loop::set_sender(sender);
    os::set_frame_wake_hook(display);
}

/// Asks JS to pump the app soon, from any thread: work queued for the main thread (a spawned
/// task, a dispatched closure) runs then.
pub fn wake() {
    crate::frame_loop::request(crate::frame_loop::Source::Other);
}

/// How many of the app's windows are key: one, or none.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// How long JS's frame loop waits for a wake before it pumps anyway (`frameWakeFallbackMs`).
pub fn wake_fallback() -> std::time::Duration {
    crate::frame_loop::fallback(ACTIVE.load(Ordering::Relaxed) > 0)
}

/// A window became the key window (`true`) or stopped being it; each window says each change
/// once. While one is, JS waits less for a wake, and on macOS input wakes it at once.
pub(crate) fn set_active(active: bool) {
    let _ = ACTIVE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| Some(if active { n + 1 } else { n.saturating_sub(1) }));
}
