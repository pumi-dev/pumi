//! A plugin's npm dependencies: `bun install` in its directory, run again only when what it asks
//! for changes. What an install was for is kept in `node_modules/.pumi-install`, so a plugin
//! installed before pumi ran it (or whose install failed) is installed on the next start.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where the stamp of the last install is, in the plugin's own `node_modules`.
const STAMP: &str = ".pumi-install";
/// The lockfiles an install follows; a change to one installs again.
const LOCKFILES: &[&str] = &["bun.lock", "bun.lockb"];

/// What an install of `dir` is for: a hash of its `dependencies` and `optionalDependencies` and
/// its lockfile. None when it asks for nothing (`devDependencies` are the author's, not needed to
/// run it).
pub fn wanted(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let pkg: Value = serde_json::from_str(&text).ok()?;
    let deps: Vec<std::collections::BTreeMap<&String, &Value>> = ["dependencies", "optionalDependencies"]
        .iter()
        .filter_map(|key| pkg.get(key)?.as_object())
        .filter(|o| !o.is_empty())
        .map(|o| o.iter().collect())
        .collect();
    if deps.is_empty() {
        return None;
    }
    // Sorted by name (serde_json may keep a file's order), so the same dependencies hash the same.
    let mut hash = fnv1a(FNV_OFFSET, serde_json::to_string(&deps).expect("plain data").as_bytes());
    for lock in LOCKFILES {
        if let Ok(bytes) = std::fs::read(dir.join(lock)) {
            hash = fnv1a(hash, lock.as_bytes());
            hash = fnv1a(hash, &bytes);
        }
    }
    Some(format!("{hash:016x}"))
}

fn stamp(dir: &Path) -> PathBuf {
    dir.join("node_modules").join(STAMP)
}

/// Whether `dir` asks for dependencies its last install was not for.
pub fn needed(dir: &Path) -> bool {
    wanted(dir).is_some_and(|want| std::fs::read_to_string(stamp(dir)).ok().as_deref().map(str::trim) != Some(&want))
}

/// Installs `dir`'s dependencies when they changed since the last install: `bun install
/// --production --no-save --ignore-scripts`, so the checkout stays as its commit has it and no
/// install script runs. Answers whether it ran.
pub fn ensure(dir: &Path) -> Result<bool, String> {
    // One install at a time: the startup's catch-up and a `plugin.update` may want the same one.
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(want) = wanted(dir) else { return Ok(false) };
    if std::fs::read_to_string(stamp(dir)).ok().as_deref().map(str::trim) == Some(want.as_str()) {
        return Ok(false);
    }
    let (program, be_bun) =
        bun().ok_or("its dependencies need Bun to install: `bun` on the PATH, or $PUMI_BUN".to_string())?;
    let mut command = Command::new(&program);
    if be_bun {
        command.env("BUN_BE_BUN", "1");
    }
    let out = command
        .args(["install", "--production", "--no-save", "--ignore-scripts", "--no-progress"])
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let why = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
        return Err(format!("bun install in {}: {why}", dir.display()));
    }
    let stamp = stamp(dir);
    if let Some(parent) = stamp.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&stamp, format!("{want}\n")).map_err(|e| format!("{}: {e}", stamp.display()))?;
    Ok(true)
}

/// Bun: `$PUMI_BUN`, `bun` on the PATH, `~/.bun/bin/bun`, or the app beside this host (a Bun
/// executable, which runs as Bun with `BUN_BE_BUN=1`). The flag says it is the app.
fn bun() -> Option<(PathBuf, bool)> {
    if let Some(path) = std::env::var_os("PUMI_BUN").filter(|p| !p.is_empty()) {
        return Some((PathBuf::from(path), false));
    }
    let name = pumi_sys::paths::exe("bun");
    let on_path = std::env::var_os("PATH")
        .and_then(|path| std::env::split_paths(&path).map(|d| d.join(&name)).find(|p| p.is_file()));
    if let Some(found) = on_path {
        return Some((found, false));
    }
    let home = pumi_proto::paths::home_dir().join(".bun").join("bin").join(&name);
    if home.is_file() {
        return Some((home, false));
    }
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let app = exe.parent()?.join(pumi_sys::paths::exe("pumi"));
    app.is_file().then_some((app, true))
}

/// The enabled plugins installed from git whose dependencies their last install was not for.
pub fn missing() -> Vec<(String, PathBuf)> {
    crate::plugins::lock_read()
        .plugins
        .into_iter()
        .filter(|(_, e)| !e.disabled && e.kind.as_deref() == Some("git"))
        .map(|(name, _)| {
            let dir = crate::plugins::dir_of(&name);
            (name, dir)
        })
        .filter(|(_, dir)| needed(dir))
        .collect()
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a: stable across builds, unlike `std`'s hasher, so a stamp outlives an update of pumi.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plugin(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), manifest).unwrap();
        dir
    }
    #[test]
    fn only_runtime_dependencies_need_an_install() {
        assert_eq!(wanted(plugin(r#"{ "name": "a" }"#).path()), None);
        assert_eq!(wanted(plugin(r#"{ "dependencies": {} }"#).path()), None);
        assert_eq!(wanted(plugin(r#"{ "devDependencies": { "typescript": "5.9.3" } }"#).path()), None);
        assert!(wanted(plugin(r#"{ "dependencies": { "qrcode": "^1.5.4" } }"#).path()).is_some());
        assert!(wanted(plugin(r#"{ "optionalDependencies": { "qrcode": "^1.5.4" } }"#).path()).is_some());
    }
    #[test]
    fn a_stamp_for_what_it_asks_now_means_no_install() {
        let dir = plugin(r#"{ "dependencies": { "qrcode": "^1.5.4" } }"#);
        assert!(needed(dir.path()), "never installed");
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(stamp(dir.path()), wanted(dir.path()).unwrap() + "\n").unwrap();
        assert!(!needed(dir.path()), "installed for these");
        std::fs::write(dir.path().join("bun.lock"), "{}").unwrap();
        assert!(needed(dir.path()), "its lockfile changed");
        std::fs::write(stamp(dir.path()), wanted(dir.path()).unwrap()).unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{ "dependencies": { "qrcode": "^1.6.0" } }"#).unwrap();
        assert!(needed(dir.path()), "its dependencies changed");
    }
    #[test]
    fn the_same_dependencies_in_another_order_are_the_same_install() {
        let a = plugin(r#"{ "dependencies": { "a": "1", "b": "2" } }"#);
        let b = plugin(r#"{ "dependencies": { "b": "2", "a": "1" } }"#);
        assert_eq!(wanted(a.path()), wanted(b.path()));
    }
}
