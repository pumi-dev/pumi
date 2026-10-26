//! Windows: Known Folders through their variables (`%APPDATA%`, `%LOCALAPPDATA%`), junctions.
use super::Dir;
use crate::win32::{self, ToIo};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_NONE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
use windows::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
use windows::core::PCWSTR;

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home() -> PathBuf {
    var("USERPROFILE").or_else(|| var("HOME")).unwrap_or_else(|| PathBuf::from(r"C:\"))
}

pub fn dir(kind: Dir, app: &str) -> PathBuf {
    let roaming = var("APPDATA").unwrap_or_else(|| home().join(r"AppData\Roaming"));
    let local = var("LOCALAPPDATA").unwrap_or_else(|| home().join(r"AppData\Local")).join(app);
    match kind {
        Dir::Config => roaming.join(app),
        Dir::Data => local.join("data"),
        Dir::State => local.join("state"),
        Dir::Cache => local.join("cache"),
        Dir::Server => local,
    }
}

pub fn restrict(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

pub fn link(target: &Path, link: &Path) -> io::Result<()> {
    if target.is_dir() { junction(target, link) } else { std::fs::hard_link(target, link) }
}

/// A directory junction, as `mklink /J` makes one: an empty directory with a mount point
/// reparse point naming `target`'s absolute path.
fn junction(target: &Path, link: &Path) -> io::Result<()> {
    let buffer = mount_point(&std::path::absolute(target)?)?;
    std::fs::create_dir(link)?;
    let set = set_reparse_point(link, &buffer);
    if set.is_err() {
        let _ = std::fs::remove_dir(link);
    }
    set
}

/// A mount point's REPARSE_DATA_BUFFER: its tag, the length of what follows and a reserved
/// `u16`; then the substitute name's offset and length and the print name's, in bytes, and the
/// two names, each ended by a NUL the lengths leave out. The substitute name is the NT path
/// `\??\C:\…`, the print name what a listing shows, `C:\…`.
fn mount_point(target: &Path) -> io::Result<Vec<u8>> {
    let mut print: Vec<u16> = target.as_os_str().encode_wide().collect();
    let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
    if print.starts_with(&verbatim) {
        print.drain(..verbatim.len());
    }
    let substitute: Vec<u16> = r"\??\".encode_utf16().chain(print.iter().copied()).collect();
    let names = 2 * (substitute.len() + 1 + print.len() + 1);
    let too_long = || io::Error::new(io::ErrorKind::InvalidInput, "a junction's target path is too long");
    let data_len = u16::try_from(8 + names).map_err(|_| too_long())?;
    let mut buffer = Vec::with_capacity(8 + data_len as usize);
    buffer.extend(IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    buffer.extend(data_len.to_le_bytes());
    buffer.extend(0u16.to_le_bytes());
    let (substitute_len, print_len) = (2 * substitute.len() as u16, 2 * print.len() as u16);
    for field in [0, substitute_len, substitute_len + 2, print_len] {
        buffer.extend(field.to_le_bytes());
    }
    for unit in substitute.iter().chain(&[0]).chain(&print).chain(&[0]) {
        buffer.extend(unit.to_le_bytes());
    }
    Ok(buffer)
}

/// Gives the empty directory `dir` the reparse point `buffer`.
fn set_reparse_point(dir: &Path, buffer: &[u8]) -> io::Result<()> {
    let path = win32::wide(dir);
    let dir = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };
    let dir = win32::Handle::new(dir.io()?);
    let mut returned = 0;
    unsafe {
        DeviceIoControl(
            dir.get(),
            FSCTL_SET_REPARSE_POINT,
            Some(buffer.as_ptr().cast()),
            buffer.len() as u32,
            None,
            0,
            Some(&raw mut returned),
            None,
        )
    }
    .io()
}

pub fn is_executable(path: &Path, _meta: &std::fs::Metadata) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}
