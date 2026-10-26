//! AppKit, embedded: `MacPlatform::new_embedded`, pumped with `pump_events`.
use std::rc::Rc;
use std::sync::Arc;

use napi::bindgen_prelude::*;

pub(super) struct Platform(Rc<gpui_macos::MacPlatform>);

impl Platform {
    pub(super) fn new() -> Result<Self> {
        input_wake::start();
        Ok(Self(Rc::new(gpui_macos::MacPlatform::new_embedded())))
    }

    pub(super) fn gpui(&self) -> Rc<dyn gpui::Platform> {
        self.0.clone()
    }

    /// Pumps AppKit once without blocking. False once the app has quit.
    pub(super) fn pump(&self) -> bool {
        self.0.pump_events()
    }
}

/// The display link wakes JS each frame.
pub(super) const FRAME_WAKE: bool = true;

pub(super) fn set_frame_wake_hook(hook: Option<Arc<dyn Fn() + Send + Sync>>) {
    gpui_macos::set_frame_wake_hook(hook);
}

/// Input reaches JS when it happens, not at the next display frame.
///
/// AppKit is pumped from the JS thread, and nothing wakes that thread when an event arrives:
/// the display link does, once a frame. So a key pressed just after a frame waited for the next
/// one before the terminal even saw it, and its echo drew a frame after that: one frame more
/// than a native app, on every keystroke. A thread here reads, every millisecond while a window
/// is key, when the system last saw a key, a button or a wheel (a shared-memory read, not an
/// event tap: no permission, nothing intercepted) and wakes JS when that moved. AppKit may not
/// have the event queued yet when the system has it, so it wakes a few times more after.
mod input_wake {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::super::ACTIVE;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    /// `kCGEventSourceStateHIDSystemState`.
    const HID_SYSTEM: i32 = 1;
    /// Key down and up, modifiers, the buttons down and up, the wheel.
    const KINDS: [u32; 9] = [10, 11, 12, 1, 2, 3, 4, 25, 22];
    /// Wakes after a new event: now, and over the next milliseconds while AppKit gets it.
    const WAKES: u8 = 4;

    pub(super) fn start() {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if STARTED.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = std::thread::Builder::new().name("pumi-input-wake".into()).spawn(|| {
            let mut last = [f64::MAX; KINDS.len()];
            let mut wakes = 0u8;
            loop {
                if ACTIVE.load(Ordering::Relaxed) == 0 {
                    last = [f64::MAX; KINDS.len()];
                    wakes = 0;
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                std::thread::sleep(Duration::from_millis(1));
                let mut fresh = false;
                for (kind, last) in KINDS.iter().zip(last.iter_mut()) {
                    // SAFETY: a pure query of the system's event state, callable from any thread.
                    let since = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM, *kind) };
                    // It grows with the time; an event sets it back to about zero.
                    fresh |= since + 0.0005 < *last;
                    *last = since;
                }
                if fresh {
                    wakes = WAKES;
                }
                if wakes > 0 {
                    wakes -= 1;
                    crate::frame_loop::request(crate::frame_loop::Source::Input);
                }
            }
        });
    }
}
