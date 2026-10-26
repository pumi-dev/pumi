//! Detaching: the server keeps running after whatever started it (the app, a shell) is gone.
use std::io;
use std::path::Path;

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// A log at `log` past this many bytes is moved aside (`<log>.1`) when a process detaches to it.
const LOG_LIMIT: u64 = 8 << 20;

/// Makes this process a daemon writing to `log`: it leaves its parent (who returns at once),
/// its session or console, and its terminal; stdin is empty and stdout and stderr go to `log`.
///
/// Unix forks and the parent exits. Windows starts this binary again, detached and with the
/// same arguments, and exits; the new process sees it is the detached one and returns.
pub fn detach(log: &Path) -> io::Result<()> {
    if std::fs::metadata(log).is_ok_and(|m| m.len() > LOG_LIMIT) {
        let _ = std::fs::rename(log, log.with_extension("log.1"));
    }
    os::detach(log)
}
