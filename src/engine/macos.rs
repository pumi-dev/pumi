//! macOS: the system's appearance before the app runs.
use objc2_foundation::{NSString, NSUserDefaults};

/// Whether `AppleInterfaceStyle` is `Dark`: the default is set only when it is.
pub fn dark_before_the_app() -> bool {
    let style = NSUserDefaults::standardUserDefaults().stringForKey(&NSString::from_str("AppleInterfaceStyle"));
    style.is_some_and(|style| style.to_string() == "Dark")
}
