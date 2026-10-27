use crate::dirs::{config, stdpath, write_atomic};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The git ref it follows (a tag, a branch or a commit); none, the default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Its directory in the repository, for one of several in a repository (`owner/repo/dir`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub plugins: BTreeMap<String, Entry>,
}
/// Where installed plugins are: `stdpath('data')/installed/<name>`, each a directory or a link
/// (`stdpath('data')/plugins` holds their logs).
pub fn root() -> PathBuf {
    stdpath("data").expect("data is a stdpath").join("installed")
}
/// The checkout a plugin from a repository's folder lives in; its directory links into it.
pub fn checkout_of(name: &str) -> PathBuf {
    root().join(".src").join(name)
}
pub fn lockfile() -> PathBuf {
    config().join("plugin-lock.json")
}
/// The plugins that ship with pumi, by name: `<runtime>/bundled/<name>/`, whose owner is `pumi`.
/// They are listed, turned off and on like any other, and never installed, updated or removed.
pub fn bundled() -> BTreeMap<String, PathBuf> {
    let Some(runtime) = crate::dirs::runtime_dir() else { return BTreeMap::new() };
    std::fs::read_dir(runtime.join("bundled"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("package.json").is_file())
        .filter_map(|p| Some((p.file_name()?.to_str()?.to_string(), p)))
        .collect()
}
pub fn is_bundled(name: &str) -> bool {
    bundled().contains_key(name)
}
/// The owner only bundled plugins have.
pub const BUNDLED_OWNER: &str = "pumi";
/// Where a plugin's code is: a bundled one's directory, a linked one's where it was linked from, and
/// otherwise where it is installed (`installed_at`).
pub fn dir_of(name: &str) -> PathBuf {
    if let Some(dir) = bundled().remove(name) {
        return dir;
    }
    // A link lives where it was linked from (`plugin add ./dir`), until it is installed as a copy.
    let lock = lock_read();
    if let Some(entry) = lock.plugins.get(name)
        && entry.kind.as_deref() == Some("link")
        && let Some(src) = entry.src.as_deref()
    {
        let path = PathBuf::from(src);
        if path.join("package.json").is_file() {
            return path;
        }
    }
    installed_at(name)
}
/// Where a plugin is installed: `root()/<name>`, the directory it was cloned to or a link. For a
/// linked plugin that is the link, not its directory (`dir_of`), which is its author's.
pub fn installed_at(name: &str) -> PathBuf {
    root().join(name)
}
/// The lockfile's earlier name, `pack-lock.json`: read when the current one is missing.
fn legacy_lockfile() -> PathBuf {
    config().join("pack-lock.json")
}
pub fn lock_read() -> Lock {
    // A lockfile under the earlier name: read it, and move it over on the first write.
    if !lockfile().exists()
        && let Ok(bytes) = std::fs::read(legacy_lockfile())
    {
        let _ = write_atomic(&lockfile(), &String::from_utf8_lossy(&bytes));
    }
    let mut lock: Lock =
        std::fs::read(lockfile()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    for name in bundled().into_keys() {
        let entry = lock.plugins.entry(name).or_default();
        *entry = Entry { kind: Some("bundled".into()), disabled: entry.disabled, ..Entry::default() };
    }
    lock
}
/// Writes the lockfile. A bundled plugin is kept in it only while it is turned off.
pub fn lock_write(lock: &Lock) -> std::io::Result<()> {
    let mut lock = lock.clone();
    lock.plugins.retain(|_, e| e.kind.as_deref() != Some("bundled") || e.disabled);
    let text = serde_json::to_string_pretty(&lock).expect("plain data") + "\n";
    crate::dirs::write_atomic(&lockfile(), &text)
}
pub fn enabled() -> Vec<(String, PathBuf)> {
    lock_read().plugins.into_iter().filter(|(_, e)| !e.disabled).map(|(n, _)| (n.clone(), dir_of(&n))).collect()
}
fn start_plugins(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir.join("plugins"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|p| std::fs::read_dir(p.path().join("start")).ok())
        .flat_map(|r| r.flatten().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    found
}
pub fn runtimepath(clean: bool) -> String {
    let mut bases = Vec::new();
    if !clean {
        bases.push(config());
        bases.extend(stdpath("data").map(|d| d.join("site")));
    }
    bases.extend(crate::dirs::runtime_dir());
    let mut dirs = Vec::new();
    for base in bases {
        dirs.push(base.clone());
        dirs.extend(start_plugins(&base));
    }
    if !clean {
        dirs.extend(enabled().into_iter().map(|(_, d)| d).filter(|d| d.exists()));
    }
    dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(",")
}
