//! Windows: a console control handler.
//!
//! A process with a console hears Ctrl+C, Ctrl+Break and its console closing. One with none (a
//! detached server) hears only logoff and shutdown, and those only while it loads no user32 or
//! gdi32 (as a service would); otherwise the session's end ends it unannounced.
use std::sync::{Mutex, PoisonError};
use windows::Win32::Foundation::{FALSE, TRUE};
use windows::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT, SetConsoleCtrlHandler,
};
use windows::core::BOOL;

/// What `on_shutdown` was given, until the first event takes it.
static THEN: Mutex<Option<Box<dyn FnOnce() + Send>>> = Mutex::new(None);

pub fn ignore_hangups() {}

pub fn end_on_closed_pipe() {}

pub fn on_shutdown(then: Box<dyn FnOnce() + Send>) {
    *THEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(then);
    let _ = unsafe { SetConsoleCtrlHandler(Some(handler), true) };
}

/// Runs on a thread the system starts for the event.
unsafe extern "system" fn handler(event: u32) -> BOOL {
    if !matches!(event, CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
        return FALSE;
    }
    if let Some(then) = THEN.lock().unwrap_or_else(PoisonError::into_inner).take() {
        let _ = std::thread::Builder::new().name("pumi-signals".into()).spawn(then);
    }
    // After these the system ends the process as soon as this returns (or its few seconds are
    // up): never returning gives the shutdown that time to finish and exit by itself.
    if matches!(event, CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT) {
        loop {
            std::thread::park();
        }
    }
    TRUE
}
