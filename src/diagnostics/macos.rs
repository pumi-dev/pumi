//! macOS: when the process started (libproc), and the display's refresh rate (AppKit).
use std::time::Duration;

/// When this process started, since the Unix epoch.
pub fn process_started() -> Option<Duration> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let pid = std::process::id() as libc::c_int;
    let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    (read == size).then(|| Duration::new(info.pbi_start_tvsec, (info.pbi_start_tvusec * 1000) as u32))
}

/// `NSScreen.mainScreen.maximumFramesPerSecond`; 0 without a screen.
pub fn refresh_hz() -> f32 {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    // SAFETY: class methods and a property of NSScreen, read on the main thread.
    unsafe {
        let screen: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
        if screen.is_null() {
            return 0.0;
        }
        let fps: isize = msg_send![screen, maximumFramesPerSecond];
        fps.max(0) as f32
    }
}
