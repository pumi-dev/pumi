//! macOS: the zip unpacked with `ditto`, its signature checked against this copy's.
use std::path::Path;
use std::process::{Command, Stdio};

pub use super::unix::swap;

/// ditto, as the zip was made: it restores the signature and symlinks exactly.
pub fn unpack(archive: &Path, into: &Path) -> Command {
    let mut ditto = Command::new("ditto");
    ditto.args(["-x", "-k"]).arg(archive).arg(into);
    ditto
}

/// The new bundle's seal must be whole, and when this copy is signed with a certificate the
/// new one must satisfy this one's designated requirement: the same certificate, so file
/// and keychain permissions carry over (macOS keys them on that requirement).
pub fn check(app: &Path, new: &Path) -> Result<(), String> {
    let ours = Command::new("codesign").args(["-d", "-r-"]).arg(app).output().map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&ours.stdout).into_owned() + &String::from_utf8_lossy(&ours.stderr);
    let requirement = text.lines().find_map(|line| line.strip_prefix("designated => ")).map(str::to_string);
    let mut verify = Command::new("codesign");
    verify.args(["--verify", "--deep", "--strict"]);
    if let Some(requirement) = requirement.filter(|r| r.contains("certificate")) {
        verify.arg(format!("-R={requirement}"));
    }
    let status = verify.arg(new).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|err| err.to_string())?;
    status.success().then_some(()).ok_or_else(|| "the update is not signed like this copy of pumi".into())
}
