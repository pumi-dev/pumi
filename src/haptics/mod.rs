//! Haptic feedback: a Force Touch trackpad's tap, which the kit's controls play as a switch
//! turns and a slider steps (Apple's HIG: alignment and level changes, never plain clicks).
//! Nothing happens on a mouse, off the main thread, or off macOS.

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

/// Plays `pattern` (`generic`, `alignment`, `levelChange`) now.
pub fn perform(pattern: &str) {
    os::perform(pattern)
}
