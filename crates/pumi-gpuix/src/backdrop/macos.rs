//! macOS: the window server's background blur (CoreGraphics' private CGS calls).
pub fn set_blur(window: &gpui::Window, radius: f64) -> Result<(), String> {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        // Private, and stable for years: Apple's Terminal.app uses them.
        fn CGSDefaultConnectionForThread() -> *mut c_void;
        fn CGSSetWindowBackgroundBlurRadius(connection: *mut c_void, window: isize, radius: i32) -> i32;
    }

    // The trait's, not `Window::window_handle` (GPUI's own handle).
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Ok(());
    };
    let view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    let radius = if radius.is_finite() { radius.round().clamp(0., 200.) as i32 } else { 0 };
    // SAFETY: `view` is the window's live content view (the handle borrows the window), and
    // both messages are NSView's and NSWindow's own.
    unsafe {
        let ns_window: *mut AnyObject = msg_send![view, window];
        if ns_window.is_null() {
            return Ok(());
        }
        let number: isize = msg_send![ns_window, windowNumber];
        // Ghostty's call, on the thread AppKit runs on.
        let status = CGSSetWindowBackgroundBlurRadius(CGSDefaultConnectionForThread(), number, radius);
        if status != 0 {
            return Err(format!("the window server would not blur the window ({status})"));
        }
    }
    Ok(())
}

/// Sets the window's opacity on its layer (the content view's, GPUI's `CAMetalLayer`), under the
/// key the renderer reads (`vendor/gpui_apple`'s `GLASS_KEY`): a CALayer keeps a value for any key.
pub fn set_glass(window: &gpui::Window, opacity: f64) -> Result<(), String> {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return Ok(());
    };
    let view = appkit.ns_view.as_ptr().cast::<AnyObject>();
    let opacity = if opacity.is_finite() { opacity.clamp(0., 1.) } else { 1. };
    // SAFETY: `view` is the window's live content view (the handle borrows the window); the
    // number and the key are autoreleased Foundation objects, and `layer` and
    // `setValue:forKey:` are NSView's and CALayer's own.
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        if layer.is_null() {
            return Ok(());
        }
        let value: *mut AnyObject = msg_send![class!(NSNumber), numberWithDouble: opacity];
        let key: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: c"pumiGlass".as_ptr()];
        let _: () = msg_send![layer, setValue: value, forKey: key];
    }
    Ok(())
}
