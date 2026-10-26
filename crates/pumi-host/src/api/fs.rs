use pumi_api::{Cx, Error, Optional, Result, api};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
/// Past this many, a folder's entries are cut off (a picker has no use for more).
const MOST: usize = 5000;
/// What a path in a listing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}
/// One entry of a listing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirEntry {
    /// Its name in its folder.
    pub name: String,
    /// Its full path.
    pub path: String,
    pub kind: EntryKind,
}
/// What `fs.list` takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListOpts {
    /// Hidden entries too (`.` files).
    #[serde(default)]
    pub hidden: Optional<bool>,
    /// Every folder under it, not only its own entries.
    #[serde(default)]
    pub recursive: Optional<bool>,
}
/// The entries of a folder on the server's machine: `~` and `~/…` are its home, a relative path
/// is in it, and an empty path is the home itself. Hidden entries are left out unless `hidden`,
/// and `recursive` lists every folder under it too; both sorted by name, case-insensitively.
#[api(
    name = "fs.list",
    effect = "read",
    example(args = r#"["~/src"]"#),
    example(args = r#"["/tmp", {"hidden": true}]"#, desc = "Hidden entries too.")
)]
pub fn pumi_list(cx: &mut Cx, path: String, opts: Optional<ListOpts>) -> Result<Vec<DirEntry>> {
    let _ = cx;
    let opts = opts.into_option().unwrap_or_default();
    let home = pumi_proto::paths::home_dir();
    let path = expand(if path.is_empty() { "~" } else { &path }, &home);
    let path = path.canonicalize().map_err(|err| Error::not_found(format!("{}: {err}", path.display())))?;
    let recursive = opts.recursive.0.unwrap_or(false);
    let hidden = opts.hidden.0.unwrap_or(false);
    let mut entries = Vec::new();
    walk(&path, hidden, recursive, &mut entries)?;
    entries.sort_by(|a, b| {
        let by_name = a.path.to_lowercase().cmp(&b.path.to_lowercase());
        by_name.then_with(|| a.path.cmp(&b.path))
    });
    entries.truncate(MOST);
    Ok(entries)
}
fn expand(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ if Path::new(path).is_absolute() => PathBuf::from(path),
        _ => home.join(path),
    }
}
fn walk(dir: &Path, hidden: bool, recursive: bool, out: &mut Vec<DirEntry>) -> Result<()> {
    let read = std::fs::read_dir(dir).map_err(|err| Error::not_found(format!("{}: {err}", dir.display())))?;
    for entry in read.flatten() {
        if out.len() >= MOST {
            return Ok(());
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !hidden && name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let kind = if meta.file_type().is_symlink() {
            EntryKind::Symlink
        } else if meta.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        out.push(DirEntry { name, path: path.display().to_string(), kind });
        if recursive && meta.is_dir() {
            walk(&path, hidden, recursive, out)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_tilde_is_the_home_and_a_relative_path_is_in_it() {
        let home = Path::new("/home/me");
        assert_eq!(expand("~", home), PathBuf::from("/home/me"));
        assert_eq!(expand("~/", home), PathBuf::from("/home/me"));
        assert_eq!(expand("~/src/", home), PathBuf::from("/home/me/src/"));
        assert_eq!(expand("src", home), PathBuf::from("/home/me/src"));
        assert_eq!(expand("/etc", home), PathBuf::from("/etc"));
    }
}
