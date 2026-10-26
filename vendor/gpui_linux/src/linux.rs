mod dispatcher;
mod headless;
mod keyboard;
mod platform;
mod system_notifications;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod text_system;
#[cfg(feature = "wayland")]
mod wayland;
#[cfg(feature = "x11")]
mod x11;

#[cfg(any(feature = "wayland", feature = "x11"))]
mod xdg_desktop_portal;

pub use dispatcher::*;
pub(crate) use headless::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) use text_system::*;
#[cfg(feature = "wayland")]
pub(crate) use wayland::*;
#[cfg(feature = "x11")]
pub(crate) use x11::*;

use std::cell::Cell;
use std::rc::Rc;

// pumi
pub use platform::EmbeddedPlatform;

/// Returns the default platform implementation for the current OS.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    new_platform(headless, false)
}

// pumi: `current_platform` for a host that owns the main loop.
/// Returns the default platform implementation for the current OS, driven by the caller's loop
/// through [`EmbeddedPlatform::pump_events`].
pub fn current_embedded_platform(headless: bool) -> Rc<dyn EmbeddedPlatform> {
    new_platform(headless, true)
}

fn new_platform(headless: bool, embedded: bool) -> Rc<dyn EmbeddedPlatform> {
    #[cfg(feature = "x11")]
    use anyhow::Context as _;

    fn platform<P: LinuxClient + 'static>(inner: P, embedded: bool) -> Rc<dyn EmbeddedPlatform> {
        Rc::new(LinuxPlatform {
            inner,
            embedded,
            quit_requested: Cell::new(false),
        })
    }

    if headless {
        return platform(HeadlessClient::new(), embedded);
    }

    match gpui::guess_compositor() {
        #[cfg(feature = "wayland")]
        "Wayland" => platform(WaylandClient::new(), embedded),

        #[cfg(feature = "x11")]
        "X11" => platform(
            X11Client::new()
                .context("Failed to initialize X11 client.")
                .unwrap(),
            embedded,
        ),

        "Headless" => platform(HeadlessClient::new(), embedded),
        _ => unreachable!(
            r#"At least one of the "wayland" or "x11" features must be enabled on gpui_linux or gpui_platform."#
        ),
    }
}
