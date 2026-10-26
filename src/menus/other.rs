//! Linux and Windows: no About panel of the system's, so a prompt on the window in front.
use gpui::{App, PromptLevel};

/// The app's name and version, in a prompt on the window in front (nothing without one).
pub fn about(cx: &mut App) {
    let Some(window) = cx.active_window().or_else(|| cx.windows().into_iter().next()) else { return };
    let name = pumi_gpuix::app::app_name();
    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let _ = window.update(cx, |_, window, cx| {
        // Its answer is only "OK": nothing waits for it.
        let _ = window.prompt(PromptLevel::Info, &name, Some(&version), &["OK"], cx);
    });
}
