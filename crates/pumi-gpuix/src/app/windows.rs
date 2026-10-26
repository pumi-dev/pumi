//! Win32, embedded: `WindowsPlatform::new_embedded`, pumped with `pump_events`.
use std::rc::Rc;
use std::sync::Arc;

use napi::bindgen_prelude::*;

pub(super) struct Platform(Rc<gpui_windows::WindowsPlatform>);

impl Platform {
    pub(super) fn new() -> Result<Self> {
        let platform = gpui_windows::WindowsPlatform::new_embedded(false)
            .map_err(|error| Error::from_reason(format!("Failed to start the Windows platform: {error:#}")))?;
        Ok(Self(Rc::new(platform)))
    }

    pub(super) fn gpui(&self) -> Rc<dyn gpui::Platform> {
        self.0.clone()
    }

    /// Dispatches the thread's queued messages without waiting. False once the app has quit.
    pub(super) fn pump(&self) -> bool {
        self.0.pump_events()
    }
}

/// Nothing wakes JS when a frame is due: it pumps on its own timer.
pub(super) const FRAME_WAKE: bool = false;

pub(super) fn set_frame_wake_hook(_hook: Option<Arc<dyn Fn() + Send + Sync>>) {}
