//! What pumi's processes ask of the OS, behind one API.
//!
//! Every module here is a `mod.rs` that says what the module does and an OS file per family
//! (`unix.rs`, `windows.rs`, and `macos.rs`/`linux.rs` where those two differ). Only the target's
//! files are built, and the OS crates they use (`libc`, `windows`, `interprocess`) are target
//! dependencies, so a build links nothing for another OS. Callers never write `cfg`: what an OS
//! cannot do is an error of kind [`std::io::ErrorKind::Unsupported`], and [`caps`] says up front
//! what works here.
//!
//! The rest of the workspace keeps this rule (`cargo xtask platform`): code that only compiles on
//! one OS lives in a file named for it (`macos.rs`, `linux.rs`, `windows.rs`, `unix.rs`), or here.
pub mod caps;
pub mod daemon;
pub mod ipc;
pub mod paths;
pub mod process;
pub mod pty;
pub mod signal;
#[cfg(windows)]
mod win32;

pub use caps::{Capabilities, caps};

/// An error for something this OS does not do: `what` names it ("job control signals").
pub fn unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, format!("{what} is not supported on {}", std::env::consts::OS))
}

/// The machine's name, short: `devbox`, not `devbox.local`.
pub fn hostname() -> String {
    paths::hostname().split('.').next().filter(|name| !name.is_empty()).unwrap_or("pumi").to_string()
}
