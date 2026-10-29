#![deny(clippy::all)]
//! pumi's renderer: a retained tree fed by a UI's commits (`pumi_ui_proto`'s ops), rebuilt into
//! GPUI elements each frame. Forked from gpuix (Apache-2.0).
//!
//! The renderer is plain Rust ([`Renderer`]); the `napi` feature adds the addon's JS classes
//! over it (`addon`), and is the only part that knows about JS.

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
pub mod error;
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

#[cfg(feature = "napi")]
mod addon;

pub use element_tree::*;
pub use error::Error;
pub use pumi_ui_proto as proto;
pub use renderer::*;
pub use style::*;
