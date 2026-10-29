//! The Node addon's side of the renderer: the JS classes and functions over the plain-Rust
//! renderer (`crate::Renderer`). It is the only part of the crate that knows about JS, and it
//! builds only with the `napi` feature.
use napi_derive::napi;

mod events;
mod renderer;
// gpui's VisualTestAppContext, what the test renderer draws with, is macOS's and Windows' only.
#[cfg(all(feature = "test-support", any(target_os = "macos", target_os = "windows")))]
#[path = "test_renderer.rs"]
mod test_renderer;

/// An element id as JS has it (a number), checked.
pub(crate) fn to_element_id(id: f64) -> napi::Result<u64> {
    pumi_ui_proto::element_id(id).map_err(napi::Error::from_reason)
}

/// An image's width or height as JS has it, checked: a whole number of pixels.
pub(crate) fn dimension_u32(value: f64, label: &str) -> napi::Result<u32> {
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 || value > u32::MAX as f64 {
        return Err(napi::Error::from_reason(format!("Image {label} must be a positive integer, got {value}")));
    }
    Ok(value as u32)
}

/// The renderer's error as JS's.
pub(crate) fn js(error: crate::Error) -> napi::Error {
    napi::Error::from_reason(error.reason)
}

impl From<crate::Error> for napi::Error {
    fn from(error: crate::Error) -> Self {
        js(error)
    }
}

/// True when this binary has the GPU test renderer: built with the `test-support` feature, on
/// macOS or Windows.
#[napi]
pub fn has_test_gpuix_renderer() -> bool {
    cfg!(all(feature = "test-support", any(target_os = "macos", target_os = "windows")))
}

/// Sets the chords (`shift+cmd+p`) the window takes for the app's bindings: such a key reaches
/// `windowKeyDown` (with `captured`) and no element. `all` takes every key, for the rest of a
/// sequence.
#[napi]
pub fn set_captured_keys(chords: Vec<String>, all: bool) {
    crate::set_captured_keys(chords, all)
}
