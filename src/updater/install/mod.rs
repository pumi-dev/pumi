//! Putting a downloaded release in place.
//!
//! An update is downloaded and unpacked beside the cache ([`stage`]) while the app runs, and
//! swapped in by a small detached script once the app has exited ([`swap`]): a shell script on
//! Unix, PowerShell on Windows. On "Restart to Update" it relaunches, on a plain quit it does
//! not. The swap is two renames — the installed app to `.<name>.old`, the new one into its
//! place — so a failure half way puts the old one back.
//!
//! The old copy stays until the new version has run for a while ([`HEALTHY_AFTER`]) and
//! says so ([`mark_healthy`]). After a relaunch the script waits for that; if it never comes
//! the new version is moved aside and the old one is put back and opened again.
//!
//! On Unix terminals survive all of this: they live in the server (`pumid`), not the app, and
//! the new app hands the running server over to its own binary when that is newer
//! (`runtime::pumid`). Windows cannot replace a running binary or hand a server over
//! (`pumi_sys::caps()`), so its script stops the old server first, and the terminals start
//! again from the saved session.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::fetch::Origin;
use super::manifest::{Asset, sha256_hex};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;
#[cfg(unix)]
#[path = "unix.rs"]
mod unix;

/// How long a new version runs before it counts as working, and its rollback copy goes.
pub const HEALTHY_AFTER: Duration = Duration::from_secs(10);

/// How long the swap script waits for that after a relaunch before rolling back.
const ROLLBACK_AFTER_SECS: u32 = 120;

pub fn cache_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        return crate::home_dir().join("Library/Caches/dev.pumi.app/updates");
    }
    pumi_sys::paths::dir(pumi_sys::paths::Dir::Cache, "pumi").join("updates")
}

/// The installed app this process runs from — `pumi.app` on macOS, the `pumi.app` directory
/// of the Linux tarball or the Windows zip — or why it cannot be updated in place.
pub fn installed_app() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().and_then(|exe| exe.canonicalize()).map_err(|err| err.to_string())?;
    let exe = plain(exe);
    // `pumi.app/Contents/MacOS/pumi` on macOS, `pumi.app/bin/pumi` (`bin\pumi.exe`) elsewhere.
    let up = |path: &Path, n: usize| path.ancestors().nth(n).map(Path::to_path_buf);
    if cfg!(target_os = "macos") {
        let bundle = up(&exe, 3).ok_or("no app bundle")?;
        if bundle.extension().is_none_or(|ext| ext != "app") || !bundle.join("Contents/Info.plist").is_file() {
            return Err("this copy is not an app bundle".into());
        }
        return writable(bundle);
    }
    let root = up(&exe, 2).ok_or("no install directory")?;
    if !root.join("VERSION").is_file() || !root.join("Resources").is_dir() {
        return Err("this copy was not installed from a release".into());
    }
    writable(root)
}

/// `path` without the `\\?\` prefix `canonicalize` gives it on Windows, which PowerShell and
/// the paths shown to people do without. A Unix path never has it.
fn plain(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")) {
        Some(share) if share.starts_with(r"UNC\") => PathBuf::from(format!(r"\\{}", &share[4..])),
        Some(local) => PathBuf::from(local),
        None => path,
    }
}

/// The program inside the installed app `app`.
fn app_exe(app: &Path) -> PathBuf {
    match cfg!(target_os = "macos") {
        true => app.join("Contents/MacOS/pumi"),
        false => app.join("bin").join(pumi_sys::paths::exe("pumi")),
    }
}

/// The app, if its directory can take the swap's renames.
fn writable(app: PathBuf) -> Result<PathBuf, String> {
    let parent = app.parent().ok_or("no parent directory")?;
    let probe = parent.join(format!(".pumi-update-probe-{}", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Ok(app)
        }
        Err(err) => Err(format!("cannot write to {}: {err}", crate::display_path(parent))),
    }
}

/// A release unpacked and checked, ready to be swapped in.
#[derive(Debug, Clone, PartialEq)]
pub struct Staged {
    pub version: String,
    /// Where the app is installed.
    pub app: PathBuf,
    /// The new copy.
    pub new: PathBuf,
}

