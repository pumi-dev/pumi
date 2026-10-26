//! What the Windows files share: owned handles, wide strings, Win32 errors as `io::Error`s, the
//! user's SID and the process table.
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{IntoRawHandle, OwnedHandle};
use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, FILETIME, HANDLE, HLOCAL, LocalFree, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_ACCESS_RIGHTS,
};
use windows::core::PWSTR;

/// A handle this process owns, closed when dropped.
pub struct Handle(HANDLE);

// SAFETY: a handle is an index into the process's handle table, good on any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Handle {
    /// Owns `handle`.
    pub fn new(handle: HANDLE) -> Handle {
        Handle(handle)
    }
    pub fn get(&self) -> HANDLE {
        self.0
    }
    /// A second handle to the same object.
    pub fn duplicate(&self) -> io::Result<Handle> {
        let mut copy = HANDLE(std::ptr::null_mut());
        let me = unsafe { GetCurrentProcess() };
        unsafe { DuplicateHandle(me, self.0, me, &mut copy, 0, false, DUPLICATE_SAME_ACCESS) }.io()?;
        Ok(Handle(copy))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

impl From<OwnedHandle> for Handle {
    fn from(handle: OwnedHandle) -> Handle {
        Handle(HANDLE(handle.into_raw_handle()))
    }
}

/// A `windows` result as an `io::Result`, its error as [`error`] makes it.
pub trait ToIo<T> {
    fn io(self) -> io::Result<T>;
}

impl<T> ToIo<T> for windows::core::Result<T> {
    fn io(self) -> io::Result<T> {
        self.map_err(error)
    }
}

/// The `io::Error` of the Win32 code behind `err` (an `HRESULT_FROM_WIN32`, as `windows` reports
/// a failed call), so its kind is the one std gives that code; any other `HRESULT` as it is.
pub fn error(err: windows::core::Error) -> io::Error {
    let code = err.code().0 as u32;
    if code & 0xffff_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((code & 0xffff) as i32)
    } else {
        io::Error::from_raw_os_error(code as i32)
    }
}

/// Whether `err` is the Win32 error `code`.
pub fn is(err: &io::Error, code: WIN32_ERROR) -> bool {
    err.raw_os_error() == Some(code.0 as i32)
}

/// `s` as a NUL-terminated UTF-16 string.
pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain([0]).collect()
}

/// The SID of the user this process runs as, as a string (`S-1-5-21-…`).
pub fn user_sid() -> io::Result<String> {
    let mut token = HANDLE(std::ptr::null_mut());
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.io()?;
    let token = Handle(token);
    let mut len = 0;
    // Asked with no buffer, it fails and says how big one must be.
    let _ = unsafe { GetTokenInformation(token.get(), TokenUser, None, 0, &mut len) };
    let mut buffer = vec![0usize; (len as usize).div_ceil(size_of::<usize>())];
    unsafe { GetTokenInformation(token.get(), TokenUser, Some(buffer.as_mut_ptr().cast()), len, &mut len) }.io()?;
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut string = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut string) }.io()?;
    let sid = unsafe { string.to_string() };
    unsafe { LocalFree(Some(HLOCAL(string.0.cast()))) };
    sid.map_err(io::Error::other)
}

/// The process `pid`, opened for `access`.
pub fn open_process(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> io::Result<Handle> {
    unsafe { OpenProcess(access, false, pid) }.io().map(Handle)
}

/// When `process` started, in 100 ns ticks since 1601: a process's is never before its parent's.
pub fn created(process: HANDLE) -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    Some((created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64)
}

/// A running process, as the process table lists it.
pub struct Process {
    pub pid: u32,
    /// The id its parent had: the parent may have ended since, and the id be another's.
    pub parent: u32,
    /// Its executable's file name: `pwsh.exe`.
    pub exe: String,
}

/// Every running process (a Toolhelp snapshot).
pub fn processes() -> io::Result<Vec<Process>> {
    let snapshot = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.io()?);
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut all = Vec::new();
    let mut next = unsafe { Process32FirstW(snapshot.get(), &mut entry) };
    while next.is_ok() {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        all.push(Process {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            exe: String::from_utf16_lossy(&entry.szExeFile[..len]),
        });
        next = unsafe { Process32NextW(snapshot.get(), &mut entry) };
    }
    Ok(all)
}
