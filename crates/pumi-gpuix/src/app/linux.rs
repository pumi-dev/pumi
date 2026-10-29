//! X11 or Wayland, embedded: `gpui_linux::current_embedded_platform`, its calloop loop dispatched
//! once per pump without waiting.
use std::rc::Rc;
use std::sync::Arc;

use crate::error::Result;

pub(super) struct Platform(Rc<dyn gpui_linux::EmbeddedPlatform>);

impl Platform {
    pub(super) fn new() -> Result<Self> {
        // XWayland unless `PUMI_WAYLAND=1`: web tabs (WebKitGTK through GTK) need X11, and
        // GNOME's Wayland draws no title bars. GPUI and GTK both pick Wayland while
        // `WAYLAND_DISPLAY` is set.
        let display = std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
        if display && std::env::var_os("PUMI_WAYLAND").as_deref() != Some(std::ffi::OsStr::new("1")) {
            std::env::remove_var("WAYLAND_DISPLAY");
        }
        Ok(Self(gpui_linux::current_embedded_platform(false)))
    }

    pub(super) fn gpui(&self) -> Rc<dyn gpui::Platform> {
        self.0.clone()
    }

    /// Dispatches the event loop's ready events without waiting. False once the app has quit.
    pub(super) fn pump(&self) -> bool {
        self.0.pump_events()
    }
}

/// Nothing wakes JS when a frame is due: it pumps on its own timer.
pub(super) const FRAME_WAKE: bool = false;

pub(super) fn set_frame_wake_hook(_hook: Option<Arc<dyn Fn() + Send + Sync>>) {}