/// Download (unless an earlier run already did), verify and unpack `asset`.
pub fn stage(version: &str, asset: &Asset, origin: &Origin, app: &Path) -> Result<Staged, String> {
    let dir = cache_dir().join(version);
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let archive = dir.join(&asset.name);
    let matches = |path: &Path| std::fs::read(path).is_ok_and(|bytes| sha256_hex(&bytes) == asset.sha256);
    if !matches(&archive) {
        let part = dir.join(format!("{}.part", asset.name));
        origin.download(&asset.name, &asset.url, &part)?;
        let bytes = std::fs::read(&part).map_err(|err| err.to_string())?;
        if bytes.len() as u64 != asset.size || sha256_hex(&bytes) != asset.sha256 {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{} does not match the signed manifest", asset.name));
        }
        std::fs::rename(&part, &archive).map_err(|err| err.to_string())?;
    }

    let unpacked = dir.join("unpacked");
    let _ = std::fs::remove_dir_all(&unpacked);
    std::fs::create_dir_all(&unpacked).map_err(|err| err.to_string())?;
    run(&mut os::unpack(&archive, &unpacked))?;
    let name = app.file_name().ok_or("no app name")?;
    let new = unpacked.join("pumi.app");
    if !app_exe(&new).is_file() {
        return Err(format!("{} holds no pumi", asset.name));
    }
    os::check(app, &new)?;
    // The new copy takes the installed one's name when it is swapped in.
    let new = match name == "pumi.app" {
        true => new,
        false => {
            let renamed = unpacked.join(name);
            std::fs::rename(&new, &renamed).map_err(|err| err.to_string())?;
            renamed
        }
    };
    Ok(Staged { version: version.to_string(), app: app.to_path_buf(), new })
}

fn run(command: &mut Command) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command.stdin(Stdio::null()).output().map_err(|err| format!("{program}: {err}"))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(format!("{program}: {}", String::from_utf8_lossy(&output.stderr).trim())),
    }
}

/// The marker a version leaves once it has run for [`HEALTHY_AFTER`].
fn marker(version: &str) -> PathBuf {
    cache_dir().join(format!("launched-{version}"))
}

fn old_copy(app: &Path) -> PathBuf {
    let name = app.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    app.with_file_name(format!(".{name}.old"))
}

/// This version works: say so for a waiting swap script, and drop the copy it replaced and
/// any other version's download.
///
/// The first time a version says so after an older one did, that older version is returned: it is
/// what this one was updated from (by the swap, or by hand). `None` every other time.
pub fn mark_healthy() -> Option<semver::Version> {
    let version = env!("CARGO_PKG_VERSION");
    let current = semver::Version::parse(version).ok();
    let dir = cache_dir();
    let _ = std::fs::create_dir_all(&dir);
    let first = !marker(version).exists();
    let _ = std::fs::write(marker(version), b"");
    if let Ok(app) = installed_app() {
        let _ = std::fs::remove_dir_all(old_copy(&app));
    }
    let mut from = None;
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let launched = name.strip_prefix("launched-");
        let stale = match launched {
            Some(launched) => launched != version,
            // A download for a newer version may be waiting for its restart: keep it.
            None => entry.path().is_dir() && !super::manifest::is_newer(&name, version),
        };
        if first
            && let Some(older) = launched.and_then(|v| semver::Version::parse(v).ok())
            && current.as_ref().is_some_and(|current| &older < current)
        {
            from = from.max(Some(older));
        }
        if stale {
            let _ = std::fs::remove_dir_all(entry.path()).or_else(|_| std::fs::remove_file(entry.path()));
        }
    }
    from
}

/// Start the swap script, detached, to run once this process exits. Its inputs are
/// environment variables, so no path is ever quoted into it.
pub fn swap(staged: &Staged, relaunch: bool) -> Result<(), String> {
    let env: [(&str, OsString); 8] = [
        ("PUMI_PID", std::process::id().to_string().into()),
        ("PUMI_APP", staged.app.clone().into()),
        ("PUMI_NEW", staged.new.clone().into()),
        ("PUMI_EXE", app_exe(&staged.app).into()),
        ("PUMI_OLD", old_copy(&staged.app).into()),
        ("PUMI_MARKER", marker(&staged.version).into()),
        ("PUMI_RELAUNCH", if relaunch { "1" } else { "0" }.into()),
        ("PUMI_ROLLBACK_AFTER", ROLLBACK_AFTER_SECS.to_string().into()),
    ];
    os::swap(&env).map_err(|err| format!("could not start the update: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_copy_is_hidden_beside_the_app() {
        assert_eq!(old_copy(Path::new("/Applications/pumi.app")), Path::new("/Applications/.pumi.app.old"));
    }

    #[test]
    fn plain_paths_lose_the_verbatim_prefix() {
        assert_eq!(plain(PathBuf::from(r"\\?\C:\Users\me\pumi.app")), PathBuf::from(r"C:\Users\me\pumi.app"));
        assert_eq!(plain(PathBuf::from(r"\\?\UNC\server\share\pumi.app")), PathBuf::from(r"\\server\share\pumi.app"));
        assert_eq!(plain(PathBuf::from("/Applications/pumi.app")), PathBuf::from("/Applications/pumi.app"));
    }
}
