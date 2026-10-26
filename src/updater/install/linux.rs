//! Linux: the tarball unpacked with `tar`. Its archive is trusted by the signed manifest alone.
use std::path::Path;
use std::process::Command;

pub use super::unix::swap;

pub fn unpack(archive: &Path, into: &Path) -> Command {
    let mut tar = Command::new("tar");
    tar.arg("-xzf").arg(archive).arg("-C").arg(into);
    tar
}

pub fn check(_app: &Path, _new: &Path) -> Result<(), String> {
    Ok(())
}
