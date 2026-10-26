//! Where things are: the home directory, pumi's directories, executables, links.
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

pub(crate) use os::hostname;

/// The user's home directory: `$HOME`, `%USERPROFILE%` on Windows.
pub fn home() -> PathBuf {
    os::home()
}

/// One of an app's directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Settings the user writes: `~/.config/<app>`, `%APPDATA%\<app>`.
    Config,
    /// Data the app keeps: `~/.local/share/<app>`, `%LOCALAPPDATA%\<app>\data`.
    Data,
    /// State worth keeping across runs, not worth backing up: `~/.local/state/<app>`,
    /// `%LOCALAPPDATA%\<app>\state`.
    State,
    /// What can be thrown away: `~/.cache/<app>`, `%LOCALAPPDATA%\<app>\cache`.
    Cache,
    /// The server's own (its sockets' directory, the session, logs): `~/Library/Application
    /// Support/<app>` on macOS, `~/.local/share/<app>` on Linux, `%LOCALAPPDATA%\<app>` on Windows.
    Server,
}

impl Dir {
    /// The XDG variable that moves it, on every OS.
    fn xdg(self) -> Option<&'static str> {
        match self {
            Dir::Config => Some("XDG_CONFIG_HOME"),
            Dir::Data => Some("XDG_DATA_HOME"),
            Dir::State => Some("XDG_STATE_HOME"),
            Dir::Cache => Some("XDG_CACHE_HOME"),
            Dir::Server => None,
        }
    }
}

/// `app`'s directory of `kind`. An XDG variable that is set (`XDG_CONFIG_HOME`, …) wins on
/// every OS.
pub fn dir(kind: Dir, app: &str) -> PathBuf {
    if let Some(base) = kind.xdg().and_then(std::env::var_os).filter(|v| !v.is_empty()) {
        return PathBuf::from(base).join(app);
    }
    os::dir(kind, app)
}

/// Lets only this user in: `mode` (`0o700`, `0o600`) on Unix. On Windows a user's profile is
/// theirs already, and nothing is done.
pub fn restrict(path: &Path, mode: u32) -> io::Result<()> {
    os::restrict(path, mode)
}

/// Makes `link` name `target`: a symbolic link on Unix; on Windows a junction for a directory
/// and a hard link for a file (neither needs the rights a symbolic link does).
pub fn link(target: &Path, link: &Path) -> io::Result<()> {
    os::link(target, link)
}

/// `name` as this OS names a program: `pumid`, `pumid.exe`.
pub fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Whether `path` is a file this user can run: executable bits on Unix, an `.exe` on Windows.
pub fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && os::is_executable(path, &meta))
}
