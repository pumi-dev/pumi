//! Linux and Windows: the system's appearance is GPUI's, which needs the app running.

/// Light, until the app runs and GPUI says.
pub fn dark_before_the_app() -> bool {
    false
}
