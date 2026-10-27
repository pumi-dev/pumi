#![deny(clippy::all)]
//! pumi's renderer: a retained tree fed by React's commits, rebuilt into GPUI elements each frame.
//! Forked from gpuix (Apache-2.0).

mod accessibility;
pub mod app;
pub mod app_menu;
mod automation;
mod backdrop;
mod boundary;
mod color;
pub mod custom_elements;
mod decor;
mod element_tree;
pub mod frame_loop;
mod images;
mod motion;
pub mod occlusion;
mod renderer;
pub mod retained_tree;
mod schedule;
pub mod style;
mod text;
mod theme;
mod tooltip;

// gpui's VisualTestAppContext, what the test renderer draws with, is macOS's and Windows' only.
#[cfg(all(feature = "test-support", any(target_os = "macos", target_os = "windows")))]
#[path = "test_renderer.rs"]
mod test_renderer;

pub use element_tree::*;
pub use renderer::*;
pub use style::*;

/// True when this binary has the GPU test renderer: built with the `test-support` feature, on
/// macOS or Windows.
#[napi_derive::napi]
pub fn has_test_gpuix_renderer() -> bool {
    cfg!(all(feature = "test-support", any(target_os = "macos", target_os = "windows")))
}
