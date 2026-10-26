//! Unix: `waitpid`, `kill`, `exec`. Naming a process differs between macOS and Linux.
use crate::pty::Signal;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod names;
#[cfg(not(target_os = "macos"))]
#[path = "linux.rs"]
mod names;

pub use names::{cwd, name};

fn status(status: libc::c_int) -> Option<i32> {
    if libc::WIFEXITED(status) {
        Some(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        Some(128 + libc::WTERMSIG(status))
    } else {
        None
    }
}

pub fn wait(pid: u32) -> Option<i32> {
    let mut code = 0;
    loop {
        let got = unsafe { libc::waitpid(pid as libc::pid_t, &mut code, 0) };
        if got == pid as libc::pid_t {
            return status(code);
        }
        if got == -1 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return None;
    }
}

pub fn exited(pid: u32) -> bool {
    let mut code = 0;
    unsafe { libc::waitpid(pid as libc::pid_t, &mut code, libc::WNOHANG) != 0 }
}

pub fn signal(pid: u32, signal: Signal, group: bool) -> io::Result<()> {
    let target = if group { -(pid as libc::pid_t) } else { pid as libc::pid_t };
    if unsafe { libc::kill(target, crate::pty::os::number(signal)) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn parent_gone() -> bool {
    static PARENT: std::sync::OnceLock<libc::pid_t> = std::sync::OnceLock::new();
    let now = unsafe { libc::getppid() };
    *PARENT.get_or_init(|| now) != now
}

pub fn reset_signals(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            let mut empty: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut empty);
            libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
            Ok(())
        });
    }
}

pub fn exec(cmd: &mut Command) -> io::Error {
    cmd.exec()
}

pub fn default_shell() -> String {
    let fallback = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" };
    std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| fallback.into())
}
