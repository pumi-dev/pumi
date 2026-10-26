//! Asking a process to end, and ignoring what should not end it.

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// A write to a closed socket is an error, not the end of this process, and losing the
/// terminal it started from (`SIGHUP`) does not end it either. Nothing to do on Windows.
pub fn ignore_hangups() {
    os::ignore_hangups()
}

/// A write to a closed pipe ends this process quietly, as it does other commands (`pumi … |
/// head`): the default `SIGPIPE`, which Rust's runtime ignores. Nothing to do on Windows.
pub fn end_on_closed_pipe() {
    os::end_on_closed_pipe()
}

/// Runs `then` once, on a thread of its own, when this process is asked to end: `SIGTERM` or
/// `SIGINT` on Unix (blocked for every other thread), Ctrl+C, Ctrl+Break, a console closing, a
/// logoff or a shutdown on Windows. Call it before starting other threads.
pub fn on_shutdown(then: impl FnOnce() + Send + 'static) {
    os::on_shutdown(Box::new(then))
}
