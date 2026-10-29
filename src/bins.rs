use pumi_sys::paths::{exe, is_executable};
use std::path::PathBuf;
fn candidate_dirs() -> Vec<PathBuf> {
    let home = crate::home_dir();
    vec![
        home.join(".local/bin"),
        home.join(".cargo/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}
fn find(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|dir| dir.join(name)).find(|path| is_executable(path))
}
pub fn pumid_bin() -> PathBuf {
    if let Some(configured) = std::env::var_os("PUMID_BIN") {
        return PathBuf::from(configured);
    }
    let name = exe("pumid");
    let beside = std::env::current_exe().ok().and_then(|app| app.parent().map(|dir| dir.join(&name)));
    beside.filter(|path| is_executable(path)).or_else(|| find(&name)).unwrap_or_else(|| PathBuf::from(name))
}
/// The name an installed bundle runs as (`Contents/Resources/appname`, which scripts/install.sh
/// writes: `pumi`, or `pumi-dev` for pumi Dev). It wins over an inherited `$PUMI_APPNAME`: `open`
/// from a tab of the other pumi passes that tab's, and pumi Dev would use pumi's server and files.
pub fn adopt_bundled_appname() {
    let Some(file) =
        std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/appname")))
    else {
        return;
    };
    if let Ok(name) = std::fs::read_to_string(file)
        && !name.trim().is_empty()
    {
        unsafe { std::env::set_var(pumi_proto::paths::APPNAME_ENV, name.trim()) };
    }
}
pub fn hydrate_path() {
    let mut dirs: Vec<PathBuf> = candidate_dirs().into_iter().filter(|dir| dir.is_dir()).collect();
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let extra: Vec<PathBuf> = std::env::split_paths(&inherited).filter(|dir| !dirs.contains(dir)).collect();
    dirs.extend(extra);
    if let Ok(joined) = std::env::join_paths(dirs) {
        unsafe { std::env::set_var("PATH", joined) };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_hydration_keeps_what_was_there() {
        let before = std::env::var_os("PATH").unwrap_or_default();
        hydrate_path();
        let after = std::env::var_os("PATH").expect("hydrate_path always sets PATH");
        let after: Vec<PathBuf> = std::env::split_paths(&after).collect();
        for dir in std::env::split_paths(&before) {
            assert!(after.contains(&dir), "{dir:?} was dropped from PATH");
        }
        let shell = exe(if cfg!(windows) { "cmd" } else { "sh" });
        assert!(find(&shell).is_some(), "{shell} is on PATH but was not found");
    }
}
