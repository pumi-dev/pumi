//! The blur of what is behind a window that is not opaque: the window server's own, the one
//! Terminal.app and Ghostty use, at a radius the app picks (GPUI's `Blurred` is AppKit's vibrancy,
//! at none it can set).

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

/// Blurs what shows through `window` by `radius` points; 0 takes the blur away. Only a window
/// whose background is not opaque shows it.
pub fn set_blur(window: &gpui::Window, radius: f64) -> Result<(), String> {
    os::set_blur(window, radius)
}

/// How much of what is behind `window` its backgrounds hide (0–1, 1 is opaque): GPUI's Metal
/// renderer (pumi's, `vendor/gpui_apple`) reads it from the window's layer each frame.
pub fn set_glass(window: &gpui::Window, opacity: f64) -> Result<(), String> {
    os::set_glass(window, opacity)
}
