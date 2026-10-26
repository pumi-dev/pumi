//! Unix: XDG directories (and macOS's Application Support for the server), modes, symlinks.
use super::Dir;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn dir(kind: Dir, app: &str) -> PathBuf {
    let home = home();
    match kind {
        Dir::Config => home.join(".config").join(app),
        Dir::Data => home.join(".local/share").join(app),
        Dir::State => home.join(".local/state").join(app),
        Dir::Cache => home.join(".cache").join(app),
        Dir::Server if cfg!(target_os = "macos") => home.join("Library/Application Support").join(app),
        Dir::Server => super::dir(Dir::Data, app),
    }
}

pub fn restrict(path: &Path, mode: u32) -> io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

pub fn link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

pub fn is_executable(_path: &Path, meta: &std::fs::Metadata) -> bool {
    meta.permissions().mode() & 0o111 != 0
}

pub fn hostname() -> String {
    let mut buf = [0u8; 256];
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return String::new();
    }
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}
