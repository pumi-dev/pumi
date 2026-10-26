//! Other processes: what they are, waiting for them, ending them.
use crate::pty::Signal;
use std::io;
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// The process's short name (`zsh`, `node`): what `ps -o comm` shows.
pub fn name(pid: u32) -> Option<String> {
    os::name(pid)
}

/// The process's working directory. `None` on Windows, where a terminal's directory is what its
/// shell says (OSC 7).
pub fn cwd(pid: u32) -> Option<String> {
    os::cwd(pid)
}

/// Waits for the child `pid` to end; its exit code (128 + the signal, for one a signal ended).
/// `None` if it is not this process's child or the wait failed.
pub fn wait(pid: u32) -> Option<i32> {
    os::wait(pid)
}

/// Whether the child `pid` has ended, without waiting (and reaps it if so).
pub fn exited(pid: u32) -> bool {
    os::exited(pid)
}

/// Sends `signal` to the process `pid` (or, with `group`, to its process group on Unix).
pub fn signal(pid: u32, signal: Signal, group: bool) -> io::Result<()> {
    os::signal(pid, signal, group)
}

/// Asks `pid` to end (`TERM`), then ends it (`KILL`) if it is still there after `grace`.
pub fn stop(pid: u32, grace: Duration) {
    if exited(pid) {
        return;
    }
    let _ = signal(pid, Signal::Term, false);
    let deadline = std::time::Instant::now() + grace;
    while !exited(pid) {
        if std::time::Instant::now() > deadline {
            let _ = signal(pid, Signal::Kill, false);
            wait(pid);
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Whether the process that started this one has gone (this one was orphaned).
pub fn parent_gone() -> bool {
    os::parent_gone()
}

/// The `cmd` that starts with the signal mask and dispositions reset, whatever this process
/// blocks (the server blocks `TERM` and `INT` for its signal thread). Nothing on Windows.
pub fn reset_signals(cmd: &mut Command) {
    os::reset_signals(cmd)
}

/// Replaces this process with `cmd`, keeping its id and what it lets be inherited. Only returns
/// on failure. Unsupported on Windows (`caps().handoff`).
pub fn exec(cmd: &mut Command) -> io::Error {
    os::exec(cmd)
}

/// The program a terminal runs when it is given none: `$SHELL` (else `/bin/zsh` on macOS,
/// `/bin/sh` elsewhere) on Unix; PowerShell 7 (`pwsh.exe`) if it is on `PATH`, else Windows
/// PowerShell, else `%COMSPEC%`, on Windows.
pub fn default_shell() -> String {
    os::default_shell()
}

#[cfg(test)]
mod tests {
    #[test]
    fn this_process() {
        let pid = std::process::id();
        assert!(super::name(pid).is_some());
        if !cfg!(windows) {
            assert_eq!(super::cwd(pid).map(std::path::PathBuf::from), std::env::current_dir().ok());
        }
        assert!(!super::parent_gone());
    }
}
