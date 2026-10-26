//! What the app draws natively: the elements React places (`<terminal>`, `<preview>`), and the
//! Diagnostics window (Settings is React: runtime/app/app/settings).
pub use pumi_ui as kit;

pub mod diagnostics;
pub mod elements;
pub mod ime;
pub mod preview;
pub mod terminal;
pub mod webview;
pub mod widgets;
