//! macOS: AppKit's About panel.
use gpui::App;

/// AppKit's About panel: the app icon, name, version and copyright, from the bundle's Info.plist.
pub fn about(_cx: &mut App) {
    let Some(mtm) = objc2_foundation::MainThreadMarker::new() else { return };
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    app.orderFrontStandardAboutPanel(None);
    app.activate();
}
