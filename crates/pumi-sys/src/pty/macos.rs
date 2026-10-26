//! macOS: a terminal's program spawned as its own responsible process (`Spawn::disclaim`).
//!
//! posix_spawn, the only way to set the responsibility attribute: the session, the controlling
//! terminal (opened by the new session leader), the signals and the directory are spawn
//! attributes and file actions instead of `pre_exec`.
use super::RESET_SIGNALS;
use crate::pty::Spawn;
use std::ffi::{CString, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
unsafe extern "C" {
    // libSystem, macOS 10.14+ (what Chromium and LLDB use for the same thing).
    fn responsibility_spawnattrs_setdisclaim(attr: *mut libc::posix_spawnattr_t, disclaim: libc::c_int) -> libc::c_int;
    fn posix_spawn_file_actions_addchdir_np(
        actions: *mut libc::posix_spawn_file_actions_t,
        path: *const libc::c_char,
    ) -> libc::c_int;
}
const POSIX_SPAWN_SETSID: libc::c_int = 0x0400;
fn check(code: libc::c_int) -> io::Result<()> {
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
}
fn cstr(s: impl AsRef<OsStr>) -> io::Result<CString> {
    CString::new(s.as_ref().as_bytes()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL byte"))
}
/// What `Command` does: a bare name is looked up on the child's PATH.
fn resolve(spec: &Spawn) -> io::Result<PathBuf> {
    if spec.program.contains('/') {
        return Ok(spec.cwd.join(&spec.program));
    }
    let path = spec
        .env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(&spec.program))
        .find(|candidate| {
            cstr(candidate).is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0) && candidate.is_file()
        })
        .ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))
}
struct Attr(libc::posix_spawnattr_t);
impl Drop for Attr {
    fn drop(&mut self) {
        unsafe { libc::posix_spawnattr_destroy(&mut self.0) };
    }
}
struct Actions(libc::posix_spawn_file_actions_t);
impl Drop for Actions {
    fn drop(&mut self) {
        unsafe { libc::posix_spawn_file_actions_destroy(&mut self.0) };
    }
}
pub fn spawn(spec: &Spawn, tty: &str) -> io::Result<u32> {
    let resolved = resolve(spec)?;
    let argv0 = spec.argv0.as_deref().unwrap_or(&spec.program);
    let rest = spec.args.iter().map(OsStr::new);
    // Through the trampoline, which takes the tty before it becomes the program.
    let (program, argv): (_, Vec<&OsStr>) = match super::TRAMPOLINE.get() {
        Some(trampoline) => (
            trampoline.as_os_str(),
            [OsStr::new("pumid"), OsStr::new(crate::pty::TTY_EXEC), resolved.as_os_str(), OsStr::new(argv0)]
                .into_iter()
                .chain(rest)
                .collect(),
        ),
        None => (resolved.as_os_str(), std::iter::once(OsStr::new(argv0)).chain(rest).collect()),
    };
    let program = cstr(program)?;
    let argv: Vec<CString> = argv.into_iter().map(cstr).collect::<io::Result<_>>()?;
    let envp: Vec<CString> = spec.env.iter().map(|(k, v)| cstr(format!("{k}={v}"))).collect::<io::Result<_>>()?;
    let mut argv_ptrs: Vec<*mut libc::c_char> = argv.iter().map(|a| a.as_ptr().cast_mut()).collect();
    argv_ptrs.push(ptr::null_mut());
    let mut envp_ptrs: Vec<*mut libc::c_char> = envp.iter().map(|e| e.as_ptr().cast_mut()).collect();
    envp_ptrs.push(ptr::null_mut());
    let cwd = cstr(&spec.cwd)?;
    let tty = cstr(tty)?;
    unsafe {
        let mut attr = Attr(ptr::null_mut());
        check(libc::posix_spawnattr_init(&mut attr.0))?;
        let flags = POSIX_SPAWN_SETSID
            | libc::POSIX_SPAWN_SETSIGDEF
            | libc::POSIX_SPAWN_SETSIGMASK
            | libc::POSIX_SPAWN_CLOEXEC_DEFAULT;
        check(libc::posix_spawnattr_setflags(&mut attr.0, flags as libc::c_short))?;
        let mut defaults: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut defaults);
        for signal in RESET_SIGNALS {
            libc::sigaddset(&mut defaults, *signal);
        }
        check(libc::posix_spawnattr_setsigdefault(&mut attr.0, &defaults))?;
        let mut empty: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut empty);
        check(libc::posix_spawnattr_setsigmask(&mut attr.0, &empty))?;
        // Best effort: without it the program is pumi's, as with `Command`.
        let _ = responsibility_spawnattrs_setdisclaim(&mut attr.0, 1);
        let mut actions = Actions(ptr::null_mut());
        check(libc::posix_spawn_file_actions_init(&mut actions.0))?;
        check(posix_spawn_file_actions_addchdir_np(&mut actions.0, cwd.as_ptr()))?;
        // Opened by the new session's leader, the tty becomes its controlling terminal.
        check(libc::posix_spawn_file_actions_addopen(&mut actions.0, 0, tty.as_ptr(), libc::O_RDWR, 0))?;
        check(libc::posix_spawn_file_actions_adddup2(&mut actions.0, 0, 1))?;
        check(libc::posix_spawn_file_actions_adddup2(&mut actions.0, 0, 2))?;
        let mut pid: libc::pid_t = 0;
        check(libc::posix_spawn(
            &mut pid,
            program.as_ptr(),
            &actions.0,
            &attr.0,
            argv_ptrs.as_ptr(),
            envp_ptrs.as_ptr(),
        ))?;
        Ok(pid as u32)
    }
}
