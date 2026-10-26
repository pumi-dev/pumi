#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::current_platform;
// pumi
pub use linux::{EmbeddedPlatform, current_embedded_platform};
