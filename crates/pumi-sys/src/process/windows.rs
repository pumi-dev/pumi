//! Windows: process handles, Toolhelp snapshots, TerminateProcess.
use crate::pty::Signal;
use crate::win32::{self, ToIo};
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows::core::PWSTR;

/// The exit code of a process `signal` ended.
const KILLED: u32 = 1;

pub fn name(pid: u32) -> Option<String> {
    let process = win32::open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    // As long as a path can be.
    let mut path = vec![0u16; 32 * 1024];
    let mut len = path.len() as u32;
    unsafe { QueryFullProcessImageNameW(process.get(), PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len) }
        .ok()?;
    let path = PathBuf::from(OsString::from_wide(&path[..len as usize]));
    Some(path.file_stem()?.to_string_lossy().into_owned())
}

pub fn cwd(_pid: u32) -> Option<String> {
    None
}

pub fn wait(pid: u32) -> Option<i32> {
    let process = win32::open_process(pid, PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    if unsafe { WaitForSingleObject(process.get(), INFINITE) } != WAIT_OBJECT_0 {
        return None;
    }
    let mut code = 0;
    unsafe { GetExitCodeProcess(process.get(), &mut code) }.ok()?;
    Some(code as i32)
}

/// A process that cannot be opened is taken to have ended.
pub fn exited(pid: u32) -> bool {
    match win32::open_process(pid, PROCESS_SYNCHRONIZE) {
        Ok(process) => (unsafe { WaitForSingleObject(process.get(), 0) }) == WAIT_OBJECT_0,
        Err(_) => true,
    }
}

/// Every signal that ends a process ends it at once: Windows has no asking first.
pub fn signal(pid: u32, signal: Signal, _group: bool) -> io::Result<()> {
    match signal {
        Signal::Term | Signal::Kill | Signal::Hup | Signal::Quit => {
            let process = win32::open_process(pid, PROCESS_TERMINATE)?;
            unsafe { TerminateProcess(process.get(), KILLED) }.io()
        }
        // GenerateConsoleCtrlEvent reaches only processes on this process's own console.
        Signal::Int => Err(crate::unsupported("interrupting a process on another console")),
        Signal::Stop | Signal::Cont | Signal::Winch => Err(crate::unsupported("job control and window signals")),
    }
}

/// The parent is looked up once, at the first call, and kept open: its handle is signalled when
/// it ends, whatever process takes its id next. A parent that was gone (or could not be opened)
/// at the first call is never reported, as Unix takes the first call's parent as the baseline.
pub fn parent_gone() -> bool {
    static PARENT: OnceLock<Option<win32::Handle>> = OnceLock::new();
    let parent = PARENT.get_or_init(|| {
        let me = std::process::id();
        let id = win32::processes().ok()?.into_iter().find(|p| p.pid == me)?.parent;
        let parent = win32::open_process(id, PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
        // An id outlives its process, and may name a newer one: never this one's parent.
        let older = win32::created(parent.get())? <= win32::created(unsafe { GetCurrentProcess() })?;
        older.then_some(parent)
    });
    parent.as_ref().is_some_and(|parent| unsafe { WaitForSingleObject(parent.get(), 0) } == WAIT_OBJECT_0)
}

pub fn reset_signals(_cmd: &mut Command) {}

pub fn exec(_cmd: &mut Command) -> io::Error {
    crate::unsupported("replacing a running process")
}

pub fn default_shell() -> String {
    let on_path = |name: &str| {
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
    };
    if let Some(pwsh) = on_path("pwsh.exe") {
        return pwsh.to_string_lossy().into_owned();
    }
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let powershell = std::path::Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if powershell.is_file() {
        return powershell.to_string_lossy().into_owned();
    }
    std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
}
